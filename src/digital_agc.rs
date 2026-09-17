// Software complex-envelope AGC. One positive gain preserves I/Q phase.
// f64 state keeps the release accurate even at multi-megasample rates.
pub struct DigitalAgc {
    envelope: f64,
    decay: f64,
}

impl DigitalAgc {
    pub fn new(rate: u32) -> Self {
        assert!(rate > 0);
        Self {
            envelope: 64.0,
            decay: (-1.0 / (0.2 * rate as f64)).exp(),
        }
    }

    pub fn process(&mut self, iq: [f32; 2]) -> [f32; 2] {
        let magnitude = ((iq[0] as f64).powi(2) + (iq[1] as f64).powi(2)).sqrt();
        // Hold on exact silence: no gain wind-up during gaps or disconnected input.
        if magnitude > 0.0 {
            self.envelope = magnitude.max(self.envelope * self.decay).max(1.0);
        }
        let gain = (64.0 / self.envelope).min(64.0) as f32;
        [iq[0] * gain, iq[1] * gain]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_signal_release_is_rate_independent_and_preserves_phase() {
        for rate in [250_000, 2_048_000, 3_200_000] {
            let mut agc = DigitalAgc::new(rate);
            for _ in 0..rate / 5 {
                agc.process([0.3, 0.4]);
            }
            let out = agc.process([0.3, 0.4]);
            assert!((out[0] / 0.3 - std::f32::consts::E).abs() < 0.001);
            for _ in 0..rate {
                agc.process([0.3, 0.4]);
            }
            let out = agc.process([0.3, 0.4]);
            assert!((out[0] - 19.2).abs() < 0.001);
            assert!((out[1] - 25.6).abs() < 0.001);
        }
    }

    #[test]
    fn sudden_overload_at_maximum_gain_is_limited_without_phase_error() {
        let mut agc = DigitalAgc::new(250_000);
        for _ in 0..250_000 {
            agc.process([0.1, 0.0]);
        }
        let out = agc.process([-128.0, 127.0]);
        assert!((out[0].hypot(out[1]) - 64.0).abs() < 0.001);
        assert!((out[0] / out[1] + 128.0 / 127.0).abs() < 0.0001);
        let weak = agc.process([0.1, 0.0]);
        assert!(weak[0] < 0.1, "Gain must recover slowly after overload");
    }

    #[test]
    fn silence_holds_gain_and_dc_cannot_overflow() {
        let mut agc = DigitalAgc::new(250_000);
        for _ in 0..250_000 {
            assert_eq!(agc.process([0.0, 0.0]), [0.0, 0.0]);
        }
        assert!((agc.process([1.0, 0.0])[0] - 1.0).abs() < 0.001);
        for _ in 0..250_000 {
            let out = agc.process([50.0, 50.0]);
            assert!(out[0] <= 64.0 && out[1] <= 64.0);
        }
    }
}
