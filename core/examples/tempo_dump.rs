//! Dumps, per file named with its tempo, what tempo choice can work from: the length in
//! seconds and beat-comb scores over 60-200 BPM (4 beats every 0.25, 16 beats every 0.05),
//! one JSON line each. For tuning tempo choice offline.
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example tempo_dump -- <list.txt>

use shiftdrop_core::analyze::{self, Pulse};

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
    let ff = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    for path in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = std::path::Path::new(path);
        let Some(truth) = truth_bpm(&p.file_stem().unwrap().to_string_lossy()) else { continue };
        let Ok((pcm, _)) = analyze::decode(&ff, p) else { continue };
        let Some(pulse) = Pulse::of(&pcm) else { continue };
        let grid = |step: f32, beats: usize| -> String {
            let n = ((200.0 - 60.0) / step).round() as usize;
            (0..=n).map(|i| format!("{:.4}", pulse.comb(60.0 + i as f32 * step, beats))).collect::<Vec<_>>().join(",")
        };
        let now = analyze::tempo_scored(&pcm).map(|(b, c)| format!("[{b},{c:.3}]")).unwrap_or_else(|| "null".into());
        println!(
            "{{\"truth\":{truth},\"secs\":{:.4},\"now\":{now},\"c4\":[{}],\"c16\":[{}]}}",
            pcm.len() as f32 / analyze::RATE,
            grid(0.25, 4),
            grid(0.05, 16)
        );
    }
}
