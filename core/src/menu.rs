//! What the ring offers for a given set of dragged files. Pure logic, no Windows calls.
//!
//! Page 0 is the Convert ring (target formats), page 1 the Tools ring. Order is clockwise
//! from the top, matching the reference app (for a JPG: PNG at the top, DOCX top-left).
//! Like the reference, images open on Tools and PDFs on Convert; tapping the key again
//! (or clicking the hub) switches.

use std::path::Path;

use serde::Serialize;

use crate::formats::{Caps, Format};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Segment {
    /// "convert:png" or "tool:crop"; this is what a drop reports.
    pub id: String,
    pub label: String,
    /// Icon key the ring UI maps to an icon; None for format labels.
    pub icon: Option<&'static str>,
    pub enabled: bool,
    /// One short line shown in the hub while the petal is hovered.
    pub hint: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Menu {
    pub pages: [Vec<Segment>; 2],
    pub start_page: usize,
    /// Shown in the hub: the file's size ("1.2 MB"), or "3 files".
    pub title: String,
    /// Audio or video files: the hub shows a record instead of a thumbnail.
    pub media: bool,
}

pub const CONVERT_PAGE: usize = 0;
pub const TOOLS_PAGE: usize = 1;

const TARGETS: [Format; 9] = [
    Format::Jpg,
    Format::Png,
    Format::Webp,
    Format::Heic,
    Format::Tiff,
    Format::Avif,
    Format::Bmp,
    Format::Pdf,
    Format::Docx,
];

/// (id, label, icon, hint)
const TOOLS: [(&str, &str, &str, &str); 8] = [
    ("zip", "Zip", "archive", "Pack them into one ZIP"),
    ("compress", "Compress", "compress", "Smaller file, same look"),
    ("metadata", "Metadata", "tag", "Strip camera and GPS info"),
    ("edit", "Edit", "sliders", "Adjust light and colour"),
    ("annotate", "Annotate", "pen", "Draw and write on it"),
    ("add_bg", "Add BG", "image", "Put a colour behind it"),
    ("crop", "Crop", "crop", "Cut it to size"),
    ("redact", "Redact", "eye-off", "Black out private details"),
];

fn convert_page(formats: &[Option<Format>], caps: &Caps) -> Vec<Segment> {
    let Some(known) = formats.iter().copied().collect::<Option<Vec<Format>>>() else { return Vec::new() };
    if known.is_empty() || known.iter().any(|f| !f.is_image() && *f != Format::Pdf) {
        return Vec::new();
    }
    let readable = known.iter().all(|f| caps.can_read(*f));
    // Hide the source's own format only when every file shares it.
    let skip = known.iter().all(|f| *f == known[0]).then_some(known[0]);
    TARGETS
        .iter()
        .filter(|f| Some(**f) != skip)
        .map(|f| Segment {
            id: format!("convert:{}", f.id()),
            label: f.label(),
            icon: None,
            enabled: readable && caps.can_write(*f),
            hint: format!("Save a copy as {}", f.label()),
        })
        .collect()
}

fn tool_enabled(tool: &str, formats: &[Option<Format>], caps: &Caps) -> bool {
    if tool == "zip" {
        return !formats.is_empty();
    }
    let Some(known) = formats.iter().copied().collect::<Option<Vec<Format>>>() else { return false };
    if known.is_empty() || !known.iter().all(|f| caps.can_read(*f)) {
        return false;
    }
    let all = |ok: &dyn Fn(Format) -> bool| known.iter().all(|f| ok(*f));
    match tool {
        // Batch-safe quick actions.
        "compress" => all(&|f| matches!(f, Format::Jpg | Format::Png | Format::Webp | Format::Tiff | Format::Bmp | Format::Pdf)),
        "metadata" => all(&|f| matches!(f, Format::Jpg | Format::Png | Format::Webp | Format::Tiff | Format::Pdf)),
        // Editors open one file at a time.
        _ if known.len() > 1 => false,
        "annotate" | "crop" | "redact" => known[0].is_image() || known[0] == Format::Pdf,
        _ => known[0].is_image(),
    }
}

/// Audio targets for audio and video files, minus the format every file already is.
fn audio_page(paths: &[String], caps: &Caps) -> Vec<Segment> {
    let owns: Vec<Option<&str>> = paths.iter().map(|p| crate::audio::own_target(Path::new(p))).collect();
    let skip = owns.iter().all(|o| *o == owns[0]).then_some(owns[0]).flatten();
    crate::audio::TARGETS
        .iter()
        .filter(|(id, _)| Some(*id) != skip)
        .map(|(id, _)| Segment {
            id: format!("convert:{id}"),
            label: id.to_ascii_uppercase(),
            icon: None,
            enabled: caps.audio,
            hint: format!("{} copy, BPM in name", id.to_ascii_uppercase()),
        })
        .collect()
}

fn tools_page(formats: &[Option<Format>], caps: &Caps) -> Vec<Segment> {
    TOOLS
        .iter()
        .map(|(id, label, icon, hint)| Segment {
            id: format!("tool:{id}"),
            label: label.to_string(),
            icon: Some(icon),
            enabled: tool_enabled(id, formats, caps),
            hint: hint.to_string(),
        })
        .collect()
}

pub fn build(paths: &[String], caps: &Caps) -> Menu {
    let formats: Vec<Option<Format>> = paths.iter().map(|p| Format::from_path(Path::new(p))).collect();
    let media = !paths.is_empty() && paths.iter().all(|p| crate::audio::is_media(Path::new(p)));
    let convert = if media { audio_page(paths, caps) } else { convert_page(&formats, caps) };
    let tools = if media {
        let ps: Vec<&Path> = paths.iter().map(Path::new).collect();
        crate::audio::TOOLS
            .iter()
            .map(|(id, label, icon, hint)| Segment {
                id: format!("tool:{id}"),
                label: label.to_string(),
                icon: Some(icon),
                enabled: caps.audio && crate::audio::tool_enabled(id, &ps),
                hint: hint.to_string(),
            })
            .collect()
    } else {
        tools_page(&formats, caps)
    };
    // Every kind of file opens on its conversions; Shift or the hub flips to the tools.
    let start_page = if convert.iter().any(|s| s.enabled) {
        CONVERT_PAGE
    } else {
        TOOLS_PAGE
    };
    let title = match paths {
        [one] => match std::fs::metadata(one) {
            Ok(m) => human_size(m.len()),
            Err(_) => Path::new(one).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        },
        many => format!("{} files", many.len()),
    };
    Menu { pages: [convert, tools], start_page, title, media }
}

/// "840 KB", "1.2 MB", "12 MB": one decimal only below 10.
pub fn human_size(bytes: u64) -> String {
    let (value, unit) = match bytes {
        b if b >= 1 << 30 => (b as f64 / (1u64 << 30) as f64, "GB"),
        b if b >= 1 << 20 => (b as f64 / (1u64 << 20) as f64, "MB"),
        b if b >= 1 << 10 => (b as f64 / 1024.0, "KB"),
        b => return format!("{b} B"),
    };
    if value < 10.0 { format!("{value:.1} {unit}") } else { format!("{value:.0} {unit}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(segs: &[Segment]) -> Vec<&str> {
        segs.iter().map(|s| s.id.as_str()).collect()
    }

    fn enabled(segs: &[Segment]) -> Vec<&str> {
        segs.iter().filter(|s| s.enabled).map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn jpg_convert_ring_matches_reference_order() {
        let m = build(&["C:\\x\\Page 001.jpg".into()], &Caps::ALL);
        assert_eq!(
            ids(&m.pages[CONVERT_PAGE]),
            ["convert:png", "convert:webp", "convert:heic", "convert:tiff", "convert:avif", "convert:bmp", "convert:pdf", "convert:docx"]
        );
        assert_eq!(m.start_page, CONVERT_PAGE);
        assert_eq!(m.title, "Page 001.jpg");
    }

    #[test]
    fn tools_ring_order_and_jpg_enables_all() {
        let m = build(&["a.jpg".into()], &Caps::ALL);
        assert_eq!(
            ids(&m.pages[TOOLS_PAGE]),
            ["tool:zip", "tool:compress", "tool:metadata", "tool:edit", "tool:annotate", "tool:add_bg", "tool:crop", "tool:redact"]
        );
        assert_eq!(enabled(&m.pages[TOOLS_PAGE]).len(), 8);
    }

    #[test]
    fn pdf_skips_pdf_target_and_image_only_tools() {
        let m = build(&["doc.PDF".into()], &Caps::ALL);
        assert_eq!(m.start_page, CONVERT_PAGE);
        assert!(!ids(&m.pages[CONVERT_PAGE]).contains(&"convert:pdf"));
        assert!(ids(&m.pages[CONVERT_PAGE]).contains(&"convert:jpg"));
        assert_eq!(
            enabled(&m.pages[TOOLS_PAGE]),
            ["tool:zip", "tool:compress", "tool:metadata", "tool:annotate", "tool:crop", "tool:redact"]
        );
    }

    #[test]
    fn mixed_batch_shows_all_formats_and_only_batch_tools() {
        let m = build(&["a.jpg".into(), "b.png".into()], &Caps::ALL);
        assert_eq!(m.pages[CONVERT_PAGE].len(), 9);
        assert_eq!(enabled(&m.pages[TOOLS_PAGE]), ["tool:zip", "tool:compress", "tool:metadata"]);
        assert_eq!(m.title, "2 files");
    }

    #[test]
    fn unsupported_file_starts_on_tools_with_only_zip() {
        let m = build(&["notes.txt".into()], &Caps::ALL);
        assert!(m.pages[CONVERT_PAGE].is_empty());
        assert_eq!(m.start_page, TOOLS_PAGE);
        assert_eq!(enabled(&m.pages[TOOLS_PAGE]), ["tool:zip"]);
    }

    #[test]
    fn missing_heic_codecs_grey_out_heic() {
        let no_heic = Caps { heic_decode: false, heic_encode: false, avif_decode: true, audio: true };
        let m = build(&["a.png".into()], &no_heic);
        let heic = m.pages[CONVERT_PAGE].iter().find(|s| s.id == "convert:heic").unwrap();
        assert!(!heic.enabled);
        // A HEIC file itself can't be opened at all; it can still be zipped.
        let m = build(&["IMG_0001.HEIC".into()], &no_heic);
        assert!(enabled(&m.pages[CONVERT_PAGE]).is_empty());
        assert_eq!(enabled(&m.pages[TOOLS_PAGE]), ["tool:zip"]);
    }

    #[test]
    fn hub_shows_the_file_size() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(840 * 1024), "840 KB");
        assert_eq!(human_size(1_258_291), "1.2 MB");
        assert_eq!(human_size(12 * 1024 * 1024), "12 MB");
        let p = std::env::temp_dir().join("filebeaver-size-test.jpg");
        std::fs::write(&p, vec![0u8; 3 * 1024]).unwrap();
        assert_eq!(build(&[p.to_string_lossy().into_owned()], &Caps::ALL).title, "3.0 KB");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn audio_and_video_open_on_audio_targets() {
        let m = build(&["C:\\x\\Clip.mp4".into()], &Caps::ALL);
        assert!(m.media);
        assert_eq!(m.start_page, CONVERT_PAGE);
        assert_eq!(enabled(&m.pages[CONVERT_PAGE]), ["convert:mp3", "convert:wav", "convert:flac", "convert:m4a", "convert:ogg", "convert:opus", "convert:aac"]);
        // A video can be zipped or named by its tempo; editing tools need audio.
        assert_eq!(enabled(&m.pages[TOOLS_PAGE]), ["tool:zip", "tool:bpm"]);
        assert_eq!(enabled(&build(&["a.wav".into()], &Caps::ALL).pages[TOOLS_PAGE]).len(), 4);
        // An MP3 isn't offered MP3; a mixed audio batch gets every target.
        assert!(!ids(&build(&["a.mp3".into()], &Caps::ALL).pages[CONVERT_PAGE]).contains(&"convert:mp3"));
        assert_eq!(build(&["a.mp3".into(), "b.wav".into()], &Caps::ALL).pages[CONVERT_PAGE].len(), 7);
        // Without FFmpeg the targets are greyed out.
        let no_ff = Caps { audio: false, ..Caps::ALL };
        assert!(enabled(&build(&["a.wav".into()], &no_ff).pages[CONVERT_PAGE]).is_empty());
        // Audio mixed with a photo: nothing converts both.
        assert!(!build(&["a.mp3".into(), "b.jpg".into()], &Caps::ALL).media);
    }

    #[test]
    fn gif_and_avif_have_no_compress_or_metadata() {
        let m = build(&["a.gif".into()], &Caps::ALL);
        assert!(!enabled(&m.pages[TOOLS_PAGE]).contains(&"tool:compress"));
        assert!(!enabled(&m.pages[TOOLS_PAGE]).contains(&"tool:metadata"));
    }
}
