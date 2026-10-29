//! Prints what con/vert reads from audio files: tags, tempo (with confidence) and key.
//! usage: CONVERT_FFMPEG=ffmpeg.exe cargo run --release --example analyze_file -- <file>...

use shiftdrop_core::analyze;

fn main() {
    let ff = shiftdrop_core::audio::ffmpeg().expect("set CONVERT_FFMPEG");
    for arg in std::env::args().skip(1) {
        let p = std::path::Path::new(&arg);
        let (pcm, (tag_bpm, tag_key)) = analyze::decode(&ff, p).expect("decode");
        let tempo = analyze::tempo_scored(&pcm);
        let key = analyze::key_scored(&pcm);
        println!(
            "{}: {:.1} s, tags bpm {tag_bpm:?} key {:?} | tempo {tempo:?} | key {:?}",
            p.file_name().unwrap().to_string_lossy(),
            pcm.len() as f32 / analyze::RATE,
            tag_key.map(|k| k.name()),
            key.map(|(k, m)| (k.name(), m)),
        );
    }
}
