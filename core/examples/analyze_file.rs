//! Prints what con/vert reads from audio files: the BPM tag and the tempo (with confidence).
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example analyze_file -- <file>...

use shiftdrop_core::analyze;

fn main() {
    let ff = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    for arg in std::env::args().skip(1) {
        let p = std::path::Path::new(&arg);
        let (pcm, tag) = analyze::decode(&ff, p).expect("decode");
        println!(
            "{}: {:.1} s, tag bpm {tag:?} | tempo {:?}",
            p.file_name().unwrap().to_string_lossy(),
            pcm.len() as f32 / analyze::RATE,
            analyze::tempo_scored(&pcm),
        );
    }
}
