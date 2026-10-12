//! nodepath against Node's own answers. tests/data/node-path.json is what
//! bun 1.3.14's `node:path` answered for every input below, posix and win32
//! (made 2026-10-11 by a throwaway bun script that printed, per input:
//! normalize, basename, dirname, extname, isAbsolute, join('/root', i),
//! win32.join('C:\\root', i), join(i, 'K'), and resolve for the absolute
//! ones). The inputs cover what the device code feeds it: mount roots, drive
//! roots, UNC shares, `..`, trailing separators and dotted names.

use serde_json::Value;
use sp_fsutil::nodepath::{posix, win32};

fn answers() -> serde_json::Map<String, Value> {
    let text = include_str!("data/node-path.json");
    serde_json::from_str::<Value>(text)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn every_function_answers_as_node_does() {
    let mut wrong = Vec::new();
    for (input, want) in answers() {
        let i = input.as_str();
        let mut check = |name: &str, got: String, key: &str| {
            if got != s(&want[key]) {
                wrong.push(format!(
                    "{name}({i:?}) = {got:?}, Node says {:?}",
                    s(&want[key])
                ));
            }
        };
        check("posix.normalize", posix::normalize(i), "pn");
        check("posix.basename", posix::basename(i), "pb");
        check("posix.dirname", posix::dirname(i), "pd");
        check("posix.extname", posix::extname(i), "pe");
        check("posix.join(/root)", posix::join(&["/root", i]), "pj");
        check("posix.join(_, K)", posix::join(&[i, "K"]), "pj2");
        check("win32.normalize", win32::normalize(i), "wn");
        check("win32.basename", win32::basename(i), "wb");
        check("win32.dirname", win32::dirname(i), "wd");
        check("win32.extname", win32::extname(i), "we");
        check("win32.join(C:\\root)", win32::join(&["C:\\root", i]), "wj");
        check("win32.join(_, K)", win32::join(&[i, "K"]), "wj2");
        if want["pa"].as_bool().unwrap() != posix::is_absolute(i) {
            wrong.push(format!("posix.isAbsolute({i:?})"));
        }
        if want["wa"].as_bool().unwrap() != win32::is_absolute(i) {
            wrong.push(format!("win32.isAbsolute({i:?})"));
        }
        if let Some(r) = want["pr"].as_str()
            && posix::resolve(&[i]) != r
        {
            wrong.push(format!(
                "posix.resolve({i:?}) = {:?}, Node says {r:?}",
                posix::resolve(&[i])
            ));
        }
        if let Some(r) = want["wr"].as_str()
            && win32::resolve(&[i]) != r
        {
            wrong.push(format!(
                "win32.resolve({i:?}) = {:?}, Node says {r:?}",
                win32::resolve(&[i])
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_reveal_case_a_trailing_dot_segment_is_resolved_away_before_dirname() {
    // cli-reveal on Windows: dirname alone hands the file back, resolve first
    // collapses the "\." so dirname strips the real file name.
    assert_eq!(win32::dirname("C:\\x\\a.exe\\."), "C:\\x\\a.exe");
    assert_eq!(
        win32::dirname(&win32::resolve(&["C:\\x\\a.exe\\."])),
        "C:\\x"
    );
}
