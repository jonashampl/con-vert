//! Runs a ring action on dropped files and reports progress. One file failing never stops
//! the rest; originals are never modified.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::Serialize;

use crate::compress;
use crate::convert::{self, name};
use crate::formats::Format;
use crate::metadata;
use crate::naming::{self, OutputDir};

#[derive(Serialize, Clone, Debug)]
pub struct Progress {
    /// 0.0 ..= 1.0
    pub fraction: f32,
    /// "1 of 3 file(s)" or "Page 2 of 12".
    pub detail: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Outcome {
    pub outputs: Vec<PathBuf>,
    pub summary: String,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

impl Outcome {
    pub fn failed(msg: String) -> Outcome {
        Outcome { errors: vec![msg], summary: "Failed".into(), ..Default::default() }
    }

    pub fn cancelled() -> Outcome {
        Outcome { cancelled: true, summary: "Cancelled".into(), ..Default::default() }
    }
}

/// Whether an action runs straight away (vs. opening an editor window).
pub fn is_quick(action: &str) -> bool {
    action.starts_with("convert:")
        || action == "tool:compress"
        || action == "tool:metadata"
        || action.strip_prefix("tool:").is_some_and(|id| crate::audio::TOOLS.iter().any(|t| t.0 == id))
}

/// "JPG" or "MP3" for a convert action.
fn target_label(action: &str) -> Option<String> {
    let id = action.strip_prefix("convert:")?;
    (Format::from_id(id).is_some() || crate::audio::is_target(id)).then(|| id.to_ascii_uppercase())
}

/// Card title while running, e.g. "Converting to JPG".
pub fn title(action: &str) -> String {
    let action = action.strip_suffix("+zip").unwrap_or(action);
    match action {
        "tool:compress" => "Compressing".into(),
        "tool:metadata" => "Removing metadata".into(),
        "tool:zip" => "Zipping".into(),
        "tool:bpmkey" => "Reading tempo and key".into(),
        "tool:cover" => "Saving the cover".into(),
        a => match target_label(a) {
            Some(f) => format!("Converting to {f}"),
            None => "Working".into(),
        },
    }
}

/// Card title when finished, e.g. "Converted to JPG".
pub fn done_title(action: &str, o: &Outcome) -> String {
    let action = action.strip_suffix("+zip").unwrap_or(action);
    if o.cancelled {
        return "Cancelled".into();
    }
    if o.outputs.is_empty() {
        return if o.errors.is_empty() { "Nothing to do".into() } else { "Couldn't finish".into() };
    }
    match action {
        "tool:compress" => "Compressed".into(),
        "tool:metadata" => "Metadata removed".into(),
        "tool:zip" => "Zipped".into(),
        "tool:bpmkey" => "Tempo and key added".into(),
        "tool:cover" => "Cover saved".into(),
        a => match target_label(a) {
            Some(f) => format!("Converted to {f}"),
            None => "Done".into(),
        },
    }
}

pub fn run(action: &str, paths: &[PathBuf], out: &OutputDir, cancel: &AtomicBool, progress: &mut (dyn FnMut(Progress) + Send)) -> Outcome {
    // "convert:mp3+zip": convert as usual, then bundle the results into one ZIP.
    if let Some(base) = action.strip_suffix("+zip") {
        let mut o = run(base, paths, out, cancel, progress);
        if o.cancelled || o.outputs.len() < 2 {
            return o;
        }
        let folder = paths[0].parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Converted".into());
        let label = base.strip_prefix("convert:").unwrap_or(base).to_ascii_uppercase();
        let dir = o.outputs[0].parent().map(PathBuf::from).unwrap_or_else(naming::fallback_dir);
        match zip_into(&o.outputs, &dir, &format!("{folder} ({label})"), cancel, progress) {
            Ok(z) => {
                // Only the files this action just made; originals are never touched.
                for f in &o.outputs {
                    let _ = std::fs::remove_file(f);
                }
                o.summary = format!("Saved {} ({} files)", name(&z), o.outputs.len());
                o.outputs = vec![z];
            }
            Err(_) if cancel.load(Ordering::SeqCst) => {}
            Err(e) => o.errors.push(format!("ZIP: {e:#}")),
        }
        return o;
    }
    if let Some(id) = action.strip_prefix("convert:") {
        if crate::audio::is_target(id) {
            return crate::audio::run(paths, id, out, cancel, progress);
        }
        return match convert::target(id) {
            Ok(f) => convert::run(paths, f, out, cancel, progress),
            Err(e) => Outcome::failed(format!("{e:#}")),
        };
    }
    // Audio and video get the audio tools (same ids for compress and metadata).
    if let Some(id) = action.strip_prefix("tool:") {
        if !paths.is_empty() && paths.iter().all(|p| crate::audio::is_media(p)) {
            return crate::audio::tool(id, paths, out, cancel, progress);
        }
    }
    match action {
        "tool:compress" => per_file(paths, cancel, progress, |p| {
            let before = std::fs::metadata(p)?.len();
            match compress::compress(p)? {
                Some((bytes, fmt)) => {
                    let saved = naming::write_new(&naming::target_dir(p, out), &format!("{} Compressed", naming::stem(p)), fmt.ext(), &bytes)?;
                    Ok((Some(saved), (before, bytes.len() as u64)))
                }
                None => Ok((None, (before, before))),
            }
        })
        .map_summary(|o, sizes| {
            let (b, a): (u64, u64) = sizes.iter().fold((0, 0), |(x, y), (b, a)| (x + b, y + a));
            if o.outputs.is_empty() {
                "Already well compressed, nothing saved".into()
            } else {
                let pct = 100.0 * (1.0 - a as f64 / b as f64);
                format!("{} → {} (−{pct:.0}%)", compress::human(b), compress::human(a))
            }
        }),
        "tool:metadata" => per_file(paths, cancel, progress, |p| match metadata::strip(p)? {
            Some(s) => {
                let saved = naming::write_new(&naming::target_dir(p, out), &format!("{} Clean", naming::stem(p)), s.format.ext(), &s.bytes)?;
                Ok((Some(saved), (s.removed.join(", "), s.had_gps)))
            }
            None => Ok((None, (String::new(), false))),
        })
        .map_summary(|o, found| {
            let gps = found.iter().any(|(_, g)| *g);
            match (o.outputs.len(), found.iter().find(|(r, _)| !r.is_empty())) {
                (0, _) => "No metadata found, nothing saved".into(),
                (1, Some((what, _))) => format!("Removed {what}{}", if gps { ", incl. GPS location" } else { "" }),
                (n, _) => format!("Cleaned {n} files{}", if gps { ", incl. GPS location" } else { "" }),
            }
        }),
        other => Outcome::failed(format!("{other} isn't a quick action")),
    }
}

/// Puts `files`, as they are, into one new ZIP "<stem>.zip" in `dir`. Entries are stored
/// (audio and images barely compress), names made unique inside the archive.
pub(crate) fn zip_into(files: &[PathBuf], dir: &std::path::Path, stem: &str, cancel: &AtomicBool, progress: &mut (dyn FnMut(Progress) + Send)) -> anyhow::Result<PathBuf> {
    use anyhow::Context;
    let total = files.len();
    naming::write_new_with(dir, stem, "zip", |tmp| {
        let mut zw = zip::ZipWriter::new(std::fs::File::create(tmp).context("can't create the ZIP")?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
        let mut used = std::collections::HashSet::new();
        for (i, p) in files.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("cancelled");
            }
            progress(Progress { fraction: i as f32 / total as f32, detail: format!("Zipping {} of {total}", i + 1) });
            let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            let entry = (1..)
                .map(|n| if n == 1 { format!("{}{ext}", naming::stem(p)) } else { format!("{} ({n}){ext}", naming::stem(p)) })
                .find(|e| used.insert(e.to_lowercase()))
                .unwrap();
            zw.start_file(entry, opts)?;
            std::io::copy(&mut std::fs::File::open(p).with_context(|| format!("can't read {}", name(p)))?, &mut zw)?;
        }
        zw.finish()?;
        Ok(())
    })
}

/// Outcome plus per-file results, for building the summary afterwards.
struct PerFile<T>(Outcome, Vec<T>);

impl<T> PerFile<T> {
    fn map_summary(self, f: impl FnOnce(&Outcome, &[T]) -> String) -> Outcome {
        let PerFile(mut o, results) = self;
        if o.cancelled {
            return o;
        }
        o.summary = if o.outputs.is_empty() && !o.errors.is_empty() {
            // Every file failed: say so, don't claim "nothing to do".
            format!("Couldn't process {} file(s)", o.errors.len())
        } else {
            f(&o, &results)
        };
        o
    }
}

/// Runs `each` on every file on the worker pool, in parallel. Results (and any
/// output file) are gathered back in the order the files were dropped.
fn per_file<T: Send>(
    paths: &[PathBuf],
    cancel: &AtomicBool,
    progress: &mut (dyn FnMut(Progress) + Send),
    each: impl Fn(&PathBuf) -> anyhow::Result<(Option<PathBuf>, T)> + Sync,
) -> PerFile<T> {
    use rayon::prelude::*;
    let done = AtomicUsize::new(0);
    let total = paths.len();
    progress(Progress { fraction: 0.0, detail: format!("1 of {total} file(s)") });
    let progress = Mutex::new(progress);
    let results: Vec<anyhow::Result<(Option<PathBuf>, T)>> = crate::pool::get().install(|| {
        paths
            .par_iter()
            .map(|p| {
                if cancel.load(Ordering::SeqCst) {
                    anyhow::bail!("cancelled");
                }
                let r = each(p);
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                if let Ok(mut report) = progress.try_lock() {
                    report(Progress { fraction: n as f32 / total as f32, detail: format!("{n} of {total} file(s)") });
                }
                r
            })
            .collect()
    });
    if cancel.load(Ordering::SeqCst) {
        return PerFile(Outcome::cancelled(), Vec::new());
    }
    let mut outcome = Outcome::default();
    let mut per = Vec::new();
    for (p, r) in paths.iter().zip(results) {
        match r {
            Ok((saved, data)) => {
                outcome.outputs.extend(saved);
                per.push(data);
            }
            Err(e) => outcome.errors.push(format!("{}: {e:#}", name(p))),
        }
    }
    PerFile(outcome, per)
}
