use crate::{
    Result,
    config::Config,
    dsp::Decimator,
    protocol::{self, Command},
    radio::Radio,
};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

fn stopped(local: &AtomicBool, global: &AtomicBool) -> bool {
    local.load(Ordering::Relaxed) || global.load(Ordering::Relaxed)
}

pub fn run(config: Config, shutdown: Arc<AtomicBool>) -> Result<()> {
    let listener = TcpListener::bind(config.listen)?;
    listener.set_nonblocking(true)?;
    eprintln!(
        "mayhem_tcp listening on {} (one RX client; Ctrl+C to stop)",
        listener.local_addr()?
    );
    let mut active: Option<thread::JoinHandle<Result<()>>> = None;
    let mut sessions = 0;
    while !shutdown.load(Ordering::Relaxed) {
        if active.as_ref().is_some_and(|h| h.is_finished()) {
            match active.take().unwrap().join() {
                Ok(Ok(())) => eprintln!("Session closed; ready for another client"),
                Ok(Err(error)) => eprintln!("Session ended: {error}"),
                Err(_) => return Err("Session thread panicked".into()),
            }
            if config.sessions != 0 && sessions >= config.sessions {
                break;
            }
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if active.is_some() {
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                sessions += 1;
                let options = config.clone();
                let shutdown = shutdown.clone();
                active = Some(thread::spawn(move || session(stream, options, &shutdown)));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(error) => {
                shutdown.store(true, Ordering::Relaxed);
                if let Some(handle) = active.take() {
                    let _ = handle.join();
                }
                return Err(error.into());
            }
        }
    }
    if let Some(handle) = active {
        let _ = handle.join();
    }
    eprintln!("mayhem_tcp stopped");
    Ok(())
}

struct Block {
    generation: u64,
    bytes: Vec<u8>,
}

fn session(mut stream: TcpStream, config: Config, global: &AtomicBool) -> Result<()> {
    // Windows accepted sockets inherit the nonblocking listener's mode.
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let radio = Radio::open(config.serial.as_deref())?;
    stream.write_all(&protocol::greeting())?;
    let reader = stream.try_clone()?;
    let writer = stream.try_clone()?;
    let local = AtomicBool::new(false);
    let generation = AtomicU64::new(0);
    let (commands_tx, commands_rx) = mpsc::sync_channel(64);
    let (data_tx, data_rx) = mpsc::sync_channel::<Block>(32);
    thread::scope(|scope| {
        scope.spawn(|| {
            if let Err(error) = read_commands(reader, commands_tx, &local, global) {
                eprintln!("Command connection ended: {error}");
            }
            local.store(true, Ordering::Relaxed);
        });
        scope.spawn(|| {
            if let Err(error) = write_samples(writer, data_rx, &generation, &local, global) {
                eprintln!("Sample connection ended: {error}");
            }
            local.store(true, Ordering::Relaxed);
        });
        let result = capture(
            &radio,
            &config,
            commands_rx,
            data_tx,
            &generation,
            &local,
            global,
        );
        local.store(true, Ordering::Relaxed);
        let _ = stream.shutdown(Shutdown::Both);
        result
    })
}

// Accumulate a command explicitly: read_exact would lose partial-command state
// if a timeout occurred between its bytes.
pub fn read_commands(
    mut reader: impl Read,
    sender: SyncSender<Command>,
    local: &AtomicBool,
    global: &AtomicBool,
) -> Result<()> {
    let mut packet = [0_u8; 5];
    let mut filled = 0;
    while !stopped(local, global) {
        match reader.read(&mut packet[filled..]) {
            Ok(0) => {
                return if filled == 0 {
                    Ok(())
                } else {
                    Err("Truncated command".into())
                };
            }
            Ok(count) => {
                filled += count;
                if filled == 5 {
                    sender.try_send(Command::from(packet))?;
                    filled = 0;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn write_samples(
    mut writer: TcpStream,
    receiver: Receiver<Block>,
    generation: &AtomicU64,
    local: &AtomicBool,
    global: &AtomicBool,
) -> Result<()> {
    while !stopped(local, global) {
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(block) if block.generation == generation.load(Ordering::Acquire) => {
                writer.write_all(&block.bytes)?
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn capture(
    radio: &Radio,
    config: &Config,
    commands: Receiver<Command>,
    data: SyncSender<Block>,
    generation: &AtomicU64,
    local: &AtomicBool,
    global: &AtomicBool,
) -> Result<()> {
    let mut settings = config.settings.clone();
    let mut input = vec![0_u8; 256 * 1024];
    let started = Instant::now();
    let mut input_bytes = 0_u64;
    let mut output_bytes = 0_u64;
    while !stopped(local, global) {
        radio.configure(&settings)?;
        let current_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
        let (_, divisor) = settings.hardware_rate();
        let mut filter = Decimator::new(divisor);
        let mut reader = radio.reader()?;
        radio.start()?;
        // Discard the first transfer after each restart to allow RF settling.
        let mut settling = true;
        while !stopped(local, global) {
            let previous = settings.clone();
            // Bound work per USB block even if the client floods commands.
            for command in commands.try_iter().take(64) {
                if let Some(warning) = settings.command(command, config.allow_bias_tee)? {
                    eprintln!("Command 0x{:02x}: {warning}", command.id);
                }
            }
            if previous != settings {
                break;
            }
            let count = reader.read(&mut input)?;
            if count == 0 || count % 2 != 0 {
                return Err("Empty or misaligned HackRF IQ transfer".into());
            }
            input_bytes += count as u64;
            if settling {
                settling = false;
                continue;
            }
            let mut output = Vec::with_capacity(count / divisor + 2);
            filter.process(&input[..count], &mut output);
            output_bytes += output.len() as u64;
            data.try_send(Block {
                generation: current_generation,
                bytes: output,
            })
            .map_err(
                |_| "Output queue full or disconnected; ending stream rather than dropping IQ",
            )?;
        }
        // Retire the old queue before the next configuration to avoid old-rate data.
        radio.stop()?;
        drop(reader);
    }
    eprintln!(
        "RX session: USB bytes={input_bytes} output bytes={output_bytes} elapsed={:.3}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fragmented {
        bytes: Vec<u8>,
        index: usize,
        timeout: bool,
    }
    impl Read for Fragmented {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.timeout = !self.timeout;
            if self.timeout {
                return Err(io::ErrorKind::TimedOut.into());
            }
            if self.index == self.bytes.len() {
                return Ok(0);
            }
            out[0] = self.bytes[self.index];
            self.index += 1;
            Ok(1)
        }
    }
    #[test]
    fn fragmented_commands_survive_timeouts_and_coalescing() {
        let bytes = [1, 0x05, 0xf5, 0xe1, 0, 2, 0, 0x1e, 0x84, 0x80];
        for reader in [
            Box::new(io::Cursor::new(bytes.to_vec())) as Box<dyn Read>,
            Box::new(Fragmented {
                bytes: bytes.to_vec(),
                index: 0,
                timeout: false,
            }),
        ] {
            let (tx, rx) = mpsc::sync_channel(4);
            let stop = AtomicBool::new(false);
            read_commands(reader, tx, &stop, &stop).unwrap();
            let commands: Vec<_> = rx.try_iter().collect();
            assert_eq!(commands.len(), 2);
            assert_eq!(commands[0].value, 100_000_000);
            assert_eq!(commands[1].value, 2_000_000);
        }
    }
    #[test]
    fn truncated_and_flooded_commands_fail() {
        let stop = AtomicBool::new(false);
        let (tx, _rx) = mpsc::sync_channel(1);
        assert!(read_commands(io::Cursor::new([1, 2]), tx, &stop, &stop).is_err());
        let (tx, _rx) = mpsc::sync_channel(1);
        assert!(read_commands(io::Cursor::new([0; 10]), tx, &stop, &stop).is_err());
    }
}
