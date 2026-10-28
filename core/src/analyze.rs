//! Tempo (BPM) and key of a piece of music, for naming converted files
//! ("Song 124 BPM Am.mp3"). Tags written by DJ software win when present; otherwise the
//! sound is analysed: FFmpeg decodes up to four minutes to mono 22.05 kHz, then
//! - tempo: spectral-flux onsets, autocorrelation, a comb over the beat period and its
//!   multiples, kept in the 85-180 BPM range producers name tracks in (trap at 150, not 75);
//! - key: a harmonic-only spectrogram (median-filter separation) down to 27 Hz (808s),
//!   tuned, read as spectral peaks per semitone with overtones taken off, matched per 10 s
//!   segment against three key profiles; major or minor from the 3rd, 6th and 7th.
//! Measured with examples/analyze_eval.rs on producer loops named with tempo and key:
//! tempo ~85% exact, key ~55% exact (tonic right ~2/3).
//! Either answer is dropped when the evidence is weak, so a guess never ends up in a name.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub const RATE: f32 = 22050.0;
const MAX_SECONDS: u32 = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Pitch class of the tonic, 0 = C.
    pub tonic: u8,
    pub minor: bool,
}

const MAJOR_NAMES: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MINOR_NAMES: [&str; 12] = ["Cm", "C#m", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "G#m", "Am", "Bbm", "Bm"];

impl Key {
    /// "Am", "F#m", "Bb".
    pub fn name(&self) -> &'static str {
        if self.minor { MINOR_NAMES[self.tonic as usize % 12] } else { MAJOR_NAMES[self.tonic as usize % 12] }
    }
}

/// Reads a key written as "Am", "A minor", "F#min", "Cs Min", "Bb", "C major" or Camelot
/// "8A". Bare letters are major.
pub fn parse_key(s: &str) -> Option<Key> {
    let s = s.trim();
    // Camelot wheel: 8A = A minor, 8B = C major.
    if let Some(n) = s.strip_suffix(['A', 'a', 'B', 'b']).and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=12).contains(n)) {
        let minor = s.ends_with(['A', 'a']);
        let base = if minor { 8 } else { 3 }; // 1A = G#m (8), 1B = B (11): each step is a fifth (+7)
        let tonic = ((base as i32 + (n as i32 - 1) * 7 + if minor { 0 } else { 8 }) % 12) as u8;
        return Some(Key { tonic, minor });
    }
    let mut chars = s.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let mut tonic: i32 = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let mut rest = rest.as_str();
    for (prefix, shift) in [("#", 1), ("♯", 1), ("♭", -1), ("b", -1), ("s", 1)] {
        // "b" and "s" are accidentals only when something (or nothing) that reads as a
        // quality follows: "Bb", "Bbm", "Csmin"; not "Bmin" read as B-flat.
        if let Some(r) = rest.strip_prefix(prefix) {
            if prefix.len() > 1 || prefix == "#" || quality(r).is_some() {
                tonic += shift;
                rest = r;
                break;
            }
        }
    }
    Some(Key { tonic: tonic.rem_euclid(12) as u8, minor: quality(rest)? })
}

/// Some(true) for minor, Some(false) for major, None for anything else.
fn quality(rest: &str) -> Option<bool> {
    let r = rest.trim_matches(|c: char| c == ' ' || c == '-' || c == '_');
    if r == "M" {
        return Some(false);
    }
    match r.to_ascii_lowercase().as_str() {
        "" | "maj" | "major" | "dur" => Some(false),
        "m" | "min" | "minor" | "moll" => Some(true),
        _ => None,
    }
}

/// What the file's tags say: (bpm, key), from "TBPM"/"BPM" and "TKEY"/"initialkey".
pub fn parse_tags(stderr: &str) -> (Option<u32>, Option<Key>) {
    let (mut bpm, mut key) = (None, None);
    for line in stderr.lines() {
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "tbpm" | "bpm" => bpm = bpm.or(value.trim().parse::<f32>().ok().filter(|b| (40.0..=300.0).contains(b)).map(|b| b.round() as u32)),
            "tkey" | "initialkey" | "key" => key = key.or(parse_key(value)),
            _ => {}
        }
    }
    (bpm, key)
}

/// Tempo and key of `path`, tags first. Either may be None (no tag, weak evidence).
pub fn analyze(ffmpeg: &Path, path: &Path) -> Result<(Option<u32>, Option<Key>)> {
    let (pcm, (tag_bpm, tag_key)) = decode(ffmpeg, path)?;
    if tag_bpm.is_some() && tag_key.is_some() {
        return Ok((tag_bpm, tag_key));
    }
    Ok((tag_bpm.or_else(|| tempo(&pcm)), tag_key.or_else(|| key(&pcm))))
}

/// Up to four minutes of `path` as mono samples at [`RATE`], plus what its tags say.
pub fn decode(ffmpeg: &Path, path: &Path) -> Result<(Vec<f32>, (Option<u32>, Option<Key>))> {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-t", &MAX_SECONDS.to_string(), "-ac", "1", "-ar", &RATE.to_string(), "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .context("can't start FFmpeg")?;
    let mut stderr = child.stderr.take().context("no stderr")?;
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let mut bytes = Vec::new();
    child.stdout.take().context("no stdout")?.read_to_end(&mut bytes)?;
    let _ = child.wait();
    let tags = parse_tags(&err.join().unwrap_or_default());
    let pcm: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok((pcm, tags))
}

/// "Song" -> "Song 124 BPM Am"; leaves out what's unknown or already in the name.
pub fn tagged_stem(stem: &str, bpm: Option<u32>, key: Option<Key>) -> String {
    let lower = stem.to_ascii_lowercase();
    let mut out = stem.to_string();
    if let Some(b) = bpm.filter(|_| !lower.contains("bpm")) {
        out.push_str(&format!(" {b} BPM"));
    }
    if let Some(k) = key.filter(|_| !lower.contains("bpm") || bpm.is_some()) {
        out.push(' ');
        out.push_str(k.name());
    }
    out
}

// ---------- spectra ----------

/// In-place radix-2 FFT (length a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Magnitude spectrogram: one Vec of `size / 2` bins per frame (Hann window).
fn spectrogram(x: &[f32], size: usize, hop: usize, max_bin: usize) -> Vec<Vec<f32>> {
    let win: Vec<f32> = (0..size).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()).collect();
    let (mut re, mut im) = (vec![0.0; size], vec![0.0; size]);
    let mut frames = Vec::new();
    let mut start = 0;
    while start + size <= x.len() {
        for i in 0..size {
            re[i] = x[start + i] * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        frames.push((0..max_bin.min(size / 2)).map(|k| re[k].hypot(im[k])).collect());
        start += hop;
    }
    frames
}

// ---------- tempo ----------

/// How much the winning beat period must stand out from the rest of the range.
pub const MIN_TEMPO_CONFIDENCE: f32 = 1.1;
/// How clearly major and minor must differ (share of their combined evidence).
pub const MIN_KEY_MARGIN: f32 = 0.05;

/// Beats per minute, rounded; None when there's no steady beat.
pub fn tempo(x: &[f32]) -> Option<u32> {
    tempo_scored(x).filter(|(_, c)| *c >= MIN_TEMPO_CONFIDENCE).map(|(b, _)| b)
}

/// The likeliest tempo with its confidence (winning score over the range's mean).
pub fn tempo_scored(x: &[f32]) -> Option<(u32, f32)> {
    const HOP: usize = 256;
    let fps = RATE / HOP as f32;
    let spec = spectrogram(x, 1024, HOP, 372); // up to 8 kHz
    if spec.len() < (fps * 4.0) as usize {
        return None;
    }
    // Onset strength: rise in log magnitude, summed over bins.
    let logs: Vec<Vec<f32>> = spec.iter().map(|f| f.iter().map(|m| (1.0 + 100.0 * m).ln()).collect()).collect();
    let mut onset: Vec<f32> = std::iter::once(0.0)
        .chain(logs.windows(2).map(|w| w[1].iter().zip(&w[0]).map(|(b, a)| (b - a).max(0.0)).sum()))
        .collect();
    // Remove the slow trend so only the pulses remain.
    let half = (fps * 0.25) as usize;
    let smooth: Vec<f32> = (0..onset.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(onset.len()));
            onset[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect();
    for (o, s) in onset.iter_mut().zip(&smooth) {
        *o = (*o - s).max(0.0);
    }

    // Autocorrelation up to 4 beats at the slowest tempo searched.
    let max_lag = (60.0 * fps / 40.0 * 4.0) as usize + 2;
    let n = onset.len();
    let acf: Vec<f32> = (0..max_lag.min(n)).map(|l| onset[..n - l].iter().zip(&onset[l..]).map(|(a, b)| a * b).sum()).collect();
    if acf[0] <= 0.0 {
        return None;
    }
    let at = |lag: f32| -> f32 {
        let i = lag.floor() as usize;
        if i + 1 >= acf.len() {
            return 0.0;
        }
        let f = lag - i as f32;
        acf[i] * (1.0 - f) + acf[i + 1] * f
    };
    let comb = |bpm: f32, beats: usize| -> f32 {
        let period = 60.0 * fps / bpm;
        (1..=beats).map(|k| at(period * k as f32)).sum::<f32>() / beats as f32
    };

    // Coarse search in the named range, a mild preference for ~130 breaks octave ties.
    let score = |b: f32| comb(b, 4) * (1.0 - 0.15 * ((b / 140.0).log2()).powi(2));
    let (mut best, mut best_score) = (0.0f32, f32::MIN);
    let mut total = 0.0;
    let mut count = 0.0;
    let mut b = 85.0;
    while b <= 180.0 {
        let s = score(b);
        total += s;
        count += 1.0;
        if s > best_score {
            (best, best_score) = (b, s);
        }
        b += 0.25;
    }
    if best_score <= 0.0 {
        return None;
    }
    let confidence = best_score / (total / count);
    // Refine on 16 beats, which pins the period far more precisely.
    let (mut fine, mut fine_score) = (best, f32::MIN);
    let mut b = best - 1.5;
    while b <= best + 1.5 {
        let s = comb(b, 16);
        if s > fine_score {
            (fine, fine_score) = (b, s);
        }
        b += 0.02;
    }
    Some((fine.round() as u32, confidence))
}

// ---------- key ----------

const PROFILES: [([f32; 12], [f32; 12]); 3] = [
    // Krumhansl-Kessler
    (
        [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88],
        [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17],
    ),
    // Temperley (Kostka-Payne)
    (
        [0.748, 0.060, 0.488, 0.082, 0.670, 0.460, 0.096, 0.715, 0.104, 0.366, 0.057, 0.400],
        [0.712, 0.084, 0.474, 0.618, 0.049, 0.460, 0.105, 0.747, 0.404, 0.067, 0.133, 0.330],
    ),
    // Shaath (KeyFinder)
    (
        [6.6, 2.0, 3.5, 2.3, 4.6, 4.0, 2.5, 5.2, 2.4, 3.7, 2.3, 3.4],
        [6.5, 2.7, 3.5, 5.4, 2.6, 3.5, 2.5, 5.2, 4.0, 2.7, 4.3, 3.2],
    ),
];

fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (ma, mb) = (a.iter().sum::<f32>() / 12.0, b.iter().sum::<f32>() / 12.0);
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        num += (a[i] - ma) * (b[i] - mb);
        da += (a[i] - ma).powi(2);
        db += (b[i] - mb).powi(2);
    }
    if da <= 0.0 || db <= 0.0 { 0.0 } else { num / (da * db).sqrt() }
}

/// Correlation of a chroma vector with all 24 keys (0-11 major, 12-23 minor).
fn key_scores(chroma: &[f32; 12]) -> [f32; 24] {
    let mut out = [0.0; 24];
    for (major, minor) in PROFILES {
        for t in 0..12 {
            let rot = |p: &[f32; 12]| std::array::from_fn::<f32, 12, _>(|i| p[(i + 12 - t) % 12]);
            out[t] += pearson(chroma, &rot(&major)) / PROFILES.len() as f32;
            out[12 + t] += pearson(chroma, &rot(&minor)) / PROFILES.len() as f32;
        }
    }
    out
}

/// Harmonics 2-8 as (semitones up, share of the fundamental's level taken off). Strong on
/// purpose: distorted 808s and saws carry loud overtones (measured best at these values).
const HARMONICS: [(usize, f32); 7] = [(12, 1.0), (19, 0.8), (24, 0.6), (28, 0.5), (31, 0.4), (34, 0.3), (36, 0.24)];
/// How much more the minor-scale notes count when deciding major or minor.
const MINOR_LEAN: f32 = 1.3;

fn median(v: &mut [f32]) -> f32 {
    let mid = v.len() / 2;
    *v.select_nth_unstable_by(mid, |a, b| a.total_cmp(b)).1
}

/// The key; None when no key fits clearly (drums only, speech, atonal).
pub fn key(x: &[f32]) -> Option<Key> {
    key_scored(x).filter(|(_, m)| *m >= MIN_KEY_MARGIN).map(|(k, _)| k)
}

/// The likeliest key with how clearly major and minor differed.
pub fn key_scored(x: &[f32]) -> Option<(Key, f32)> {
    const SIZE: usize = 16384;
    const HOP: usize = 4096;
    let bin_hz = RATE / SIZE as f32;
    let (lo, hi) = ((27.0 / bin_hz) as usize, (2100.0 / bin_hz) as usize);
    let spec = spectrogram(x, SIZE, HOP, hi + 2);
    if spec.len() < 8 {
        return None;
    }

    // Harmonic part: median over time (sustained) vs over frequency (percussive).
    let (nf, nb) = (spec.len(), hi + 2);
    let mut harm = vec![vec![0.0f32; nb]; nf];
    let mut buf = Vec::with_capacity(17);
    for t in 0..nf {
        for k in lo..=hi {
            buf.clear();
            buf.extend((t.saturating_sub(8)..(t + 9).min(nf)).map(|u| spec[u][k]));
            let h = median(&mut buf);
            buf.clear();
            buf.extend((k.saturating_sub(8)..(k + 9).min(nb)).map(|j| spec[t][j]));
            let p = median(&mut buf);
            let m = spec[t][k];
            harm[t][k] = if h + p > 0.0 { m * h * h / (h * h + p * p) } else { 0.0 };
        }
    }

    // Tuning: where the spectral peaks sit between semitones (circular mean, in cents).
    let (mut cs, mut sn) = (0.0f32, 0.0f32);
    for f in &harm {
        for k in lo + 1..hi {
            let (a, b, c) = (f[k - 1], f[k], f[k + 1]);
            if b > a && b > c && b > 0.0 {
                let d = 0.5 * (a - c) / (a - 2.0 * b + c);
                let freq = (k as f32 + d) * bin_hz;
                let cents = 1200.0 * (freq / 440.0).log2();
                let ang = cents / 100.0 * 2.0 * std::f32::consts::PI;
                cs += b * ang.cos();
                sn += b * ang.sin();
            }
        }
    }
    let tuning = sn.atan2(cs) / (2.0 * std::f32::consts::PI) * 100.0; // cents, -50..50
    let reference = 440.0 * 2f32.powf(tuning / 1200.0);
    // Spectral peaks at their interpolated pitch, one value per semitone. Reading bins
    // instead smears a bass note over its neighbours, and high notes (which span many
    // more bins) would drown the bass in overtones.
    let midi_of = |hz: f32| 69.0 + 12.0 * (hz / reference).log2();
    let (n_lo, n_hi) = (midi_of(lo as f32 * bin_hz).ceil() as i32, midi_of(hi as f32 * bin_hz).floor() as i32);
    let mut vals = vec![0.0f32; (n_hi - n_lo + 1).max(0) as usize];

    // 10 s segments, each normalised, summed by loudness; scores summed over segments.
    let per_seg = ((10.0 * RATE) / HOP as f32) as usize;
    let mut total = [0.0f32; 24];
    let mut sum = [0.0f32; 12];
    let mut any = false;
    for seg in harm.chunks(per_seg.max(1)) {
        let mut chroma = [0.0f32; 12];
        for f in seg {
            vals.fill(0.0);
            for k in lo + 1..hi {
                let (a, b, c) = (f[k - 1], f[k], f[k + 1]);
                if b > a && b >= c && b > 0.0 {
                    let d = 0.5 * (a - c) / (a - 2.0 * b + c);
                    let m = midi_of((k as f32 + d) * bin_hz);
                    let n = m.round() as i32;
                    let w = (std::f32::consts::PI * (m - n as f32)).cos().powi(2);
                    if let Some(v) = usize::try_from(n - n_lo).ok().and_then(|i| vals.get_mut(i)) {
                        *v = v.max((b - 0.25 * (a - c) * d) * w);
                    }
                }
            }
            // Take each note's overtones (octave, fifth, 2nd octave, major third...) off the
            // notes above it, lowest first: a bass note's 5th harmonic otherwise reads as a
            // major third and turns minor keys major.
            for i in 0..vals.len() {
                let v = vals[i];
                for (up, share) in HARMONICS {
                    if let Some(h) = vals.get_mut(i + up) {
                        *h = (*h - v * share).max(0.0);
                    }
                }
            }
            for (i, v) in vals.iter().enumerate() {
                let n = n_lo + i as i32;
                // Above C6 it's mostly overtones; trust it less.
                chroma[n.rem_euclid(12) as usize] += if n < 84 { *v } else { 0.6 * v };
            }
        }
        let energy: f32 = chroma.iter().sum();
        if energy <= 0.0 {
            continue;
        }
        any = true;
        for (t, s) in total.iter_mut().zip(key_scores(&chroma)) {
            *t += s * energy;
        }
        for (s, c) in sum.iter_mut().zip(chroma) {
            *s += c;
        }
    }
    if !any {
        return None;
    }
    // Tonic from the best-fitting key profile; major or minor from the notes that tell
    // them apart (3rd, 6th and 7th above the tonic). The profiles alone call minor
    // melodies major when they skip the third. Measured on 250 producer loops named with
    // their key (97% minor), leaning towards minor by MINOR_LEAN gained ~5 points.
    let t = (0..24).max_by(|a, b| total[*a].total_cmp(&total[*b]))? % 12;
    let c = |i: usize| sum[(t + i) % 12];
    let (minor, major) = ((c(3) + c(8) + c(10)) * MINOR_LEAN, c(4) + c(9) + c(11));
    let margin = (minor - major).abs() / (minor + major).max(1e-9);
    Some((Key { tonic: t as u8, minor: minor >= major }, margin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_key_notations() {
        let k = |s: &str| parse_key(s).map(|k| k.name());
        assert_eq!(k("Am"), Some("Am"));
        assert_eq!(k("A minor"), Some("Am"));
        assert_eq!(k("G#Min"), Some("G#m"));
        assert_eq!(k("CsMin"), Some("C#m"));
        assert_eq!(k("dmin"), Some("Dm"));
        assert_eq!(k("Bb"), Some("Bb"));
        assert_eq!(k("Bmin"), Some("Bm"));
        assert_eq!(k("F# major"), Some("F#"));
        assert_eq!(k("8A"), Some("Am"));
        assert_eq!(k("8B"), Some("C"));
        assert_eq!(k("1A"), Some("G#m"));
        assert_eq!(k("12B"), Some("E"));
        assert_eq!(k("hello"), None);
    }

    #[test]
    fn tags_win_and_names_stay_tidy() {
        let err = "  Metadata:\n    TBPM            : 128\n    TKEY            : 5A\n  Duration: 00:03:00.00";
        let (b, k) = parse_tags(err);
        assert_eq!((b, k.map(|k| k.name())), (Some(128), Some("Cm")));
        assert_eq!(tagged_stem("Song", Some(124), parse_key("Am")), "Song 124 BPM Am");
        assert_eq!(tagged_stem("Song", None, parse_key("Am")), "Song Am");
        assert_eq!(tagged_stem("Song", Some(90), None), "Song 90 BPM");
        assert_eq!(tagged_stem("Loop 140BPM", Some(140), parse_key("Gm")), "Loop 140BPM Gm");
        assert_eq!(tagged_stem("Song", None, None), "Song");
    }

    /// A rich tone (decaying harmonics) at `midi`, `secs` long, added into `out` at `start`.
    fn note(out: &mut [f32], start: usize, secs: f32, midi: f32, gain: f32) {
        let f = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
        let n = (secs * RATE) as usize;
        for i in 0..n.min(out.len().saturating_sub(start)) {
            let t = i as f32 / RATE;
            let env = (1.0 - (-t * 40.0).exp()) * (-t * 1.2).exp();
            let s: f32 = (1..=6).map(|h| (2.0 * std::f32::consts::PI * f * h as f32 * t).sin() / h as f32).sum();
            out[start + i] += gain * env * s;
        }
    }

    /// Chords (as MIDI note lists, one per bar of 4 beats) over a kick and hats at `bpm`.
    fn song(bpm: f32, chords: &[&[f32]], bars: usize) -> Vec<f32> {
        let beat = 60.0 / bpm;
        let len = (bars as f32 * 4.0 * beat * RATE) as usize;
        let mut x = vec![0.0f32; len];
        for bar in 0..bars {
            let at = (bar as f32 * 4.0 * beat * RATE) as usize;
            for &m in chords[bar % chords.len()] {
                note(&mut x, at, 4.0 * beat, m, 0.08);
            }
            note(&mut x, at, 4.0 * beat, chords[bar % chords.len()][0] - 12.0, 0.12);
        }
        let mut seed = 1u32;
        for b in 0..(bars * 4) {
            let at = (b as f32 * beat * RATE) as usize;
            // Kick: a falling sine thump.
            for i in 0..(0.15 * RATE) as usize {
                if at + i < len {
                    let t = i as f32 / RATE;
                    x[at + i] += 0.6 * (-t * 25.0).exp() * (2.0 * std::f32::consts::PI * (50.0 + 80.0 * (-t * 30.0).exp()) * t).sin();
                }
            }
            // Hat on the off-beat: short noise.
            let off = at + (beat * 0.5 * RATE) as usize;
            for i in 0..(0.03 * RATE) as usize {
                if off + i < len {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    x[off + i] += 0.15 * (-(i as f32) / RATE * 150.0).exp() * ((seed >> 9) as f32 / (1u32 << 23) as f32 - 1.0);
                }
            }
        }
        x
    }

    #[test]
    fn finds_tempo_of_drum_patterns() {
        let c = [60.0, 64.0, 67.0];
        for bpm in [90.0, 124.0, 140.0, 174.0] {
            let x = song(bpm, &[&c], 24);
            assert_eq!(tempo(&x), Some(bpm as u32), "at {bpm}");
        }
        assert_eq!(tempo(&vec![0.0; 22050 * 10]), None);
    }

    #[test]
    fn finds_major_and_minor_keys() {
        // C major: C F G C.  A minor: Am Dm E Am (harmonic minor's raised G#).
        let c_major: [&[f32]; 4] = [&[60.0, 64.0, 67.0], &[65.0, 69.0, 72.0], &[67.0, 71.0, 74.0], &[60.0, 64.0, 67.0]];
        let a_minor: [&[f32]; 4] = [&[57.0, 60.0, 64.0], &[62.0, 65.0, 69.0], &[64.0, 68.0, 71.0], &[57.0, 60.0, 64.0]];
        assert_eq!(key(&song(120.0, &c_major, 16)).map(|k| k.name()), Some("C"));
        assert_eq!(key(&song(120.0, &a_minor, 16)).map(|k| k.name()), Some("Am"));
        // Transposed and detuned by +30 cents: F#m.
        let fs_minor: Vec<Vec<f32>> = a_minor.iter().map(|ch| ch.iter().map(|m| m - 3.0 + 0.3).collect()).collect();
        let refs: Vec<&[f32]> = fs_minor.iter().map(|v| v.as_slice()).collect();
        assert_eq!(key(&song(100.0, &refs, 16)).map(|k| k.name()), Some("F#m"));
    }
}
