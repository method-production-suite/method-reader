//! The frame-time recorder: with METHOD_READER_FRAME_LOG set to a file path,
//! the window writes one line per frame it draws, so how smoothly it moves
//! can be measured rather than judged by eye. Off (and free) otherwise.
//!
//! Each line is tab-separated:
//!
//! ```text
//! t_ms  logic_ms  cpu_prev_ms  wants  tag
//! ```
//!
//! `t_ms` is when the frame began, from the log's start; `logic_ms` is the
//! window's own work that frame (answers in, timers, drawing, which is where
//! the book gets rasterized); `cpu_prev_ms` is eframe's figure for the
//! whole PREVIOUS frame (the window's work plus tessellation and painting,
//! not the wait for the screen), or `-` before there is one; `wants` is
//! what the window asked of the next frame (`idle`, `next`, or `after:N`);
//! `tag` is free text a driver (examples/motion_bench.rs) labels the frame
//! with. The intervals between `t_ms` values are the frame cadence.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub struct FrameLog {
    start: Instant,
    out: BufWriter<File>,
    /// What a driver labels the next frames with.
    pub tag: String,
}

impl FrameLog {
    /// The log METHOD_READER_FRAME_LOG names, or None when it is unset or the
    /// file cannot be made (said once on stderr).
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("METHOD_READER_FRAME_LOG")?;
        match Self::create(Path::new(&path)) {
            Ok(log) => Some(log),
            Err(e) => {
                eprintln!(
                    "method-reader-app: METHOD_READER_FRAME_LOG: could not write {}: {e}",
                    Path::new(&path).display()
                );
                None
            }
        }
    }

    pub fn create(path: &Path) -> std::io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "t_ms\tlogic_ms\tcpu_prev_ms\twants\ttag")?;
        Ok(Self {
            start: Instant::now(),
            out,
            tag: String::new(),
        })
    }

    /// One frame: when it began, how long the window's own work took,
    /// eframe's CPU figure for the frame before, and what it asked for next.
    pub fn record(&mut self, began: Instant, logic: Duration, cpu_prev: Option<f32>, wants: &str) {
        let t = began.saturating_duration_since(self.start).as_secs_f64() * 1000.0;
        let cpu = cpu_prev.map_or_else(|| "-".to_string(), |s| format!("{:.3}", s * 1000.0));
        let _ = writeln!(
            self.out,
            "{t:.3}\t{:.3}\t{cpu}\t{wants}\t{}",
            logic.as_secs_f64() * 1000.0,
            self.tag
        );
    }

    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }
}

impl Drop for FrameLog {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_per_frame_after_the_header() {
        let dir = tempfile::tempdir().expect("scratch");
        let path = dir.path().join("frames.tsv");
        {
            let mut log = FrameLog::create(&path).expect("log");
            log.tag = "show".into();
            let began = Instant::now();
            log.record(began, Duration::from_micros(1500), Some(0.004), "next");
            log.record(began, Duration::ZERO, None, "idle");
        }
        let text = std::fs::read_to_string(&path).expect("written");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "t_ms\tlogic_ms\tcpu_prev_ms\twants\ttag");
        let first: Vec<&str> = lines[1].split('\t').collect();
        assert_eq!(&first[1..], ["1.500", "4.000", "next", "show"]);
        assert!(lines[2].contains("\t-\tidle\t"));
    }
}
