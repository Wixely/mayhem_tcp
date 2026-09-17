use crate::{Result, protocol::Settings};
use std::net::{IpAddr, SocketAddr};

#[derive(Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub settings: Settings,
    pub serial: Option<String>,
    pub allow_bias_tee: bool,
    pub sessions: usize,
    pub service: bool,
    pub queue_blocks: usize,
    pub usb_buffers: usize,
    pub device_index: Option<usize>,
}

impl Config {
    pub fn parse() -> Result<Option<Self>> {
        Self::parse_args(std::env::args().skip(1))
    }

    pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Option<Self>> {
        let mut config = Self {
            listen: "127.0.0.1:1234".parse()?,
            settings: Settings::default(),
            serial: None,
            allow_bias_tee: false,
            sessions: 0,
            service: false,
            queue_blocks: 32,
            usb_buffers: crate::agc::USB_TRANSFERS,
            device_index: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "mayhem_tcp 0.2.0 - HackRF receive-only rtl_tcp proof of concept\n\
                        -a ADDRESS       Bind address (default 127.0.0.1)\n\
                        -p PORT          TCP port (default 1234)\n\
                        -f HZ            Initial frequency (default 100000000)\n\
                        -s HZ            Output rate, 225001..3200000 (default 2048000)\n\
                        -P PPM           Correct tuning and sample clock (-1000..1000)\n\
                        -b BUFFERS       USB transfers, 1..64 (default 16; 0 selects default)\n\
                        -d DEVICE        HackRF index or exact USB serial\n\
                        -T               Start with antenna power on (also permits client control)\n\
                        --offset-tuning  Capture off-centre and digitally recenter\n\
                        --test-mode      Send an incrementing byte counter instead of IQ\n\
                        -n BLOCKS        Output queue capacity, 1..1024 (default 32)\n\
                        -g DB            Initial total LNA/VGA gain, 0..102 (default 32)\n\
                        --agc            Start with analog AGC (client may override)\n\
                        --digital-agc    Start with digital IQ AGC (client may override)\n\
                        --serial SERIAL  Select one HackRF by its USB serial\n\
                        --allow-bias-tee  Allow client antenna-power commands (default blocked)\n\
                        --sessions N     Exit after N sessions (default unlimited)\n\
                        --service        Run under Windows Service Control Manager\n\
                        RF amplifier stays off. No transmit or firmware-writing operations."
                    );
                    return Ok(None);
                }
                "--allow-bias-tee" => config.allow_bias_tee = true,
                "-T" => {
                    config.allow_bias_tee = true;
                    config.settings.bias_tee = true;
                }
                "--offset-tuning" => config.settings.offset_tuning = true,
                "--test-mode" => config.settings.test_mode = true,
                "-D" => {
                    return Err(
                        "HackRF does not have RTL direct-sampling mode; tune normally".into(),
                    );
                }
                "--agc" => config.settings.auto_gain = true,
                "--digital-agc" => config.settings.digital_agc = true,
                "--service" => config.service = true,
                _ => {
                    let value = args.next().ok_or("Missing option value; use --help")?;
                    match arg.as_str() {
                        "-a" => config.listen.set_ip(value.parse::<IpAddr>()?),
                        "-p" => config.listen.set_port(value.parse()?),
                        "-f" => config.settings.frequency = parse_hz(&value)?,
                        "-s" => config.settings.rate = parse_hz(&value)?,
                        "-P" | "--ppm" => config.settings.ppm = value.parse()?,
                        "-b" | "--usb-buffers" => {
                            config.usb_buffers = value.parse()?;
                            if config.usb_buffers == 0 {
                                config.usb_buffers = crate::agc::USB_TRANSFERS;
                            }
                        }
                        "-d" => {
                            if let Ok(index) = value.parse::<usize>() {
                                config.device_index = Some(index);
                                config.serial = None;
                            } else {
                                config.serial = Some(value);
                                config.device_index = None;
                            }
                        }
                        "-g" => {
                            let gain: f64 = value.parse()?;
                            if !gain.is_finite() || !(0.0..=102.0).contains(&gain) {
                                return Err("Gain must be 0..102 dB".into());
                            }
                            config.settings.gain_tenths = (gain * 10.0).round() as i32;
                        }
                        "--serial" => {
                            config.serial = Some(value);
                            config.device_index = None;
                        }
                        "-n" | "--queue-blocks" => config.queue_blocks = value.parse()?,
                        "--sessions" => config.sessions = value.parse()?,
                        _ => return Err(format!("Unknown option {arg}; use --help").into()),
                    }
                }
            }
        }
        config.settings.validate()?;
        validate_queue_blocks(config.queue_blocks)?;
        validate_usb_buffers(config.usb_buffers)?;
        Ok(Some(config))
    }
}

fn parse_hz(value: &str) -> Result<u32> {
    let (digits, multiplier) = match value.as_bytes().last() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1_000.0),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1_000_000.0),
        Some(b'g' | b'G') => (&value[..value.len() - 1], 1_000_000_000.0),
        _ => (value, 1.0),
    };
    let hz = digits.parse::<f64>()? * multiplier;
    if !hz.is_finite() || !(0.0..=u32::MAX as f64).contains(&hz) {
        return Err("Frequency/rate must fit an unsigned 32-bit Hz value".into());
    }
    Ok(hz.round() as u32)
}

pub fn validate_usb_buffers(buffers: usize) -> Result<()> {
    if !(1..=64).contains(&buffers) {
        return Err("USB buffers must be 1..64".into());
    }
    Ok(())
}

pub fn validate_queue_blocks(blocks: usize) -> Result<()> {
    if !(1..=1024).contains(&blocks) {
        return Err("Output queue must contain 1..1024 blocks".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn osmocom_style_options_and_validation() {
        let parse = |args: &[&str]| Config::parse_args(args.iter().map(|s| s.to_string()));
        let config = parse(&[
            "-f",
            "100M",
            "-s",
            "225.001k",
            "-P",
            "-25",
            "-b",
            "8",
            "-n",
            "64",
            "-d",
            "0",
            "--offset-tuning",
            "--test-mode",
            "-T",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(config.settings.frequency, 100_000_000);
        assert_eq!(config.settings.rate, 225_001);
        assert_eq!(config.settings.ppm, -25);
        assert_eq!(config.usb_buffers, 8);
        assert_eq!(config.device_index, Some(0));
        assert!(
            config.settings.offset_tuning
                && config.settings.test_mode
                && config.settings.bias_tee
                && config.allow_bias_tee
        );
        let serial = parse(&["-d", "0", "--serial", "000123", "-b", "0"])
            .unwrap()
            .unwrap();
        assert_eq!(serial.serial.as_deref(), Some("000123"));
        assert_eq!(serial.device_index, None);
        assert_eq!(serial.usb_buffers, 16);
        for args in [
            &["-f", "NaN"][..],
            &["-s", "225k"],
            &["-b", "65"],
            &["-P", "1001"],
            &["-D"],
            &["-f", "5G"],
        ] {
            assert!(parse(args).is_err(), "accepted {args:?}");
        }
    }
    #[test]
    fn queue_capacity_is_bounded() {
        for blocks in [1, 32, 1024] {
            assert!(validate_queue_blocks(blocks).is_ok());
        }
        for blocks in [0, 1025, usize::MAX] {
            assert!(validate_queue_blocks(blocks).is_err());
        }
    }
}
