//! Renders labelled test loops in major, minor and Phrygian, so key detection can be
//! measured on audio whose key is certain (producer labels are sometimes wrong, and
//! Phrygian loops are rare). Simple synthesis: bells, plucks, saw pads, keys, an 808,
//! optional drums; random tonic, tempo, detune and voicing. Names carry the truth:
//! "synth 0042 140BPM Dsphy.wav".
//! usage: cargo run --release --example synth_loops -- <out dir> <count> [seed]

use std::f32::consts::PI;

const RATE: f32 = 22050.0;
const NAMES: [&str; 12] = ["C", "Cs", "D", "Ds", "E", "F", "Fs", "G", "Gs", "A", "As", "B"];
/// Scale steps above the tonic: major, natural minor, Phrygian.
const SCALES: [[i32; 7]; 3] = [[0, 2, 4, 5, 7, 9, 11], [0, 2, 3, 5, 7, 8, 10], [0, 1, 3, 5, 7, 8, 10]];
/// Chord progressions as scale degrees (0 = tonic), typical of each mode in bedroom
/// production: pop/major loops, trap minor loops, Phrygian i-bII moves.
const PROGS: [&[&[usize]]; 3] = [
    &[&[0, 4, 5, 3], &[0, 3, 4, 3], &[0, 5, 3, 4], &[3, 0, 4, 5], &[0, 3, 0, 4]],
    &[&[0, 5, 2, 6], &[0, 3, 4, 0], &[0, 5, 3, 4], &[0, 6, 5, 6], &[0, 3, 5, 4], &[0, 0, 5, 6]],
    &[&[0, 1, 0, 1], &[0, 1, 6, 0], &[0, 1, 3, 1], &[0, 6, 1, 0], &[0, 1, 0, 6], &[0, 3, 1, 0]],
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
    fn f(&mut self) -> f32 {
        self.next() as f32 / u32::MAX as f32
    }
    fn pick(&mut self, n: usize) -> usize {
        self.next() as usize % n
    }
}

fn hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// One note into `out`: timbre 0 bell, 1 pluck, 2 saw pad, 3 keys, 4 square lead.
fn voice(out: &mut [f32], start: usize, len: usize, midi: f32, gain: f32, timbre: usize) {
    let f = hz(midi);
    let tail = (0.25 * RATE) as usize;
    for i in 0..(len + tail).min(out.len().saturating_sub(start)) {
        let t = i as f32 / RATE;
        let gate = if i < len { 1.0 } else { (-((i - len) as f32) / RATE * 18.0).exp() };
        let (env, s) = match timbre {
            0 => ((-t * 2.5).exp(), (2.0 * PI * f * t).sin() + 0.4 * (2.0 * PI * f * 2.0 * t).sin() * (-t * 6.0).exp() + 0.25 * (2.0 * PI * f * 3.01 * t).sin() * (-t * 9.0).exp()),
            1 => ((-t * 7.0).exp(), (1..=8).map(|h| (2.0 * PI * f * h as f32 * t).sin() * (-t * 3.0 * h as f32).exp() / h as f32).sum()),
            2 => ((1.0 - (-t * 6.0).exp()), (1..=10).map(|h| (2.0 * PI * f * h as f32 * t).sin() / (h as f32).powf(1.6)).sum()),
            3 => ((-t * 1.8).exp(), (1..=6).map(|h| (2.0 * PI * f * h as f32 * (1.0 + 0.0004 * (h * h) as f32) * t).sin() * (-t * h as f32).exp() / h as f32).sum()),
            _ => (1.0, (1..=9).step_by(2).map(|h| (2.0 * PI * f * h as f32 * t).sin() / h as f32).sum::<f32>() * 0.7),
        };
        out[start + i] += gain * gate * env * (1.0 - (-t * 300.0).exp()) * s;
    }
}

/// An 808: sine with a short pitch drop and soft saturation.
fn eight_o_eight(out: &mut [f32], start: usize, len: usize, midi: f32, gain: f32) {
    let f = hz(midi);
    let mut ph = 0.0f32;
    for i in 0..len.min(out.len().saturating_sub(start)) {
        let t = i as f32 / RATE;
        ph += 2.0 * PI * f * (1.0 + 1.5 * (-t * 30.0).exp()) / RATE;
        let env = (-t * 0.9).exp() * (1.0 - (-t * 400.0).exp()) * if len - i < 400 { (len - i) as f32 / 400.0 } else { 1.0 };
        out[start + i] += gain * (1.6 * ph.sin()).tanh() * env;
    }
}

fn drums(out: &mut [f32], beat: usize, rng: &mut Rng) {
    let bars = out.len() / (4 * beat);
    for b in 0..bars * 4 {
        let at = b * beat;
        if b % 4 == 0 || (b % 4 == 2 && rng.f() < 0.4) {
            for i in 0..(0.25 * RATE) as usize {
                let t = i as f32 / RATE;
                if let Some(o) = out.get_mut(at + i) {
                    *o += 0.6 * (2.0 * PI * (50.0 + 120.0 * (-t * 35.0).exp()) * t).sin() * (-t * 9.0).exp();
                }
            }
        }
        if b % 4 == 2 {
            for i in 0..(0.18 * RATE) as usize {
                if let Some(o) = out.get_mut(at + i) {
                    *o += 0.25 * (rng.f() * 2.0 - 1.0) * (-(i as f32) / RATE * 22.0).exp();
                }
            }
        }
        for k in 0..2 {
            for i in 0..(0.04 * RATE) as usize {
                if let Some(o) = out.get_mut(at + k * beat / 2 + i) {
                    *o += 0.08 * (rng.f() * 2.0 - 1.0) * (-(i as f32) / RATE * 90.0).exp();
                }
            }
        }
    }
}

fn write_wav(path: &std::path::Path, x: &[f32]) -> std::io::Result<()> {
    let peak = x.iter().fold(1e-9f32, |m, v| m.max(v.abs()));
    let mut b = Vec::with_capacity(44 + x.len() * 2);
    let data = (x.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&(RATE as u32).to_le_bytes());
    b.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for v in x {
        b.extend_from_slice(&((v / peak * 0.9 * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("out dir"));
    let count: usize = args.next().expect("count").parse().unwrap();
    let mut rng = Rng(args.next().map(|s| s.parse().unwrap()).unwrap_or(0x9E37_79B9_7F4A_7C15));
    std::fs::create_dir_all(&dir).unwrap();
    let mut list = String::new();
    for n in 0..count {
        let mode = n % 3;
        let tonic = rng.pick(12) as i32;
        let bpm = 70 + rng.pick(91);
        let beat = (60.0 / bpm as f32 * RATE) as usize;
        let bar = 4 * beat;
        let detune = (rng.f() - 0.5) * 0.6;
        let scale = SCALES[mode];
        let prog = PROGS[mode][rng.pick(PROGS[mode].len())];
        let note = |deg: i32| -> i32 { scale[deg.rem_euclid(7) as usize] + 12 * deg.div_euclid(7) };
        let mut x = vec![0.0f32; prog.len() * bar];
        // What plays: chords, melody, 808 and drums in random combinations (never nothing).
        let (has_chords, has_melody) = match rng.pick(3) { 0 => (true, false), 1 => (false, true), _ => (true, true) };
        let has_808 = rng.f() < 0.6;
        let base = 48 + tonic + if tonic > 6 { -12 } else { 0 };
        let (chord_timbre, lead_timbre) = (rng.pick(5), rng.pick(5));
        for (i, &deg) in prog.iter().enumerate() {
            let at = i * bar;
            if has_chords {
                let voicing: &[i32] = if rng.f() < 0.5 { &[0, 2, 4] } else { &[0, 2, 4, 6] };
                for v in voicing {
                    voice(&mut x, at, bar - beat / 4, base as f32 + note(deg as i32 + v) as f32 + detune, 0.22, chord_timbre);
                }
            }
            if has_808 {
                let root = base - 24 + note(deg as i32);
                eight_o_eight(&mut x, at, bar * 3 / 4, root.max(28) as f32 + detune, 0.7);
                if rng.f() < 0.5 {
                    eight_o_eight(&mut x, at + bar * 3 / 4, bar / 4, root.max(28) as f32 + detune, 0.6);
                }
            }
            if has_melody {
                // Eighth-note melody, mostly chord tones, starting on the chord root.
                let mut deg_now = deg as i32 + 7;
                for k in 0..8 {
                    if k > 0 && rng.f() < 0.25 {
                        continue;
                    }
                    if k > 0 {
                        let step = [-2, -1, 1, 2, 0, 2, -2][rng.pick(7)];
                        deg_now = (deg_now + step).clamp(deg as i32 + 3, deg as i32 + 11);
                    }
                    voice(&mut x, at + k * beat / 2, beat / 2, (base + note(deg_now)) as f32 + detune, 0.3, lead_timbre);
                }
            }
        }
        if rng.f() < 0.5 {
            drums(&mut x, beat, &mut rng);
        }
        let name = format!("synth {n:04} {bpm}BPM {}{}.wav", NAMES[tonic as usize], ["maj", "min", "phy"][mode]);
        write_wav(&dir.join(&name), &x).unwrap();
        list += &format!("{}\n", dir.join(&name).display());
    }
    std::fs::write(dir.join("list.txt"), list).unwrap();
    println!("{count} loops in {}", dir.display());
}
