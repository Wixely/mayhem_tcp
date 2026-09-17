use crate::Result;

// R820T compatibility profile: gain values in tenths of a dB, as used by
// librtlsdr clients. These values select an approximate HackRF total gain.
pub const GAINS: [i32; 29] = [
    0, 9, 14, 27, 37, 77, 87, 125, 144, 157, 166, 197, 207, 229, 254, 280, 297, 328, 338, 364, 372,
    386, 402, 421, 434, 439, 445, 480, 496,
];

pub fn greeting() -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..4].copy_from_slice(b"RTL0");
    bytes[4..8].copy_from_slice(&5_u32.to_be_bytes()); // R820T emulation, not hardware identification.
    bytes[8..].copy_from_slice(&(GAINS.len() as u32).to_be_bytes());
    bytes
}

#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub id: u8,
    pub value: u32,
}
impl From<[u8; 5]> for Command {
    fn from(bytes: [u8; 5]) -> Self {
        Self {
            id: bytes[0],
            value: u32::from_be_bytes(bytes[1..].try_into().unwrap()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub frequency: u32,
    pub rate: u32,
    pub gain_tenths: i32,
    pub ppm: i32,
    pub bias_tee: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            frequency: 100_000_000,
            rate: 2_048_000,
            gain_tenths: 320,
            ppm: 0,
            bias_tee: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.frequency < 1_000_000 {
            return Err("Frequency must be 1 MHz..4294967295 Hz".into());
        }
        if !(250_000..=3_200_000).contains(&self.rate) {
            return Err("Output rate must be 250000..3200000 Hz".into());
        }
        if !(0..=1020).contains(&self.gain_tenths) {
            return Err("Gain must be 0..102 dB".into());
        }
        if !(-1000..=1000).contains(&self.ppm) {
            return Err("Frequency correction must be -1000..1000 ppm".into());
        }
        Ok(())
    }

    pub fn hardware_rate(&self) -> (u32, usize) {
        let mut rate = self.rate;
        let mut divisor = 1;
        while rate < 8_000_000 {
            rate *= 2;
            divisor *= 2;
        }
        (rate, divisor)
    }

    pub fn corrected_frequency(&self) -> u64 {
        // RTL-style ppm denotes clock error: a fast clock requires a lower
        // nominal tuning request. This corrects tuning, not ADC sample timing.
        (self.frequency as f64 / (1.0 + self.ppm as f64 / 1_000_000.0)).round() as u64
    }

    pub fn gains(&self) -> (u16, u16) {
        let total = ((self.gain_tenths + 10) / 20 * 2).clamp(0, 102) as u16;
        let lna = (total / 8 * 8).min(40);
        (lna, total - lna)
    }

    pub fn command(&mut self, command: Command, allow_bias: bool) -> Result<Option<&'static str>> {
        let mut next = self.clone();
        match command.id {
            0x01 => next.frequency = command.value,
            0x02 => next.rate = command.value,
            0x03 | 0x08 => return Ok(Some("AGC is not emulated; manual gain remains active")),
            0x04 => next.gain_tenths = command.value as i32,
            0x05 => next.ppm = command.value as i32,
            0x0d => {
                next.gain_tenths = *GAINS
                    .get(command.value as usize)
                    .ok_or("Invalid tuner gain index")?
            }
            0x0e => {
                if command.value > 1 {
                    return Err("Bias tee value must be 0 or 1".into());
                }
                if command.value == 1 && !allow_bias {
                    return Ok(Some(
                        "Antenna power blocked; use --allow-bias-tee to opt in",
                    ));
                }
                next.bias_tee = command.value != 0;
            }
            _ => return Ok(Some("Unsupported RTL-specific command ignored")),
        }
        next.validate()?;
        *self = next;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_profile_and_signed_correction() {
        assert_eq!(greeting(), *b"RTL0\0\0\0\x05\0\0\0\x1d");
        let mut settings = Settings::default();
        settings
            .command(Command::from([5, 255, 255, 255, 246]), false)
            .unwrap();
        assert_eq!(settings.corrected_frequency(), 100_001_000);
        settings
            .command(Command { id: 13, value: 28 }, false)
            .unwrap();
        assert_eq!(settings.gains(), (40, 10));
    }
    #[test]
    fn invalid_commands_leave_settings_unchanged() {
        let mut settings = Settings::default();
        for (id, value) in [
            (2, 0),
            (2, u32::MAX),
            (1, 0),
            (13, 29),
            (4, u32::MAX),
            (5, 1001),
        ] {
            assert!(settings.command(Command { id, value }, false).is_err());
            assert_eq!(settings, Settings::default());
        }
        settings
            .command(Command { id: 14, value: 1 }, false)
            .unwrap();
        assert!(!settings.bias_tee);
    }
    #[test]
    fn rates_and_gains_stay_in_hardware_ranges() {
        for rate in [
            250_000, 1_024_000, 1_536_000, 2_000_000, 2_048_000, 2_400_000, 3_200_000,
        ] {
            let settings = Settings {
                rate,
                ..Settings::default()
            };
            let (hardware, divisor) = settings.hardware_rate();
            assert!((8_000_000..=20_000_000).contains(&hardware));
            assert_eq!(hardware, rate * divisor as u32);
        }
        for gain in 0..=1020 {
            let (lna, vga) = Settings {
                gain_tenths: gain,
                ..Settings::default()
            }
            .gains();
            assert!(lna <= 40 && lna % 8 == 0 && vga <= 62 && vga % 2 == 0);
        }
    }
}
