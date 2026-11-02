//! Tempo (BPM) of a piece of music, for naming converted files ("Song 124 BPM.mp3"). A BPM
//! tag written by DJ software wins when present; otherwise the sound is analysed: FFmpeg
//! decodes up to four minutes to mono 22.05 kHz, then spectral-flux onsets, autocorrelation
//! and a comb over the beat period and its multiples, kept in the 85-180 BPM range producers
//! name tracks in (trap at 150, not 75); for loops, the whole-bar tempo their length implies,
//! when the beat fits it. Measured with examples/analyze_eval.rs: 85.5% exact on 1005
//! producer loops named with their tempo; on 30 s excerpts of songs (GTZAN) 43% exact, 58%
//! within 1 BPM. The answer is dropped when the beat is weak, so a guess never ends up in a name.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub const RATE: f32 = 22050.0;
const MAX_SECONDS: u32 = 240;

/// What the file's tags say about its tempo ("TBPM" / "BPM").
pub fn parse_tags(stderr: &str) -> Option<u32> {
    stderr.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        matches!(name.trim().to_ascii_lowercase().as_str(), "tbpm" | "bpm")
            .then(|| value.trim().parse::<f32>().ok().filter(|b| (40.0..=300.0).contains(b)).map(|b| b.round() as u32))
            .flatten()
    })
}

/// Tempo of `path`, its tag first. None when there's no tag and no steady beat.
pub fn analyze(ffmpeg: &Path, path: &Path) -> Result<Option<u32>> {
    let (pcm, tag) = decode(ffmpeg, path)?;
    Ok(tag.or_else(|| tempo(&pcm)))
}

/// Up to four minutes of `path` as mono samples at [`RATE`], plus its BPM tag.
pub fn decode(ffmpeg: &Path, path: &Path) -> Result<(Vec<f32>, Option<u32>)> {
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
    let tag = parse_tags(&err.join().unwrap_or_default());
    let pcm: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok((pcm, tag))
}

/// A key as producers write it in names ("Dphy", "G#Min", "abmin", "CSMIN"): only to tell a
/// bare tempo beside it ("SCH 146 Dphy") from any other number. Keys are left as written.
fn is_key_word(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    let Some(rest) = l.strip_prefix(['a', 'b', 'c', 'd', 'e', 'f', 'g']) else { return false };
    let rest = rest.strip_prefix(['#', 'b', 's']).filter(|r| !r.is_empty()).unwrap_or(rest);
    t.len() <= 9 && ["min", "minor", "maj", "major", "phy", "phry", "moll", "dur"].contains(&rest)
}

/// Where a file name states a tempo: (byte range to replace, the tempo). Understands our own
/// "Song 124 BPM", "140BPM", "bpm 130" and producer names like "SCH 146 Dphy" or "146CbMIN"
/// (a tempo run into a key); a number alone ("Track 101") is not a tempo.
fn tempo_spans(stem: &str) -> Vec<(std::ops::Range<usize>, u32)> {
    let mut toks: Vec<(usize, &str)> = Vec::new();
    let mut start = None;
    for (i, c) in stem.char_indices() {
        if c.is_ascii_alphanumeric() || c == '#' {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            toks.push((s, &stem[s..i]));
        }
    }
    if let Some(s) = start {
        toks.push((s, &stem[s..]));
    }
    let lower: Vec<String> = toks.iter().map(|t| t.1.to_ascii_lowercase()).collect();
    let number = |i: usize| lower.get(i).and_then(|t| t.parse::<u32>().ok()).filter(|n| (50..=250).contains(n));
    let span = |i: usize| toks[i].0..toks[i].0 + toks[i].1.len();
    let mut out = Vec::new();
    for i in 0..toks.len() {
        let l = lower[i].as_str();
        if l == "bpm" {
            // "124 BPM" or "bpm 130": the word and its number go together.
            if let Some(j) = [i.wrapping_sub(1), i + 1].into_iter().find(|&j| number(j).is_some()) {
                out.push((span(j.min(i)).start..span(j.max(i)).end, number(j).unwrap_or_default()));
            }
        } else if let Some(n) = l.strip_suffix("bpm").and_then(|d| d.parse::<u32>().ok()) {
            out.push((span(i), n));
        } else if is_key_word(toks[i].1) {
            if let Some(j) = [i.wrapping_sub(1), i + 1].into_iter().find(|&j| number(j).is_some() && lower.get(j + 1).is_none_or(|w| w != "bpm") && (j == 0 || lower[j - 1] != "bpm")) {
                out.push((span(j), number(j).unwrap_or_default()));
            }
        } else {
            let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
            if (2..=3).contains(&digits) && is_key_word(&toks[i].1[digits..]) {
                let s = toks[i].0;
                out.extend(toks[i].1[..digits].parse::<u32>().ok().map(|n| (s..s + digits, n)));
            }
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Whether a file name already states a tempo.
pub fn name_has_tempo(stem: &str) -> bool {
    !tempo_spans(stem).is_empty()
}

/// "Song" -> "Song 124 BPM"; as it was when the tempo is unknown or already in the name.
pub fn tagged_stem(stem: &str, bpm: Option<u32>) -> String {
    match bpm.filter(|_| !name_has_tempo(stem)) {
        Some(b) => format!("{stem} {b} BPM"),
        None => stem.to_string(),
    }
}

/// The name with its tempo set to `bpm`, written where the old one stood ("ANIME 140 gsMIN x"
/// -> "ANIME 141 BPM gsMIN x") or added at the end. None when the name already says it.
pub fn retag(stem: &str, bpm: u32) -> Option<String> {
    let spans = tempo_spans(stem);
    if !spans.is_empty() && spans.iter().all(|(_, n)| *n == bpm) {
        return None;
    }
    let tag = format!("{bpm} BPM");
    let Some(first) = spans.first() else { return Some(format!("{stem} {tag}")) };
    let mut out = stem[..first.0.start].to_string();
    out.push_str(&tag);
    // A tempo run into a key ("146CbMIN") gets a space before the key.
    if stem[first.0.end..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.push(' ');
    }
    let mut at = first.0.end;
    for (r, _) in &spans[1..] {
        out.push_str(&stem[at..r.start]);
        at = r.end;
    }
    out.push_str(&stem[at..]);
    // Tidy the gaps older tags left: "a - 126 - Fmin" stays "a - 126 BPM - Fmin".
    let mut out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    while out.contains(" - - ") {
        out = out.replace(" - - ", " - ");
    }
    Some(out.trim_matches(|c: char| c == ' ' || c == '-').to_string())
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

/// Beats per minute, rounded; None when there's no steady beat.
pub fn tempo(x: &[f32]) -> Option<u32> {
    tempo_scored(x).filter(|(_, c)| *c >= MIN_TEMPO_CONFIDENCE).map(|(b, _)| b)
}

/// Frames per second of the onset curve [`Pulse`] works on.
pub const PULSE_FPS: f32 = RATE / 256.0;

/// How strongly the onsets repeat at every lag (autocorrelation, in onset frames).
pub struct Pulse {
    acf: Vec<f32>,
}

impl Pulse {
    /// Spectral-flux onsets of `x`, autocorrelated up to 4 beats at 40 BPM.
    pub fn of(x: &[f32]) -> Option<Self> {
        const HOP: usize = 256;
        let fps = PULSE_FPS;
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
        (acf.first().copied().unwrap_or(0.0) > 0.0).then_some(Pulse { acf })
    }

    fn at(&self, lag: f32) -> f32 {
        let i = lag.floor() as usize;
        if i + 1 >= self.acf.len() {
            return 0.0;
        }
        let f = lag - i as f32;
        self.acf[i] * (1.0 - f) + self.acf[i + 1] * f
    }

    /// How well a beat at `bpm` fits, averaged over its first `beats` multiples.
    pub fn comb(&self, bpm: f32, beats: usize) -> f32 {
        let period = 60.0 * PULSE_FPS / bpm;
        (1..=beats).map(|k| self.at(period * k as f32)).sum::<f32>() / beats as f32
    }
}

/// The likeliest tempo with its confidence (winning score over the range's mean).
pub fn tempo_scored(x: &[f32]) -> Option<(u32, f32)> {
    let pulse = Pulse::of(x)?;
    let comb = |bpm: f32, beats: usize| pulse.comb(bpm, beats);

    // Coarse search in the named range, a mild preference for ~130 breaks octave ties.
    let prior = |b: f32| 1.0 - 0.15 * ((b / 140.0).log2()).powi(2);
    let score = |b: f32| comb(b, 4) * prior(b);
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
    Some((loop_tempo(x.len() as f32 / RATE, fine, fine_score, &pulse, prior).unwrap_or(fine.round() as u32), confidence))
}

/// Loops are cut to a whole number of bars, so their length gives the tempo exactly: of the
/// tempos that fit the length, the one whose beat fits the onsets nearly as well as the best
/// found (`found`, scoring `found_score` on 16 beats). Rules out triplet-feel 2/3 tempos and
/// off-by-one roundings. Measured on 1003 producer loops named with their tempo: 77% -> 85%
/// exact; songs and loops that aren't whole bars come out as before.
fn loop_tempo(secs: f32, found: f32, found_score: f32, pulse: &Pulse, prior: impl Fn(f32) -> f32) -> Option<u32> {
    if !(2.0..=LOOP_MAX_SECS).contains(&secs) || found_score <= 0.0 {
        return None;
    }
    let mut pick: Option<(f32, f32)> = None;
    for bars in 1..=64u32 {
        let tempo = 240.0 * bars as f32 / secs;
        // Half or double time stays the coarse search's call (its ~130 preference): the
        // length can't tell 90 from 180, and boom-bap at 90 must not turn into 180.
        let ratio = tempo / found;
        if !(70.0..=190.0).contains(&tempo) || (1.9..2.1).contains(&ratio) || (0.475..0.525).contains(&ratio) {
            continue;
        }
        // A little either side: decoders pad the start and end of compressed files.
        let fit = (-6..=6).map(|i| pulse.comb(tempo + i as f32 * 0.05, 16)).fold(f32::MIN, f32::max);
        // 1, 2, 4, 8... bars are the usual loop lengths; 3, 6 or 12 bars happen, but rarely.
        let usual = if bars.is_power_of_two() { 1.0 } else { ODD_BARS };
        let rel = fit / found_score * usual * prior(tempo) / prior(found);
        if pick.is_none_or(|(_, r)| rel > r) {
            pick = Some((tempo, rel));
        }
    }
    pick.filter(|(_, rel)| *rel >= LOOP_FIT).map(|(tempo, _)| tempo.round() as u32)
}

/// Longest file treated as a loop.
const LOOP_MAX_SECS: f32 = 90.0;
/// How well a whole-bar tempo must fit, relative to the best tempo found.
const LOOP_FIT: f32 = 0.7;
/// Weight of bar counts other than powers of two.
const ODD_BARS: f32 = 0.6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_win_and_names_stay_tidy() {
        let err = "  Metadata:\n    TBPM            : 128\n    TKEY            : 5A\n  Duration: 00:03:00.00";
        assert_eq!(parse_tags(err), Some(128));
        assert_eq!(parse_tags("  Duration: 00:03:00.00"), None);
        assert_eq!(tagged_stem("Song", Some(124)), "Song 124 BPM");
        assert_eq!(tagged_stem("Song", None), "Song");
        // Already named (by us or by the producer): nothing doubled, keys left as written.
        assert_eq!(tagged_stem("Beat 117 BPM C", Some(117)), "Beat 117 BPM C");
        assert_eq!(tagged_stem("Loop 140BPM", Some(140)), "Loop 140BPM");
        assert_eq!(tagged_stem("SCH 146 Dphy @1onazz", Some(146)), "SCH 146 Dphy @1onazz");
        assert_eq!(tagged_stem("Melody 140BPM G#Min", Some(141)), "Melody 140BPM G#Min");
        assert_eq!(tagged_stem("MP 127Cbmin @1onazz", Some(127)), "MP 127Cbmin @1onazz");
        // A number alone is not a tempo.
        assert_eq!(tagged_stem("Track 101", Some(120)), "Track 101 120 BPM");
        assert_eq!(tagged_stem("Track 101 x Amin", Some(140)), "Track 101 x Amin 140 BPM");
    }

    #[test]
    fn retags_the_tempo_in_place() {
        // A wrong tempo is replaced where it stood; keys and everything else stay.
        assert_eq!(retag("ANIME 140 gsMIN 1onazzz", 141).as_deref(), Some("ANIME 141 BPM gsMIN 1onazzz"));
        assert_eq!(retag("(hard, dark) gnarly 156bpm abmin @1hellion_", 155).as_deref(), Some("(hard, dark) gnarly 155 BPM abmin @1hellion_"));
        assert_eq!(retag("betrayal - 126 - Fmin @tr1pleq", 127).as_deref(), Some("betrayal - 127 BPM - Fmin @tr1pleq"));
        assert_eq!(retag("MP 127Cbmin @1onazz", 128).as_deref(), Some("MP 128 BPM Cbmin @1onazz"));
        assert_eq!(retag("@nioh - Loyal Member bpm 130 key D min", 131).as_deref(), Some("@nioh - Loyal Member 131 BPM key D min"));
        assert_eq!(retag("JRAZZZY 119 BPM DbMIN @1onazz", 120).as_deref(), Some("JRAZZZY 120 BPM DbMIN @1onazz"));
        // Two tempos in a name (an older tag too): one is left, at the first.
        assert_eq!(retag("CLOUD 140 CSMIN 140 BPM Amaj", 141).as_deref(), Some("CLOUD 141 BPM CSMIN Amaj"));
        // Already right: nothing to save.
        assert_eq!(retag("ANIME 140 gsMIN 1onazzz", 140), None);
        assert_eq!(retag("Song 124 BPM", 124), None);
        // Nothing in the name yet: added at the end.
        assert_eq!(retag("I Am Here", 120).as_deref(), Some("I Am Here 120 BPM"));
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
    fn whole_bar_loops_get_their_exact_tempo() {
        // 8 bars at 147 with triplet hats on top: the triplets alone pull towards 98 (2/3).
        let bpm = 147.0;
        let mut x = song(bpm, &[&[57.0, 60.0, 64.0]], 8);
        let step = 60.0 / bpm / 3.0;
        for k in 0..(8 * 4 * 3) {
            let at = (k as f32 * step * RATE) as usize;
            for i in 0..(0.02 * RATE) as usize {
                if let Some(v) = x.get_mut(at + i) {
                    *v += 0.25 * (-(i as f32) / RATE * 200.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 };
                }
            }
        }
        assert_eq!(tempo(&x), Some(147));
    }
}
