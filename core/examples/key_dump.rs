//! Dumps, per file named with its key, the pitch classes the note model hears (the input
//! key choice works from), one JSON line each. For tuning key choice offline. Besides the
//! plain sum ("ch") it writes variants with bass overtones taken off ("v1".."v5").
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

/// Pitch classes after taking each low note's overtones (semitones up, share) off the notes
/// above it, frame by frame. `bass_below`: only notes under this MIDI number count as sources.
fn suppressed(frames: &[[f32; 88]], partials: &[(usize, f32)], bass_below: usize) -> [f32; 12] {
    let mut out = [0.0f32; 12];
    for f in frames {
        let mut v = *f;
        for k in 0..88 {
            if 21 + k >= bass_below || v[k] <= 0.2 {
                continue;
            }
            for &(up, share) in partials {
                if let Some(h) = v.get_mut(k + up) {
                    *h = (*h - f[k] * share).max(0.0);
                }
            }
        }
        for (k, a) in v.into_iter().enumerate() {
            if a > 0.2 {
                out[(21 + k) % 12] += a;
            }
        }
    }
    out
}

fn main() {
    let list = std::fs::read_to_string(std::env::args().nth(1).expect("list file")).expect("read list");
    let ff = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    for path in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = std::path::Path::new(path);
        let Some(truth) = truth_key(&p.file_stem().unwrap().to_string_lossy()) else { continue };
        let Ok((pcm, _)) = analyze::decode(&ff, p) else { continue };
        let Some(frames) = notes::activations(&pcm) else { continue };
        let ch = suppressed(&frames, &[], 0);
        let v1 = suppressed(&frames, &[(28, 1.0)], 60);
        let v2 = suppressed(&frames, &[(28, 1.0), (34, 1.0)], 60);
        let v3 = suppressed(&frames, &[(19, 0.6), (28, 1.0), (31, 0.6), (34, 1.0)], 60);
        let v4 = suppressed(&frames, &[(28, 1.0), (34, 1.0)], 72);
        let v5 = suppressed(&frames, &[(28, 0.5), (34, 0.5), (40, 0.5)], 60);
        println!("{{\"truth\":\"{}\",\"ch\":{ch:?},\"v1\":{v1:?},\"v2\":{v2:?},\"v3\":{v3:?},\"v4\":{v4:?},\"v5\":{v5:?}}}", truth.name());
    }
}
