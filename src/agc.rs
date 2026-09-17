//! Host-controlled analog AGC, using raw signed IQ before decimation.
//! Timing is measured in input samples so USB chunk sizes do not set the speed.

pub const USB_TRANSFER_BYTES: usize = 256 * 1024;
pub const USB_TRANSFERS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gains {
    pub lna: u16,
    pub vga: u16,
}
impl Gains {
    pub fn total(self) -> u16 {
        self.lna + self.vga
    }
    pub fn balanced(total: u16) -> Self {
        let total = (total.min(102) / 2) * 2;
        let lna = ((total + 8) / 16 * 8).min(40).min(total / 8 * 8);
        Self {
            lna,
            vga: total - lna,
        }
    }

    /// Apply decreases first when moving gain between stages. The intermediate
    /// total cannot exceed the larger of the old/new totals.
    pub fn writes_from(self, previous: Self) -> Vec<(u8, u16)> {
        let changes = [(19, previous.lna, self.lna), (20, previous.vga, self.vga)];
        changes
            .iter()
            .filter(|(_, old, new)| new < old)
            .chain(changes.iter().filter(|(_, old, new)| new > old))
            .map(|&(request, _, gain)| (request, gain))
            .collect()
    }
}

#[derive(Default)]
struct Meter {
    count: u64,
    sum: [i64; 2],
    squares: u64,
    peak: u16,
    near_clip: u64,
}
impl Meter {
    fn sample(&mut self, pair: &[u8]) {
        self.count += 1;
        for (channel, &byte) in pair.iter().enumerate() {
            let value = byte as i8 as i64;
            self.sum[channel] += value;
            self.squares += (value * value) as u64;
            self.peak = self.peak.max(value.unsigned_abs() as u16);
            self.near_clip += u64::from(value.unsigned_abs() >= 120);
        }
    }
    fn rms(&self) -> f64 {
        let n = self.count as f64;
        let dc_power = (self.sum[0] as f64 / n).powi(2) + (self.sum[1] as f64 / n).powi(2);
        ((self.squares as f64 / n - dc_power).max(0.0) / 2.0).sqrt()
    }
}

pub struct Adjustment {
    pub gains: Gains,
    pub rms: f64,
    pub peak: u16,
    pub reason: &'static str,
}

pub struct Controller {
    rate: u64,
    gains: Gains,
    meter: Meter,
    cooldown: u64,
    hang: u64,
    low: u64,
    usb_transfers: usize,
}
impl Controller {
    pub fn new(rate: u32, gains: Gains) -> Self {
        Self::with_usb_transfers(rate, gains, USB_TRANSFERS)
    }
    pub fn with_usb_transfers(rate: u32, gains: Gains, usb_transfers: usize) -> Self {
        let mut agc = Self {
            rate: rate as u64,
            gains,
            meter: Meter::default(),
            cooldown: 0,
            hang: 0,
            low: 0,
            usb_transfers,
        };
        agc.cooldown = agc.settling_samples();
        agc
    }
    fn settling_samples(&self) -> u64 {
        // Old-gain IQ can still fill the USB queue after a live control write.
        (USB_TRANSFER_BYTES * self.usb_transfers / 2) as u64 + self.rate / 50
    }
    pub fn observe(&mut self, raw: &[u8]) -> Option<Adjustment> {
        assert_eq!(raw.len() % 2, 0);
        for pair in raw.chunks_exact(2) {
            self.hang = self.hang.saturating_sub(1);
            if self.cooldown > 0 {
                self.cooldown -= 1;
                continue;
            }
            self.meter.sample(pair);
            if self.meter.count < self.rate / 50 {
                continue;
            } // 20 ms window.
            let meter = std::mem::take(&mut self.meter);
            let rms = meter.rms();
            let overload =
                meter.peak >= 126 || meter.near_clip * 10_000 > meter.count * 2 || rms > 32.0;
            let (total, reason) = if overload {
                self.low = 0;
                self.hang = self.rate * 2;
                (self.gains.total().saturating_sub(8), "overload")
            } else if rms > 24.0 {
                self.low = 0;
                self.hang = self.rate * 2;
                (self.gains.total().saturating_sub(2), "high level")
            } else if rms >= 12.0 || meter.peak >= 80 {
                self.low = 0;
                self.hang = self.rate * 2;
                continue;
            } else if rms < 0.01 || self.hang != 0 {
                // Reject constant input, but allow weak quantized signals
                // whose RMS is substantially below one ADC count.
                self.low = 0;
                continue;
            } else {
                self.low += meter.count;
                if self.low < self.rate / 2 {
                    continue;
                } // 500 ms sustained low level.
                self.low = 0;
                ((self.gains.total() + 2).min(102), "low level")
            };
            let gains = Gains::balanced(total);
            if gains == self.gains {
                continue;
            }
            self.gains = gains;
            self.cooldown = self.settling_samples();
            return Some(Adjustment {
                gains,
                rms,
                peak: meter.peak,
                reason,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RATE: u32 = 8_000_000;
    fn feed(agc: &mut Controller, amplitude: i8, samples: usize) -> Vec<Adjustment> {
        let block = [
            amplitude as u8,
            amplitude as u8,
            (-amplitude) as u8,
            (-amplitude) as u8,
        ]
        .repeat(2048);
        let mut actions = vec![];
        let mut remaining = samples;
        while remaining > 0 {
            let n = remaining.min(block.len() / 2);
            if let Some(action) = agc.observe(&block[..n * 2]) {
                actions.push(action);
            }
            remaining -= n;
        }
        actions
    }
    #[test]
    fn larger_usb_queue_defers_agc_until_old_samples_are_drained() {
        let mut small = Controller::with_usb_transfers(RATE, Gains::balanced(48), 1);
        let mut large = Controller::with_usb_transfers(RATE, Gains::balanced(48), 64);
        let samples = USB_TRANSFER_BYTES / 2 + RATE as usize / 25 + 4096;
        assert!(!feed(&mut small, 127, samples).is_empty());
        assert!(feed(&mut large, 127, samples).is_empty());
    }
    #[test]
    fn fast_attack_slow_recovery_and_settling() {
        let mut agc = Controller::new(RATE, Gains::balanced(48));
        assert!(feed(&mut agc, 127, 2_097_152).is_empty()); // Queued old-gain data.
        let actions = feed(&mut agc, 127, 320_000);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].gains.total(), 40);
        assert!(feed(&mut agc, 2, RATE as usize).is_empty()); // Burst hang.
        let actions = feed(&mut agc, 2, RATE as usize * 2);
        assert!(!actions.is_empty());
        assert_eq!(actions[0].gains.total(), 42);
    }
    #[test]
    fn deadband_dc_and_bounds() {
        let mut agc = Controller::new(RATE, Gains::balanced(32));
        assert!(feed(&mut agc, 18, RATE as usize).is_empty());
        // A constant offset has no AC power; never wind up on it.
        let dc = [50_u8, (-40_i8) as u8].repeat(131072);
        for _ in 0..100 {
            assert!(agc.observe(&dc).is_none());
        }
        let mut minimum = Controller::new(RATE, Gains::balanced(0));
        assert!(feed(&mut minimum, 127, RATE as usize).is_empty());
        let mut maximum = Controller::new(RATE, Gains::balanced(102));
        assert!(feed(&mut maximum, 2, RATE as usize).is_empty());
    }
    #[test]
    fn balanced_gain_and_safe_write_order() {
        for total in (0..=102).step_by(2) {
            let gain = Gains::balanced(total);
            assert_eq!(gain.total(), total);
            assert!(gain.lna <= 40 && gain.lna % 8 == 0 && gain.vga <= 62 && gain.vga % 2 == 0);
            for old_total in (0..=102).step_by(2) {
                let mut current = Gains::balanced(old_total);
                for (request, value) in gain.writes_from(current) {
                    if request == 19 {
                        current.lna = value;
                    } else {
                        current.vga = value;
                    }
                    assert!(current.total() <= old_total.max(total));
                }
                assert_eq!(current, gain);
            }
        }
        assert_eq!(Gains::balanced(32), Gains { lna: 16, vga: 16 });
    }
    #[test]
    fn sparse_sub_count_signal_can_raise_gain() {
        let mut agc = Controller::new(RATE, Gains::balanced(32));
        let mut block = vec![0; 8000];
        for pair in block.chunks_exact_mut(200) {
            pair[0] = 1;
            pair[1] = 255;
        }
        let mut changed = false;
        for _ in 0..2000 {
            if let Some(action) = agc.observe(&block) {
                assert!(action.rms > 0.01 && action.rms < 0.5);
                assert_eq!(action.gains.total(), 34);
                changed = true;
                break;
            }
        }
        assert!(
            changed,
            "Weak nonconstant samples must not be mistaken for no input"
        );
    }
    #[test]
    fn closed_loop_converges_without_hunting() {
        let mut agc = Controller::new(RATE, Gains::balanced(64));
        let mut gain = 64;
        let mut actions = 0;
        for _ in 0..300 {
            // Constant RF input: -40 dB relative to the desired ADC amplitude.
            let amplitude = (18.0 * 10_f64.powf((gain as f64 - 40.0) / 20.0))
                .round()
                .clamp(1.0, 127.0) as i8;
            for action in feed(&mut agc, amplitude, 160_000) {
                gain = action.gains.total();
                actions += 1;
            }
        }
        assert!((38..=42).contains(&gain), "gain={gain}");
        assert!(actions < 10);
        let amplitude = (18.0 * 10_f64.powf((gain as f64 - 40.0) / 20.0)).round() as i8;
        assert!(feed(&mut agc, amplitude, RATE as usize).is_empty());
    }
}
