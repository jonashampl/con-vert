//! Audio: converts audio and video files to audio formats with the bundled FFmpeg
//! (an LGPL build shipped next to the app as ffmpeg.exe). Video files give up their
//! sound track. Results are new files, like every other action.

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};

use crate::actions::{Outcome, Progress};
use crate::convert::name;
use crate::naming::{self, OutputDir};

/// Convert targets in ring order (clockwise from the top), with their encoder settings.
/// MP3 V2 (~190 kbps) and 256 kbps AAC are transparent for nearly everyone; FLAC and WAV
/// are lossless.
pub const TARGETS: [(&str, &[&str]); 7] = [
    ("mp3", &["-c:a", "libmp3lame", "-q:a", "2"]),
    ("wav", &["-c:a", "pcm_s16le"]),
    ("flac", &["-c:a", "flac"]),
    ("m4a", &["-c:a", "aac", "-b:a", "256k"]),
    ("ogg", &["-c:a", "libvorbis", "-q:a", "6"]),
    ("opus", &["-c:a", "libopus", "-b:a", "160k"]),
    ("aac", &["-c:a", "aac", "-b:a", "256k", "-f", "adts"]),
];

/// Files we take in: audio, plus video for its sound track.
const AUDIO_IN: [&str; 11] = ["mp3", "wav", "flac", "m4a", "ogg", "oga", "opus", "aac", "wma", "aif", "aiff"];
const VIDEO_IN: [&str; 12] = ["mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "3gp"];

fn ext(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

/// An audio or video file con/vert can take the sound from.
pub fn is_media(path: &Path) -> bool {
    ext(path).is_some_and(|e| AUDIO_IN.contains(&e.as_str()) || VIDEO_IN.contains(&e.as_str()))
}

/// The audio target id a file already is ("mp3"), so the ring can leave it out.
pub fn own_target(path: &Path) -> Option<&'static str> {
    let e = ext(path)?;
    TARGETS.iter().map(|t| t.0).find(|id| *id == e || (e == "oga" && *id == "ogg"))
}

pub fn is_target(id: &str) -> bool {
    TARGETS.iter().any(|t| t.0 == id)
}

/// ffmpeg.exe next to the running app; `CONVERT_FFMPEG` overrides it (tests, development).
pub fn ffmpeg() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CONVERT_FFMPEG").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let p = std::env::current_exe().ok()?.parent()?.join("ffmpeg.exe");
    p.is_file().then_some(p)
}

/// FFmpeg arguments: first audio track only, tags kept, progress on stdout, never
/// overwrite (`-n`; the output is always a fresh temporary name anyway).
pub fn args(input: &Path, output: &Path, target: &str) -> Vec<OsString> {
    let codec = TARGETS.iter().find(|t| t.0 == target).map(|t| t.1).unwrap_or(&[]);
    let mut a: Vec<OsString> = ["-hide_banner", "-nostdin", "-n", "-i"].iter().map(OsString::from).collect();
    a.push(input.into());
    a.extend(["-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "0"].iter().map(OsString::from));
    a.extend(codec.iter().map(OsString::from));
    a.extend(["-progress", "pipe:1", "-nostats"].iter().map(OsString::from));
    a.push(output.into());
    a
}

/// Seconds from FFmpeg's "  Duration: 00:03:21.45, start: ..." line.
pub fn parse_duration(line: &str) -> Option<f64> {
    let t = line.trim().strip_prefix("Duration:")?.trim().split(',').next()?;
    let mut parts = t.split(':').map(|p| p.parse::<f64>().ok());
    let (h, m, s) = (parts.next()??, parts.next()??, parts.next()??);
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Seconds done from a "-progress" line ("out_time_us=12345678").
pub fn parse_progress(line: &str) -> Option<f64> {
    let us: f64 = line.strip_prefix("out_time_us=")?.trim().parse().ok()?;
    (us >= 0.0).then_some(us / 1_000_000.0)
}

/// A short, plain reason from FFmpeg's error output.
fn explain(stderr: &[String]) -> String {
    let all = stderr.join("\n");
    if all.contains("matches no streams") || all.contains("does not contain any stream") {
        "this file has no sound".into()
    } else if all.contains("Invalid data found") || all.contains("moov atom not found") {
        "this file is damaged or isn't an audio or video file".into()
    } else {
        stderr.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_else(|| "FFmpeg failed".into())
    }
}

/// Runs FFmpeg on one file into `output`, reporting 0..1 as it goes.
fn encode(ffmpeg: &Path, input: &Path, output: &Path, target: &str, cancel: &AtomicBool, on_fraction: &mut dyn FnMut(f32)) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = Command::new(ffmpeg)
        .args(args(input, output, target))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("can't start FFmpeg")?;

    // stderr carries the duration (needed for the fraction) and any error text.
    let duration = Arc::new(Mutex::new(None::<f64>));
    let stderr = child.stderr.take().map(BufReader::new);
    let dur = duration.clone();
    let err_reader = std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        for line in stderr.into_iter().flat_map(|r| r.lines().map_while(Result::ok)) {
            if let Some(d) = parse_duration(&line) {
                dur.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(d);
            }
            tail.push(line);
            if tail.len() > 30 {
                tail.remove(0);
            }
        }
        tail
    });

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                break;
            }
            let total = *duration.lock().unwrap_or_else(|e| e.into_inner());
            if let (Some(done), Some(total)) = (parse_progress(&line), total.filter(|t| *t > 0.0)) {
                on_fraction((done / total).clamp(0.0, 1.0) as f32);
            }
        }
    }
    let status = child.wait().context("FFmpeg stopped unexpectedly")?;
    let tail = err_reader.join().unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        bail!("cancelled");
    }
    if !status.success() {
        bail!("{}", explain(&tail));
    }
    Ok(())
}

/// Converts each file to `target` ("mp3"), one after another (FFmpeg already uses every
/// core). Results go next to the originals as "<name>.<ext>".
pub fn run(paths: &[PathBuf], target: &str, out: &OutputDir, cancel: &AtomicBool, progress: &mut (dyn FnMut(Progress) + Send)) -> Outcome {
    let Some(ffmpeg) = ffmpeg() else {
        return Outcome::failed("FFmpeg is missing from the app folder; reinstall con/vert".into());
    };
    let total = paths.len();
    let mut outcome = Outcome::default();
    for (i, src) in paths.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Outcome::cancelled();
        }
        let detail = format!("{} of {total} file(s)", i + 1);
        progress(Progress { fraction: i as f32 / total as f32, detail: detail.clone() });
        let mut report = |f: f32| progress(Progress { fraction: (i as f32 + f) / total as f32, detail: detail.clone() });
        let saved = naming::write_new_with(&naming::target_dir(src, out), &naming::stem(src), target, |tmp| {
            encode(&ffmpeg, src, tmp, target, cancel, &mut report)
        });
        match saved {
            Ok(p) => outcome.outputs.push(p),
            Err(_) if cancel.load(Ordering::SeqCst) => return Outcome::cancelled(),
            Err(e) => outcome.errors.push(format!("{}: {e:#}", name(src))),
        }
    }
    outcome.summary = match outcome.outputs.as_slice() {
        [] => format!("Couldn't convert {} file(s)", outcome.errors.len()),
        [one] => format!("Saved {}", name(one)),
        many => format!("Saved {} files", many.len()),
    };
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_audio_and_video_inputs() {
        assert!(is_media(Path::new(r"C:\a\Song.MP3")));
        assert!(is_media(Path::new("clip.mkv")));
        assert!(!is_media(Path::new("photo.jpg")));
        assert!(!is_media(Path::new("noext")));
        assert_eq!(own_target(Path::new("x.oga")), Some("ogg"));
        assert_eq!(own_target(Path::new("x.mp4")), None);
    }

    #[test]
    fn ffmpeg_args_take_first_audio_track_and_never_overwrite() {
        let a: Vec<String> = args(Path::new("in.mp4"), Path::new("out.mp3"), "mp3").iter().map(|s| s.to_string_lossy().into_owned()).collect();
        let s = a.join(" ");
        assert!(s.starts_with("-hide_banner -nostdin -n -i in.mp4 -map 0:a:0 -vn"), "{s}");
        assert!(s.contains("-c:a libmp3lame -q:a 2"), "{s}");
        assert!(s.ends_with("-progress pipe:1 -nostats out.mp3"), "{s}");
        let aac = args(Path::new("a.wav"), Path::new("b.aac"), "aac").iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(aac.contains("-f adts"), "{aac}");
    }

    #[test]
    fn parses_duration_and_progress() {
        assert_eq!(parse_duration("  Duration: 00:03:21.50, start: 0.000000, bitrate: 320 kb/s"), Some(201.5));
        assert_eq!(parse_duration("  Duration: N/A, bitrate: N/A"), None);
        assert_eq!(parse_progress("out_time_us=12500000"), Some(12.5));
        assert_eq!(parse_progress("out_time_us=N/A"), None);
        assert_eq!(parse_progress("progress=continue"), None);
    }

    #[test]
    fn explains_common_failures() {
        assert_eq!(explain(&["Stream map '0:a:0' matches no streams.".into()]), "this file has no sound");
        assert_eq!(explain(&["x.mp4: Invalid data found when processing input".into()]), "this file is damaged or isn't an audio or video file");
    }

    /// End to end with a real FFmpeg; runs when CONVERT_FFMPEG points at ffmpeg.exe.
    #[test]
    fn converts_to_every_target() {
        let Some(ff) = ffmpeg() else {
            eprintln!("skipped: set CONVERT_FFMPEG to run the FFmpeg conversion test");
            return;
        };
        let dir = std::env::temp_dir().join(format!("convert-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("Tone.wav");
        let ok = Command::new(&ff)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
            .arg(&src)
            .status()
            .unwrap();
        assert!(ok.success());
        let cancel = AtomicBool::new(false);
        for (id, _) in TARGETS {
            let mut last = 0.0;
            let o = run(std::slice::from_ref(&src), id, &OutputDir::Beside, &cancel, &mut |p| last = p.fraction);
            assert!(o.errors.is_empty(), "{id}: {:?}", o.errors);
            let out = &o.outputs[0];
            let want = if id == "wav" { "Tone (2).wav".to_string() } else { format!("Tone.{id}") };
            assert_eq!(out.file_name().unwrap().to_string_lossy(), want);
            assert!(std::fs::metadata(out).unwrap().len() > 1000, "{id} output is empty");
            assert!(last > 0.0);
        }
        // A file without sound says so, and leaves nothing behind.
        let silent = dir.join("NoSound.mp4");
        Command::new(&ff)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=black:s=64x64:d=1", "-c:v", "mpeg4"])
            .arg(&silent)
            .status()
            .unwrap();
        let before = std::fs::read_dir(&dir).unwrap().count();
        let o = run(std::slice::from_ref(&silent), "mp3", &OutputDir::Beside, &cancel, &mut |_| {});
        assert!(o.errors[0].ends_with("this file has no sound"), "{:?}", o.errors);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
