//! Measures BPM and key detection against files whose names state them
//! ("Loop 140BPM G#Min.wav"). Reads only; prints misses, then accuracy against how many
//! files get an answer at several confidence thresholds.
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example analyze_eval -- <list.txt>
//!        (one audio file path per line)

use shiftdrop_core::analyze::{self, Key};

fn truth_bpm(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let i = lower.find("bpm")?;
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok();
    let before: String = lower[..i].chars().rev().skip_while(|c| " _-[(".contains(*c)).take_while(|c| c.is_ascii_digit()).collect();
    let after: String = lower[i + 3..].chars().skip_while(|c| " _-:".contains(*c)).take_while(|c| c.is_ascii_digit()).collect();
    let b = digits(&before.chars().rev().collect::<String>()).or_else(|| digits(&after))?;
    (50..=250).contains(&b).then_some(b)
}

/// Only keys written with an explicit quality ("min", "maj"), so words aren't misread.
fn truth_key(name: &str) -> Option<Key> {
    let tokens: Vec<&str> = name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).filter(|t| !t.is_empty()).collect();
    let explicit = |s: &str| ["min", "minor", "maj", "major"].iter().any(|q| s.to_ascii_lowercase().ends_with(q));
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

fn kind(t: Key, k: Key) -> &'static str {
    if k == t {
        "exact"
    } else if t.minor != k.minor && (if t.minor { (t.tonic + 3) % 12 } else { (t.tonic + 9) % 12 }) == k.tonic {
        "relative"
    } else if t.minor == k.minor && ((t.tonic + 7) % 12 == k.tonic || (t.tonic + 5) % 12 == k.tonic) {
        "fifth"
    } else if t.minor != k.minor && t.tonic == k.tonic {
        "parallel"
    } else {
        "other"
    }
}

fn main() {
    let list = std::fs::read_to_string(std::env::args().nth(1).expect("list file")).expect("read list");
    let ffmpeg = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    let mut tempos: Vec<(u32, u32, f32)> = Vec::new();
    let mut keys: Vec<(Key, Key, f32)> = Vec::new();
    for path in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = std::path::Path::new(path);
        let name = p.file_stem().unwrap().to_string_lossy();
        let (tb, tk) = (truth_bpm(&name), truth_key(&name));
        if tb.is_none() && tk.is_none() {
            continue;
        }
        let Ok((pcm, _)) = analyze::decode(&ffmpeg, p) else { continue };
        let mut line = String::new();
        if let (Some(t), Some((b, c))) = (tb, analyze::tempo_scored(&pcm)) {
            tempos.push((t, b, c));
            if b != t {
                line += &format!(" bpm {t}->{b} ({c:.2})");
            }
        }
        if let (Some(t), Some((k, m))) = (tk, analyze::key_scored(&pcm)) {
            keys.push((t, k, m));
            if k != t {
                line += &format!(" key {}->{} {} ({m:.3})", t.name(), k.name(), kind(t, k));
            }
        }
        if !line.is_empty() {
            println!("MISS {name}:{line}");
        }
    }

    println!("\nBPM ({} files): confidence threshold -> answered, exact, within 1", tempos.len());
    for th in [0.0, 1.05, 1.1, 1.15, 1.2, 1.3, 1.5] {
        let ans: Vec<_> = tempos.iter().filter(|t| t.2 >= th).collect();
        let n = ans.len().max(1) as f32;
        let exact = ans.iter().filter(|t| t.0 == t.1).count() as f32;
        let near = ans.iter().filter(|t| t.0.abs_diff(t.1) <= 1).count() as f32;
        println!("  >= {th:.2}: {:5.1}% answered, {:5.1}% exact, {:5.1}% within 1", 100.0 * ans.len() as f32 / tempos.len().max(1) as f32, 100.0 * exact / n, 100.0 * near / n);
    }
    println!("\nKey ({} files): margin threshold -> answered, exact, relative, fifth, parallel", keys.len());
    for th in [0.0, 0.005, 0.01, 0.02, 0.03, 0.05, 0.08] {
        let ans: Vec<_> = keys.iter().filter(|k| k.2 >= th).collect();
        let n = ans.len().max(1) as f32;
        let pct = |what: &str| 100.0 * ans.iter().filter(|k| kind(k.0, k.1) == what).count() as f32 / n;
        println!(
            "  >= {th:.3}: {:5.1}% answered, {:5.1}% exact, {:4.1}% relative, {:4.1}% fifth, {:4.1}% parallel",
            100.0 * ans.len() as f32 / keys.len().max(1) as f32,
            pct("exact"),
            pct("relative"),
            pct("fifth"),
            pct("parallel")
        );
    }
}
