//! `cargo xtask capture-film --out <folder>`: the launch film, from the real
//! window. sp-app's film program (examples/film.rs) drives the window
//! offscreen through a short story on the committed invented scripts and
//! writes raw frames; this pipes them into ffmpeg and makes the three files
//! the launch posts use:
//!
//! - `method-reader-film.mp4`: H.264 at 1280 by 800, for the site and posts;
//! - `method-reader-film.gif`: 15 seconds of the opening, smaller;
//! - `method-reader-film-poster.png`: one frame of the book.
//!
//! The film runs on a scratch library and scratch settings in a fresh temp
//! folder, removed afterwards, with the same engine environment the
//! pictures have (no reader, a reMarkable address nothing listens on, an
//! MTP helper that is not there, Calibre pointed at an empty scratch
//! folder), and the film program's own guard keeps every call inside it.
//! Needs ffmpeg on PATH. Exit codes: 0 made, 1 failed, 2 bad orders.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::paths::{repo_root, workspace_root};
use crate::scratch::TempDir;

pub const MP4: &str = "method-reader-film.mp4";
pub const GIF: &str = "method-reader-film.gif";
pub const POSTER: &str = "method-reader-film-poster.png";

/// The window's frame in pixels (1280 by 800 points at two pixels a point).
const DRAWN: &str = "2560x1600";
/// The MP4's ceiling: what Reddit and the site take without a fuss.
pub const MP4_LIMIT: u64 = 20 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Orders {
    pub out: PathBuf,
    pub fps: u32,
}

pub fn parse(args: &[String]) -> Result<Orders, String> {
    let (mut out, mut fps) = (None, 15u32);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out needs a folder")?)),
            "--fps" => {
                fps = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|n| (1..=60).contains(n))
                    .ok_or("--fps needs a number from 1 to 60")?
            }
            other => return Err(format!("no such order: {other}")),
        }
    }
    Ok(Orders {
        out: out.ok_or("--out <folder> is needed")?,
        fps,
    })
}

/// ffmpeg's orders for the MP4: raw frames on stdin, scaled to 1280 by 800.
pub fn mp4_args(fps: u32, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = [
        "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", DRAWN, "-r",
    ]
    .map(String::from)
    .to_vec();
    a.push(fps.to_string());
    a.extend(
        [
            "-i",
            "-",
            "-vf",
            "scale=1280:800:flags=lanczos,format=yuv420p",
            "-c:v",
            "libx264",
            "-crf",
            "20",
            "-preset",
            "slow",
            "-movflags",
            "+faststart",
        ]
        .map(String::from),
    );
    a.push(out.to_string_lossy().into_owned());
    a
}

/// The GIF: the first three shots, a little of each (0.5 to 5 s, 6.5 to
/// 11 s, 12 to 18 s: fifteen seconds), at 640 wide and 12 frames a second,
/// on a palette made from those frames.
pub const GIF_FILTER: &str = "[0:v]trim=0.5:5,setpts=PTS-STARTPTS[a];\
[0:v]trim=6.5:11,setpts=PTS-STARTPTS[b];\
[0:v]trim=12:18,setpts=PTS-STARTPTS[c];\
[a][b][c]concat=n=3:v=1,fps=12,scale=640:-1:flags=lanczos,split[x][y];\
[x]palettegen=stats_mode=diff[p];[y][p]paletteuse=dither=bayer:bayer_scale=4";

fn ffmpeg(args: &[String]) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(args)
        .status()
        .map_err(|e| format!("could not run ffmpeg: {e}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("ffmpeg failed: {}", args.join(" ")))
}

fn build_film() -> Result<PathBuf, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--release",
            "--locked",
            "--package",
            "sp-app",
            "--example",
            "film",
        ])
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("could not build the film program: {e}"))?;
    if !status.success() {
        return Err("the film program (sp-app's film example) did not build".into());
    }
    let exe = if cfg!(windows) { "film.exe" } else { "film" };
    Ok(super::release_dir().join("examples").join(exe))
}

fn make(o: &Orders) -> Result<(), String> {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return Err("ffmpeg is not on PATH (brew install ffmpeg)".into());
    }
    std::fs::create_dir_all(&o.out)
        .map_err(|e| format!("could not make {}: {e}", o.out.display()))?;
    let film = build_film()?;
    let scratch = TempDir::new("method-reader-film-").map_err(|e| e.to_string())?;
    let library = scratch.path().join("library");
    let calibre = scratch.path().join("calibre");
    for d in [&library, &calibre, &library.join(".no-volumes")] {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let compare = scratch.path().join("compare.rgba");
    let picture = repo_root().join("site/img/compare-light.png");
    ffmpeg(
        &["-v", "error", "-y", "-i"]
            .map(String::from)
            .into_iter()
            .chain([picture.to_string_lossy().into_owned()])
            .chain(["-f", "rawvideo", "-pix_fmt", "rgba"].map(String::from))
            .chain([compare.to_string_lossy().into_owned()])
            .collect::<Vec<_>>(),
    )?;

    let mp4 = o.out.join(MP4);
    let mut window = Command::new(&film)
        .args([
            "--fixtures",
            &repo_root().join("tests/fixtures").to_string_lossy(),
        ])
        .args(["--compare", &compare.to_string_lossy()])
        .args(["--fps", &o.fps.to_string()])
        .envs(super::run::engine_env(&library, &calibre))
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the film program: {e}"))?;
    let frames = window
        .stdout
        .take()
        .ok_or("the film program has no stdout")?;
    let encoder = Command::new("ffmpeg")
        .args(mp4_args(o.fps, &mp4))
        .stdin(frames)
        .status()
        .map_err(|e| format!("could not run ffmpeg: {e}"))?;
    let filmed = window
        .wait()
        .map_err(|e| format!("the film program: {e}"))?;
    if !filmed.success() {
        return Err("the film program failed (its reason is above)".into());
    }
    if !encoder.success() {
        return Err("ffmpeg could not make the MP4".into());
    }
    let size = std::fs::metadata(&mp4).map_err(|e| e.to_string())?.len();
    if size > MP4_LIMIT {
        return Err(format!(
            "{} is {size} bytes, over the 20 MB the posts take",
            mp4.display()
        ));
    }

    let gif = o.out.join(GIF);
    let s = |p: &Path| p.to_string_lossy().into_owned();
    ffmpeg(&[
        "-v".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        s(&mp4),
        "-filter_complex".into(),
        GIF_FILTER.into(),
        s(&gif),
    ])?;
    let poster = o.out.join(POSTER);
    ffmpeg(&[
        "-v".into(),
        "error".into(),
        "-y".into(),
        "-ss".into(),
        "9".into(),
        "-i".into(),
        s(&mp4),
        "-frames:v".into(),
        "1".into(),
        s(&poster),
    ])?;
    for f in [&mp4, &gif, &poster] {
        let n = std::fs::metadata(f).map_err(|e| e.to_string())?.len();
        println!("{}  {} KB", f.display(), n / 1024);
    }
    Ok(())
}

pub fn main(args: &[String]) -> u8 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("capture-film: {e}");
            return 2;
        }
    };
    match make(&o) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("capture-film: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_orders_need_a_folder_and_take_a_frame_rate() {
        assert_eq!(
            parse(&args(&["--out", "/tmp/film"])).unwrap(),
            Orders {
                out: "/tmp/film".into(),
                fps: 15
            }
        );
        assert_eq!(
            parse(&args(&["--out", "x", "--fps", "24"])).unwrap().fps,
            24
        );
        assert!(parse(&args(&[])).unwrap_err().contains("--out"));
        assert!(parse(&args(&["--out", "x", "--fps", "0"])).is_err());
        assert!(parse(&args(&["--out", "x", "--fps", "120"])).is_err());
        assert!(parse(&args(&["--out", "x", "--loud"])).is_err());
    }

    #[test]
    fn the_mp4_is_read_from_stdin_at_the_drawn_size_and_scaled_to_the_frame() {
        let a = mp4_args(15, Path::new("/o/f.mp4"));
        let at = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].as_str());
        assert_eq!(at("-s"), Some("2560x1600"));
        assert_eq!(at("-r"), Some("15"));
        assert_eq!(at("-i"), Some("-"));
        assert_eq!(at("-c:v"), Some("libx264"));
        assert!(at("-vf").unwrap().starts_with("scale=1280:800"));
        assert_eq!(a.last().map(String::as_str), Some("/o/f.mp4"));
    }

    #[test]
    fn the_gif_is_fifteen_seconds_of_the_first_three_shots() {
        let spans: f32 = ["0.5:5", "6.5:11", "12:18"]
            .iter()
            .map(|s| {
                assert!(GIF_FILTER.contains(&format!("trim={s}")), "{s}");
                let (a, b) = s.split_once(':').unwrap();
                b.parse::<f32>().unwrap() - a.parse::<f32>().unwrap()
            })
            .sum();
        assert_eq!(spans, 15.0);
    }
}
