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
}

impl Config {
    pub fn parse() -> Result<Option<Self>> {
        let mut config = Self {
            listen: "127.0.0.1:1234".parse()?,
            settings: Settings::default(),
            serial: None,
            allow_bias_tee: false,
            sessions: 0,
            service: false,
            queue_blocks: 32,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "mayhem_tcp 0.1.0 - HackRF receive-only rtl_tcp proof of concept\n\
                        -a ADDRESS       Bind address (default 127.0.0.1)\n\
                        -p PORT          TCP port (default 1234)\n\
                        -f HZ            Initial frequency (default 100000000)\n\
                        -s HZ            Output rate, 240000..3200000 (default 2048000)\n\
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
                "--agc" => config.settings.auto_gain = true,
                "--digital-agc" => config.settings.digital_agc = true,
                "--service" => config.service = true,
                _ => {
                    let value = args.next().ok_or("Missing option value; use --help")?;
                    match arg.as_str() {
                        "-a" => config.listen.set_ip(value.parse::<IpAddr>()?),
                        "-p" => config.listen.set_port(value.parse()?),
                        "-f" => config.settings.frequency = value.parse()?,
                        "-s" => config.settings.rate = value.parse()?,
                        "-g" => {
                            let gain: f64 = value.parse()?;
                            if !gain.is_finite() || !(0.0..=102.0).contains(&gain) {
                                return Err("Gain must be 0..102 dB".into());
                            }
                            config.settings.gain_tenths = (gain * 10.0).round() as i32;
                        }
                        "--serial" => config.serial = Some(value),
                        "-n" | "--queue-blocks" => config.queue_blocks = value.parse()?,
                        "--sessions" => config.sessions = value.parse()?,
                        _ => return Err(format!("Unknown option {arg}; use --help").into()),
                    }
                }
            }
        }
        config.settings.validate()?;
        validate_queue_blocks(config.queue_blocks)?;
        Ok(Some(config))
    }
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
    fn queue_capacity_is_bounded() {
        for blocks in [1, 32, 1024] {
            assert!(validate_queue_blocks(blocks).is_ok());
        }
        for blocks in [0, 1025, usize::MAX] {
            assert!(validate_queue_blocks(blocks).is_err());
        }
    }
}
