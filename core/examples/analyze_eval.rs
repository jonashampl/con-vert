//! Measures tempo detection against files whose names state it ("Loop 140BPM.wav",
//! "Beat 128 BPM.mp3"). Reads only; prints misses, then accuracy against how many files
//! get an answer at several confidence thresholds.
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example analyze_eval -- <list.txt>
//!        (one audio file path per line)

use shiftdrop_core::analyze;

fn truth_bpm(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let i = lower.find("bpm")?;
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok();
    let before: String = lower[..i].chars().rev().skip_while(|c| " _-[(".contains(*c)).take_while(|c| c.is_ascii_digit()).collect();
    let after: String = lower[i + 3..].chars().skip_while(|c| " _-:".contains(*c)).take_while(|c| c.is_ascii_digit()).collect();
    let b = digits(&before.chars().rev().collect::<String>()).or_else(|| digits(&after))?;
    (50..=250).contains(&b).then_some(b)
}

fn main() {
    let list = std::fs::read_to_string(std::env::args().nth(1).expect("list file")).expect("read list");
    let ffmpeg = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    let mut tempos: Vec<(u32, u32, f32)> = Vec::new();
    for path in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = std::path::Path::new(path);
        let name = p.file_stem().unwrap().to_string_lossy();
        let Some(t) = truth_bpm(&name) else { continue };
        let Ok((pcm, _)) = analyze::decode(&ffmpeg, p) else { continue };
        if let Some((b, c)) = analyze::tempo_scored(&pcm) {
            tempos.push((t, b, c));
            if b != t {
                println!("MISS {name}: bpm {t}->{b} ({c:.2})");
            }
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
}
