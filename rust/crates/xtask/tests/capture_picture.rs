//! New with the port: the PNG half a pixel-drawing backend needs (capture
//! /picture.rs), and the small helpers make-og and the run lean on.

use std::path::Path;

use xtask::capture::picture::{Diff, decode, diff, encode_rgba, size};

fn checker(w: u32, h: u32, flip: Option<(u32, u32)>) -> Vec<u8> {
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let on = ((x + y) % 2 == 0) != (flip == Some((x, y)));
            px.extend_from_slice(if on {
                &[20, 30, 40, 255]
            } else {
                &[200, 210, 220, 128]
            });
        }
    }
    px
}

#[test]
fn same_pixels_encode_to_the_same_bytes_and_decode_back() {
    let px = checker(7, 5, None);
    let a = encode_rgba(7, 5, &px).unwrap();
    let b = encode_rgba(7, 5, &px).unwrap();
    assert_eq!(a, b);
    assert_eq!(size(&a).unwrap(), (7, 5));
    let back = decode(&a).unwrap();
    assert_eq!((back.width, back.height), (7, 5));
    assert_eq!(back.pixels, px);
}

#[test]
fn a_wrong_pixel_count_is_refused() {
    assert!(
        encode_rgba(2, 2, &[0; 15])
            .unwrap_err()
            .contains("needs 16")
    );
}

#[test]
fn diff_tells_same_from_changed_pixels_from_another_size() {
    let a = encode_rgba(4, 4, &checker(4, 4, None)).unwrap();
    let one = encode_rgba(4, 4, &checker(4, 4, Some((1, 2)))).unwrap();
    let wide = encode_rgba(5, 4, &checker(5, 4, None)).unwrap();
    assert_eq!(diff(&a, &a).unwrap(), Diff::Same);
    assert_eq!(
        diff(&a, &one).unwrap(),
        Diff::Pixels {
            changed: 1,
            total: 16
        }
    );
    assert_eq!(
        diff(&a, &wide).unwrap(),
        Diff::Size {
            before: (4, 4),
            after: (5, 4)
        }
    );
    assert!(diff(b"not a png", &a).is_err());
}

#[test]
fn the_committed_pictures_decode_at_twice_the_framed_size() {
    // The README's window pictures are pass two: the window plus the frame,
    // at deviceScaleFactor 2.
    let drop = xtask::capture::shots::SHOTS
        .iter()
        .find(|s| s.name == "drop")
        .unwrap();
    let (w, h) = xtask::capture::shots::framed_size(drop);
    let path = xtask::paths::repo_root().join("assets/screens/drop-light.png");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipped: no {}", path.display());
        return;
    };
    assert_eq!(size(&bytes).unwrap(), (w * 2, h * 2));
}

#[test]
fn file_urls_encode_what_a_url_cannot_carry() {
    let url = xtask::capture::file_url(Path::new("/Users/me/My Site/og card.html"));
    assert_eq!(url, "file:///Users/me/My%20Site/og%20card.html");
}

#[test]
fn tokens_are_32_hex_digits_and_never_repeat() {
    let a = xtask::capture::random_token();
    let b = xtask::capture::random_token();
    assert_eq!(a.len(), 32);
    assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}
