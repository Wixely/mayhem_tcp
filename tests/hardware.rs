//! Explicit receive-only test: cargo test --release --test hardware -- --ignored --nocapture
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

static HARDWARE_LOCK: Mutex<()> = Mutex::new(());

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn connect(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut greeting = [0; 12];
            if stream.read_exact(&mut greeting).is_ok() {
                assert_eq!(&greeting[..4], b"RTL0");
                assert_eq!(u32::from_be_bytes(greeting[4..8].try_into().unwrap()), 5);
                assert_eq!(u32::from_be_bytes(greeting[8..].try_into().unwrap()), 29);
                return stream;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Server did not accept connection"
        );
        thread::sleep(Duration::from_millis(100));
    }
}
fn command(stream: &mut TcpStream, id: u8, value: u32) {
    let mut data = vec![id];
    data.extend_from_slice(&value.to_be_bytes());
    stream.write_all(&data).unwrap();
}
fn receive(stream: &mut TcpStream, seconds: f64) -> (u64, f64, usize) {
    let started = Instant::now();
    let mut bytes = 0;
    let mut buffer = vec![0; 64 * 1024];
    let mut seen = [false; 256];
    while started.elapsed().as_secs_f64() < seconds {
        let count = stream.read(&mut buffer).expect("IQ read failed");
        assert!(count > 0, "Unexpected disconnect");
        bytes += count as u64;
        for &b in &buffer[..count] {
            seen[b as usize] = true;
        }
    }
    (
        bytes,
        started.elapsed().as_secs_f64(),
        seen.into_iter().filter(|x| *x).count(),
    )
}
fn measure(stream: &mut TcpStream, rate: u32) {
    command(stream, 2, rate);
    receive(stream, 0.6); // Drain prior-rate TCP bytes and filter startup.
    let (bytes, seconds, distinct) = receive(stream, 3.0);
    let measured = bytes as f64 / seconds / 2.0;
    println!(
        "rate={rate} measured={measured:.0} S/s bytes={bytes} seconds={seconds:.3} distinct={distinct}"
    );
    assert!(
        (measured / rate as f64 - 1.0).abs() < 0.03,
        "Incorrect output sample rate"
    );
    assert!(distinct > 1, "Constant IQ");
}
fn expect_closed(stream: &mut TcpStream) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut buffer = [0; 65536];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                return;
            }
            Err(e) => panic!("Expected disconnect, got {e}"),
            Ok(_) => assert!(
                Instant::now() < deadline,
                "Server kept streaming after stalled reader"
            ),
        }
    }
}

#[test]
#[ignore = "Requires an exclusively available HackRF; performs RX at 100/101 MHz"]
fn live_protocol_rates_reconnect_and_backpressure() {
    let _exclusive = HARDWARE_LOCK.lock().unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_mayhem_tcp"))
            .args(["-p", &port.to_string(), "--sessions", "5", "-n", "64"])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    {
        let mut stream = connect(port);
        // A command split across the server's 200ms read timeout.
        stream.write_all(&[1, 0x05]).unwrap();
        thread::sleep(Duration::from_millis(250));
        stream.write_all(&[0xf5, 0xe1, 0]).unwrap();
        // Several commands coalesced into one write, including compatibility no-ops.
        stream
            .write_all(&[3, 0, 0, 0, 1, 8, 0, 0, 0, 0, 13, 0, 0, 0, 20, 9, 0, 0, 0, 0])
            .unwrap();
        for rate in [2_000_000, 2_048_000, 2_400_000, 250_000, 240_000] {
            measure(&mut stream, rate);
        }
        command(&mut stream, 1, 101_000_000);
        receive(&mut stream, 0.3);
    }
    {
        let mut stream = connect(port);
        measure(&mut stream, 2_048_000);
    }
    {
        let mut stream = connect(port);
        command(&mut stream, 2, 0);
        command(&mut stream, 3, 2);
        command(&mut stream, 8, 2);
        // Rejected commands must leave the initial rate and connection intact.
        receive(&mut stream, 0.6);
        let (bytes, seconds, _) = receive(&mut stream, 3.0);
        assert!((bytes as f64 / seconds / 2.0 / 2_048_000.0 - 1.0).abs() < 0.03);
        measure(&mut stream, 240_000);
        println!("Invalid commands ignored; later valid rate accepted");
    }
    {
        let mut stream = connect(port);
        thread::sleep(Duration::from_secs(5));
        expect_closed(&mut stream);
        println!("Stalled reader disconnected correctly");
    }
    {
        let mut stream = connect(port);
        measure(&mut stream, 2_000_000);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "Server failed to exit cleanly");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "Requires an exclusively available HackRF; exercises live analog gain at 100/101 MHz"]
fn live_agc_and_manual_restore_without_gain_restarts() {
    let _exclusive = HARDWARE_LOCK.lock().unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_mayhem_tcp"))
            .args(["-p", &port.to_string(), "--sessions", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stderr = server.0.stderr.take().unwrap();
    let logger = thread::spawn(move || {
        let mut log = String::new();
        stderr.read_to_string(&mut log).unwrap();
        log
    });
    {
        let mut stream = connect(port);
        command(&mut stream, 8, 1);
        command(&mut stream, 3, 0);
        measure(&mut stream, 2_048_000);
        // Remember a manual gain while automatic control continues.
        command(&mut stream, 4, 460);
        receive(&mut stream, 0.5);
        command(&mut stream, 3, 1);
        receive(&mut stream, 0.5);
        command(&mut stream, 8, 0);
        receive(&mut stream, 0.5);
        command(&mut stream, 4, 240);
        receive(&mut stream, 0.5);
        command(&mut stream, 3, 0);
        command(&mut stream, 8, 1);
        receive(&mut stream, 0.5);
        command(&mut stream, 1, 101_000_000);
        measure(&mut stream, 2_048_000);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "Server failed to stop");
        thread::sleep(Duration::from_millis(50));
    }
    let log = logger.join().unwrap();
    println!("{log}");
    assert_eq!(
        log.matches("Configured frequency=").count(),
        2,
        "Only start and retune may restart RX"
    );
    assert!(log.contains("Live gain mode=auto"));
    assert_eq!(log.matches("Digital AGC=true").count(), 3);
    assert_eq!(log.matches("Digital AGC=false").count(), 2);
    assert!(
        log.contains("Live gain mode=manual LNA=40 VGA=6"),
        "Manual setting was not restored"
    );
    assert!(log.contains("Live gain mode=manual LNA=24 VGA=0"));
    assert!(!log.contains("Session ended:"), "Unexpected stream error");
    // Actual AGC adjustments depend on the ambient RF level: synthetic tests
    // assert the controller's responses, while this checks live control and RX.
}
