use nusb::{
    Interface, MaybeFuture,
    transfer::{Bulk, ControlIn, ControlOut, ControlType, In, Recipient},
};
use std::{
    error::Error,
    io::{self, Read},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TIMEOUT: Duration = Duration::from_secs(2);

fn get(iface: &Interface, request: u8, index: u16, length: u16) -> Result<Vec<u8>> {
    Ok(iface
        .control_in(
            ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value: 0,
                index,
                length,
            },
            TIMEOUT,
        )
        .wait()?)
}

fn set(iface: &Interface, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()> {
    iface
        .control_out(
            ControlOut {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value,
                index,
                data,
            },
            TIMEOUT,
        )
        .wait()?;
    Ok(())
}

struct StopRx<'a>(&'a Interface);
impl Drop for StopRx<'_> {
    fn drop(&mut self) {
        if let Err(error) = set(self.0, 1, 0, 0, &[]) {
            eprintln!("Failed to stop RX: {error}");
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|x| x == "--help") {
        println!(
            "mayhem-usb-probe [--rx]\nDefault: read device information. --rx: receive/discard 5 seconds at each of 8, 10 and 20 MS/s, 100 MHz, RF amp and antenna power off. No transmission or flash writes."
        );
        return Ok(());
    }
    if args.iter().any(|x| x != "--rx") {
        return Err("Unknown argument; use --help".into());
    }
    let devices: Vec<_> = nusb::list_devices()
        .wait()?
        .filter(|d| d.vendor_id() == 0x1d50 && d.product_id() == 0x6089)
        .collect();
    if devices.len() != 1 {
        return Err(format!("Expected one HackRF One, found {}", devices.len()).into());
    }
    let device = devices[0].open().wait()?;
    let iface = device.claim_interface(0).wait()?;
    println!(
        "USB API: 0x{:04x}",
        device.device_descriptor().device_version()
    );
    println!("Board ID: {:?}", get(&iface, 14, 0, 1)?);
    let version = get(&iface, 15, 0, 255)?;
    println!(
        "Firmware: {}",
        String::from_utf8_lossy(&version).trim_end_matches('\0')
    );
    if !args.iter().any(|x| x == "--rx") {
        return Ok(());
    }
    for rate in [8_000_000_u32, 10_000_000, 20_000_000] {
        set(&iface, 1, 0, 0, &[])?;
        let _stop = StopRx(&iface);
        set(&iface, 17, 0, 0, &[])?;
        set(&iface, 23, 0, 0, &[])?;
        let mut sample_rate = rate.to_le_bytes().to_vec();
        sample_rate.extend_from_slice(&1_u32.to_le_bytes());
        set(&iface, 6, 0, 0, &sample_rate)?;
        let bandwidth = match rate {
            8_000_000 => 6_000_000_u32,
            10_000_000 => 7_000_000,
            _ => 15_000_000,
        };
        set(&iface, 7, bandwidth as u16, (bandwidth >> 16) as u16, &[])?;
        let mut frequency = 100_u32.to_le_bytes().to_vec();
        frequency.extend_from_slice(&0_u32.to_le_bytes());
        set(&iface, 16, 0, 0, &frequency)?;
        for request in [19, 20] {
            if get(&iface, request, 16, 1)? != [1] {
                return Err("Gain configuration rejected".into());
            }
        }
        let mut reader = iface
            .endpoint::<Bulk, In>(0x81)?
            .reader(256 * 1024)
            .with_num_transfers(16)
            .with_read_timeout(TIMEOUT);
        set(&iface, 1, 1, 0, &[])?;
        let started = Instant::now();
        let mut bytes = 0_u64;
        let mut histogram = [0_u64; 256];
        let mut buffer = vec![0_u8; 256 * 1024];
        while started.elapsed() < Duration::from_secs(5) {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Empty USB read").into());
            }
            bytes += count as u64;
            for &value in &buffer[..count] {
                histogram[value as usize] += 1;
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        // Stop the radio explicitly before releasing the transfer queue.
        set(&iface, 1, 0, 0, &[])?;
        println!(
            "RX target={} MS/s bytes={} elapsed={:.3}s measured={:.3} MS/s distinct_byte_values={}",
            rate / 1_000_000,
            bytes,
            elapsed,
            bytes as f64 / elapsed / 2e6,
            histogram.iter().filter(|&&n| n > 0).count()
        );
    }
    Ok(())
}
