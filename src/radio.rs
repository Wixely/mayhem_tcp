use crate::{
    Result,
    agc::{Gains, USB_TRANSFER_BYTES, USB_TRANSFERS},
    protocol::Settings,
};
use nusb::{
    Device, Interface, MaybeFuture,
    transfer::{Bulk, ControlIn, ControlOut, ControlType, In, Recipient},
};
use std::{io::Read, time::Duration};

pub struct Radio {
    _device: Device,
    interface: Interface,
}
impl Radio {
    pub fn open(serial: Option<&str>) -> Result<Self> {
        let devices: Vec<_> = nusb::list_devices()
            .wait()?
            .filter(|d| {
                d.vendor_id() == 0x1d50
                    && d.product_id() == 0x6089
                    && serial.is_none_or(|s| d.serial_number() == Some(s))
            })
            .collect();
        if devices.len() != 1 {
            return Err(format!(
                "Expected one available HackRF One, found {}; select HackRF mode or use --serial",
                devices.len()
            )
            .into());
        }
        let device = devices[0].open().wait()?;
        let interface = device.claim_interface(0).wait()?;
        let api = device.device_descriptor().device_version();
        let radio = Self {
            _device: device,
            interface,
        };
        let firmware = radio.get(15, 0, 255)?;
        eprintln!(
            "HackRF firmware={} USB API=0x{api:04x}",
            String::from_utf8_lossy(&firmware).trim_end_matches('\0')
        );
        Ok(radio)
    }
    fn get(&self, request: u8, index: u16, length: u16) -> Result<Vec<u8>> {
        Ok(self
            .interface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value: 0,
                    index,
                    length,
                },
                Duration::from_secs(2),
            )
            .wait()?)
    }
    fn set(&self, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()> {
        self.interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value,
                    index,
                    data,
                },
                Duration::from_secs(2),
            )
            .wait()?;
        Ok(())
    }
    pub fn configure(&self, settings: &Settings) -> Result<()> {
        self.stop()?;
        self.set(17, 0, 0, &[])?; // RF amplifier off.
        self.set(23, settings.bias_tee as u16, 0, &[])?;
        let (rate, divisor) = settings.hardware_rate();
        let mut data = rate.to_le_bytes().to_vec();
        data.extend_from_slice(&1_u32.to_le_bytes());
        self.set(6, 0, 0, &data)?;
        // Choose the next supported analog bandwidth at/above output rate;
        // digital anti-alias filtering is done before decimation.
        let bandwidth = [1_750_000_u32, 2_500_000, 3_500_000]
            .into_iter()
            .find(|&b| b >= settings.rate)
            .unwrap();
        self.set(7, bandwidth as u16, (bandwidth >> 16) as u16, &[])?;
        let frequency = settings.corrected_frequency();
        let mut data = ((frequency / 1_000_000) as u32).to_le_bytes().to_vec();
        data.extend_from_slice(&((frequency % 1_000_000) as u32).to_le_bytes());
        self.set(16, 0, 0, &data)?;
        let Gains { lna, vga } = settings.initial_gains();
        for (request, gain) in [(19, lna), (20, vga)] {
            if self.get(request, gain, 1)? != [1] {
                return Err("HackRF rejected gain".into());
            }
        }
        eprintln!(
            "Configured frequency={} Hz output={} S/s USB={} S/s decimation={} LNA={} VGA={} bias={} auto_gain={}",
            settings.frequency,
            settings.rate,
            rate,
            divisor,
            lna,
            vga,
            settings.bias_tee,
            settings.auto_gain
        );
        Ok(())
    }
    pub fn reader(&self) -> Result<impl Read + use<>> {
        Ok(self
            .interface
            .endpoint::<Bulk, In>(0x81)?
            .reader(USB_TRANSFER_BYTES)
            .with_num_transfers(USB_TRANSFERS)
            .with_read_timeout(Duration::from_millis(500)))
    }
    pub fn start(&self) -> Result<()> {
        self.set(1, 1, 0, &[])
    }
    pub fn set_gains_live(&self, previous: Gains, gains: Gains) -> Result<()> {
        for (request, gain) in gains.writes_from(previous) {
            if self.get(request, gain, 1)? != [1] {
                return Err("HackRF rejected live gain update".into());
            }
        }
        Ok(())
    }
    pub fn stop(&self) -> Result<()> {
        self.set(1, 0, 0, &[])
    }
}
impl Drop for Radio {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("RX cleanup failed: {error}");
        }
        if let Err(error) = self.set(23, 0, 0, &[]) {
            eprintln!("Antenna-power cleanup failed: {error}");
        }
    }
}
