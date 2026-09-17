use crate::{
    Result,
    agc::{Gains, USB_TRANSFER_BYTES},
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
    #[cfg(target_os = "android")]
    pub fn open(_serial: Option<&str>) -> Result<Self> {
        Err("Android requires an app-granted USB descriptor; use Radio::from_device".into())
    }

    #[cfg(not(target_os = "android"))]
    pub fn open(serial: Option<&str>) -> Result<Self> {
        Self::open_selected(serial, None)
    }

    #[cfg(target_os = "android")]
    pub fn open_selected(_serial: Option<&str>, _index: Option<usize>) -> Result<Self> {
        Self::open(None)
    }

    #[cfg(not(target_os = "android"))]
    pub fn open_selected(serial: Option<&str>, index: Option<usize>) -> Result<Self> {
        let devices: Vec<_> = nusb::list_devices()
            .wait()?
            .filter(|d| {
                d.vendor_id() == 0x1d50
                    && d.product_id() == 0x6089
                    && serial.is_none_or(|s| d.serial_number() == Some(s))
            })
            .collect();
        if index.is_none() && devices.len() != 1 {
            return Err(format!(
                "Expected one available HackRF One, found {}; select HackRF mode or use --serial",
                devices.len()
            )
            .into());
        }
        let device = devices
            .get(index.unwrap_or(0))
            .ok_or("HackRF device index out of range")?
            .open()
            .wait()?;
        Self::from_device(device)
    }

    pub fn from_device(device: Device) -> Result<Self> {
        let descriptor = device.device_descriptor();
        if descriptor.vendor_id() != 0x1d50 || descriptor.product_id() != 0x6089 {
            return Err("USB device is not a HackRF One in HackRF mode".into());
        }
        let interface = device.claim_interface(0).wait()?;
        let api = device.device_descriptor().device_version();
        let radio = Self {
            _device: device,
            interface,
        };
        let firmware = radio.get(15, 0, 255)?;
        crate::diagnostic!(
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
        let (numerator, denominator) = settings.sample_clock();
        let mut data = numerator.to_le_bytes().to_vec();
        data.extend_from_slice(&denominator.to_le_bytes());
        self.set(6, 0, 0, &data)?;
        // Choose the next supported analog bandwidth at/above output rate;
        // digital anti-alias filtering is done before decimation.
        let bandwidth = settings.analog_bandwidth();
        self.set(7, bandwidth as u16, (bandwidth >> 16) as u16, &[])?;
        let frequency = settings.corrected_frequency();
        crate::diagnostic!(
            "Hardware tuning={frequency} Hz clock={numerator}/{denominator} Hz offset={} ppm={}",
            settings.offset_tuning,
            settings.ppm
        );
        let mut data = ((frequency / 1_000_000) as u32).to_le_bytes().to_vec();
        data.extend_from_slice(&((frequency % 1_000_000) as u32).to_le_bytes());
        self.set(16, 0, 0, &data)?;
        let Gains { lna, vga } = settings.initial_gains();
        for (request, gain) in [(19, lna), (20, vga)] {
            if self.get(request, gain, 1)? != [1] {
                return Err("HackRF rejected gain".into());
            }
        }
        crate::diagnostic!(
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
    pub fn reader(&self, transfers: usize) -> Result<impl Read + use<>> {
        Ok(self
            .interface
            .endpoint::<Bulk, In>(0x81)?
            .reader(USB_TRANSFER_BYTES)
            .with_num_transfers(transfers)
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
            crate::diagnostic!("RX cleanup failed: {error}");
        }
        if let Err(error) = self.set(23, 0, 0, &[]) {
            crate::diagnostic!("Antenna-power cleanup failed: {error}");
        }
    }
}
