//! The typewriter, heard: model/typewriter.rs's samples played through the
//! computer's default output with cpal. Nothing is opened until the first
//! sound, and a computer with no output, or one that refuses it, is simply
//! quiet (typewriter.js's rule: a sound is never worth an error).
//!
//! cpal talks to Core Audio on a Mac and WASAPI on Windows through the
//! OS's own APIs; on Linux it links ALSA's libasound, a C system library.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::model::typewriter::{Noise, ding_samples, key_samples};

/// What the speaker is still to play, mixed: one entry per output frame.
type Queue = Arc<Mutex<VecDeque<f32>>>;

pub struct Speaker {
    _stream: cpal::Stream,
    queue: Queue,
    rate: f32,
    noise: Noise,
}

/// Add `samples` into what is queued, from the front, so sounds overlap
/// rather than wait their turn.
pub fn mix_into(queue: &mut VecDeque<f32>, samples: &[f32]) {
    for (i, s) in samples.iter().enumerate() {
        match queue.get_mut(i) {
            Some(q) => *q = (*q + s).clamp(-1.0, 1.0),
            None => queue.push_back(*s),
        }
    }
}

impl Speaker {
    /// The default output, or None: no device, no f32 format, or refused.
    pub fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return None;
        }
        let config = supported.config();
        let channels = usize::from(config.channels).max(1);
        let rate = config.sample_rate as f32;
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        let feed = queue.clone();
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    let mut q = feed.lock().unwrap_or_else(|e| e.into_inner());
                    for frame in data.chunks_mut(channels) {
                        let s = q.pop_front().unwrap_or(0.0);
                        frame.fill(s);
                    }
                },
                |_| {},
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Self {
            _stream: stream,
            queue,
            rate,
            noise: Noise::new(0x5c2e_e9b1),
        })
    }

    pub fn key(&mut self) {
        let samples = key_samples(self.rate, &mut self.noise);
        self.push(&samples);
    }

    pub fn ding(&mut self) {
        let samples = ding_samples(self.rate);
        self.push(&samples);
    }

    fn push(&self, samples: &[f32]) {
        if let Ok(mut q) = self.queue.lock() {
            mix_into(&mut q, samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_overlap_rather_than_queue() {
        let mut q = VecDeque::from(vec![0.5, 0.5]);
        mix_into(&mut q, &[0.25, 0.25, 0.25]);
        assert_eq!(q, VecDeque::from(vec![0.75, 0.75, 0.25]));
        mix_into(&mut q, &[0.9]);
        assert_eq!(q[0], 1.0, "never past full scale");
    }
}
