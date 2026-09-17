use std::f64::consts::PI;

// Symmetric Blackman-windowed sinc, evaluated only at retained output samples.
// 64 taps per decimation factor: cutoff 0.4*output_rate, transition toward
// output Nyquist. Mirrored history keeps the dot product contiguous.
pub struct Decimator {
    divisor: usize,
    phase: usize,
    taps: Vec<f32>,
    history: Vec<[f32; 2]>,
    cursor: usize,
}

impl Decimator {
    pub fn new(divisor: usize) -> Self {
        assert!(divisor.is_power_of_two() && divisor <= 32);
        let length = 64 * divisor + 1;
        let cutoff = 0.4 / divisor as f64;
        let mut taps: Vec<f32> = (0..length)
            .map(|i| {
                let x = i as f64 - (length - 1) as f64 / 2.0;
                let sinc = if x == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * x).sin() / (PI * x)
                };
                let angle = 2.0 * PI * i as f64 / (length - 1) as f64;
                (sinc * (0.42 - 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos())) as f32
            })
            .collect();
        let sum: f32 = taps.iter().sum();
        for tap in &mut taps {
            *tap /= sum;
        }
        Self {
            divisor,
            phase: 0,
            history: vec![[0.0; 2]; length * 2],
            taps,
            cursor: 0,
        }
    }

    pub fn process(&mut self, input: &[u8], output: &mut Vec<u8>) {
        self.process_with(input, output, |sample| sample);
    }

    // Transform filtered floating-point IQ before the final 8-bit quantization.
    pub fn process_with(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        mut transform: impl FnMut([f32; 2]) -> [f32; 2],
    ) {
        assert_eq!(input.len() % 2, 0);
        output.clear();
        let length = self.taps.len();
        for pair in input.chunks_exact(2) {
            let sample = [pair[0] as i8 as f32, pair[1] as i8 as f32];
            self.history[self.cursor] = sample;
            self.history[self.cursor + length] = sample;
            self.cursor += 1;
            if self.cursor == length {
                self.cursor = 0;
            }
            self.phase += 1;
            if self.phase != self.divisor {
                continue;
            }
            self.phase = 0;
            let window = &self.history[self.cursor..self.cursor + length];
            let half = length / 2;
            // Pair symmetric taps to halve the number of multiplications.
            let mut result = [
                window[half][0] * self.taps[half],
                window[half][1] * self.taps[half],
            ];
            for (i, &tap) in self.taps[..half].iter().enumerate() {
                result[0] += tap * (window[i][0] + window[length - 1 - i][0]);
                result[1] += tap * (window[i][1] + window[length - 1 - i][1]);
            }
            for value in transform(result) {
                output.push((value.round().clamp(-128.0, 127.0) as i16 + 128) as u8);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tone(frequency: f64, samples: usize) -> Vec<u8> {
        (0..samples)
            .flat_map(|i| {
                let angle = 2.0 * PI * frequency * i as f64;
                [
                    (100.0 * angle.cos()).round() as i8 as u8,
                    (100.0 * angle.sin()).round() as i8 as u8,
                ]
            })
            .collect()
    }
    fn rms(bytes: &[u8]) -> f64 {
        (bytes
            .iter()
            .map(|&b| (b as f64 - 128.0).powi(2))
            .sum::<f64>()
            / bytes.len() as f64)
            .sqrt()
    }
    #[test]
    fn passband_and_alias_rejection() {
        for divisor in [4, 8, 32] {
            let mut output = vec![];
            Decimator::new(divisor).process(&tone(0.2 / divisor as f64, 32_768), &mut output);
            let pass = rms(&output[512..]);
            assert!((69.0..72.0).contains(&pass), "passband {divisor}: {pass}");
            for frequency in [0.5, 0.6, 0.9, 1.2] {
                Decimator::new(divisor)
                    .process(&tone(frequency / divisor as f64, 32_768), &mut output);
                let stop = rms(&output[512..]);
                assert!(stop < 0.5, "alias {divisor} {frequency}: {stop}");
            }
        }
    }
    #[test]
    fn digital_agc_preserves_chunking_and_amplifies_before_quantization() {
        use crate::digital_agc::DigitalAgc;
        let input = tone(0.03, 10003);
        let mut whole = vec![];
        let mut agc = DigitalAgc::new(250_000);
        Decimator::new(4).process_with(&input, &mut whole, |iq| agc.process(iq));
        let mut filter = Decimator::new(4);
        let mut agc = DigitalAgc::new(250_000);
        let mut fragmented = vec![];
        let mut block = vec![];
        for chunk in input.chunks(126) {
            filter.process_with(chunk, &mut block, |iq| agc.process(iq));
            fragmented.extend_from_slice(&block);
        }
        assert_eq!(whole, fragmented);
        // Sparse one-count impulses become fractional samples in the filter.
        let mut sparse = vec![0; 8192];
        sparse[4096] = 1;
        let mut plain = vec![];
        let mut boosted = vec![];
        Decimator::new(4).process(&sparse, &mut plain);
        Decimator::new(4).process_with(&sparse, &mut boosted, |iq| [iq[0] * 64.0, iq[1] * 64.0]);
        assert!(plain.iter().all(|&b| b == 128));
        assert!(boosted.iter().any(|&b| b != 128));
    }

    #[test]
    fn chunk_boundaries_preserve_phase_and_iq() {
        let input = tone(0.03, 10003);
        let mut complete = vec![];
        Decimator::new(4).process(&input, &mut complete);
        let mut fragmented = vec![];
        let mut block = vec![];
        let mut filter = Decimator::new(4);
        for chunk in input.chunks(126) {
            filter.process(chunk, &mut block);
            fragmented.extend_from_slice(&block);
        }
        assert_eq!(complete, fragmented);
        assert_eq!(complete.len(), (10003 / 4) * 2);
        let mut dc = vec![];
        Decimator::new(4).process(&[12_u8, (-34_i8) as u8].repeat(1000), &mut dc);
        assert_eq!(&dc[dc.len() - 2..], &[140, 94]);
    }
}
