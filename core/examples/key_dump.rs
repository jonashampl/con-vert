//! Dumps, per file named with its key, the pitch classes the note model hears (the input
//! key choice works from), one JSON line each. For tuning key choice offline.
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example key_dump -- <list.txt>

use shiftdrop_core::{analyze, notes};

fn truth_key(name: &str) -> Option<analyze::Key> {
    let tokens: Vec<&str> = name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter(|t| !t.is_empty()).collect();
    let explicit = |s: &str| ["min", "minor", "maj", "major", "phy"].iter().any(|q| s.to_ascii_lowercase().ends_with(q));
    for (i, t) in tokens.iter().enumerate() {
        if explicit(t) && t.len() <= 7 {
            if let Some(k) = analyze::parse_key(t) {
                return Some(k);
            }
        }
        if let Some(next) = tokens.get(i + 1) {
            if t.len() <= 2 && matches!(next.to_ascii_lowercase().as_str(), "min" | "minor" | "maj" | "major") {
                if let Some(k) = analyze::parse_key(&format!("{t}{next}")) {
                    return Some(k);
                }
            }
        }
    }
    None
}

fn main() {
    let list = std::fs::read_to_string(std::env::args().nth(1).expect("list file")).expect("read list");
    let ff = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    for path in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = std::path::Path::new(path);
        let Some(truth) = truth_key(&p.file_stem().unwrap().to_string_lossy()) else { continue };
        let Ok((pcm, _)) = analyze::decode(&ff, p) else { continue };
        let ch = notes::chroma(&pcm).map(|c| format!("{c:?}")).unwrap_or_else(|| "null".into());
        println!("{{\"truth\":\"{}\",\"ch\":{ch}}}", truth.name());
    }
}
