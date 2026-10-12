//! The typewriter's sounds, made rather than recorded:
//! `desktop/ui/typewriter.js`'s Web Audio graph, as samples. A key is 35 ms
//! of white noise through a band-pass (1800 to 3400 Hz, different each
//! time, Q 1.4) under an exponential fade from 0.3; the bell is a 2093 Hz
//! sine fading from 0.08 over half a second.

pub const KEY_GAIN: f32 = 0.3;
pub const KEY_SECONDS: f32 = 0.035;
pub const DING_PITCH: f32 = 2093.0;
pub const DING_GAIN: f32 = 0.08;
pub const DING_SECONDS: f32 = 0.5;
/// Web Audio's exponential ramps end here, never at zero.
const FLOOR: f32 = 0.0001;

/// A small deterministic noise source (xorshift), so a test can hear the
/// same key twice.
#[derive(Clone, Debug)]
pub struct Noise(u64);

impl Noise {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// A value in [0, 1).
    pub fn sample(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The exponential ramp Web Audio draws from `level` to FLOOR over `n`.
fn envelope(level: f32, i: usize, n: usize) -> f32 {
    level * (FLOOR / level).powf(i as f32 / n as f32)
}

/// One key: band-passed noise under its fade.
pub fn key_samples(rate: f32, noise: &mut Noise) -> Vec<f32> {
    let n = (rate * KEY_SECONDS).ceil() as usize;
    let centre = 1800.0 + noise.sample() * 1600.0;
    let w0 = 2.0 * std::f32::consts::PI * centre / rate;
    let alpha = w0.sin() / (2.0 * 1.4);
    let a0 = 1.0 + alpha;
    let (b0, b2) = (alpha / a0, -alpha / a0);
    let (a1, a2) = (-2.0 * w0.cos() / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            let x = noise.sample() * 2.0 - 1.0;
            let y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            y * envelope(KEY_GAIN, i, n)
        })
        .collect()
}

/// The carriage-return bell.
pub fn ding_samples(rate: f32) -> Vec<f32> {
    let n = (rate * DING_SECONDS).ceil() as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            (2.0 * std::f32::consts::PI * DING_PITCH * t).sin() * envelope(DING_GAIN, i, n)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_35_ms_and_fades_to_nothing() {
        let k = key_samples(48_000.0, &mut Noise::new(7));
        assert_eq!(k.len(), 1680);
        let head: f32 = k[..200].iter().map(|s| s.abs()).sum();
        let tail: f32 = k[k.len() - 200..].iter().map(|s| s.abs()).sum();
        assert!(head > tail * 10.0, "loud at the strike, quiet at the end");
        assert!(k.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn no_two_keys_sound_alike() {
        let mut noise = Noise::new(3);
        let a = key_samples(44_100.0, &mut noise);
        let b = key_samples(44_100.0, &mut noise);
        assert_ne!(a, b);
    }

    #[test]
    fn the_bell_is_half_a_second_at_c7() {
        let d = ding_samples(48_000.0);
        assert_eq!(d.len(), 24_000);
        let peak = d.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= DING_GAIN + 1e-6 && peak > DING_GAIN * 0.9);
        let crossings = d[..4800]
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        assert!(
            (205..=215).contains(&crossings),
            "about 209 cycles in a tenth of a second: {crossings}"
        );
    }
}
