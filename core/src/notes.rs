//! Which notes are played, from Spotify's Basic Pitch model (Apache-2.0, models/), run in
//! pure Rust by tract. Key detection reads its pitch classes: the model hears notes rather
//! than overtones, which the spectrum-only method kept confusing for other keys.
//! Measured on 714 labelled loops (two producer-kit sets, Phrygian loops, synthetic loops):
//! 285 keys right against 229 for the spectrum-only method, better in every set.

use std::sync::OnceLock;

use tract_onnx::prelude::*;

/// The model's input: 2 s at 22050 Hz, minus one hop.
const WINDOW: usize = 43844;
/// Half of the 30 frames each window shares with the next; those edge frames are dropped.
const EDGE_FRAMES: usize = 15;
const PAD: usize = EDGE_FRAMES * 256;
const HOP: usize = WINDOW - 2 * PAD;
/// Only the first 90 s: a key doesn't need more, and each window costs ~60 ms.
const MAX_SAMPLES: usize = 90 * 22050;
/// A note counts while its activation is above this.
const ACTIVE: f32 = 0.2;

type Model = std::sync::Arc<TypedRunnableModel>;

fn model() -> Option<&'static Model> {
    static MODEL: OnceLock<Option<Model>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let load = || -> TractResult<Model> {
                let bytes: &[u8] = include_bytes!("../models/basic-pitch-nmp.onnx");
                tract_onnx::onnx()
                    .model_for_read(&mut std::io::Cursor::new(bytes))?
                    .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, WINDOW, 1)))?
                    .into_optimized()?
                    .into_runnable()
            };
            load().ok()
        })
        .as_ref()
}

/// How strongly each pitch class (0 = C) sounds across `x` (mono, 22050 Hz), summed over
/// active notes. None if the model can't run or hears no notes. (Taking a bass note's 5th
/// harmonic off helped trap beats but cost songs and EDM more: measured on 1336 files.)
pub fn chroma(x: &[f32]) -> Option<[f32; 12]> {
    let mut out = [0.0f32; 12];
    for frame in activations(x)? {
        for (k, v) in frame.into_iter().enumerate() {
            if v > ACTIVE {
                out[(21 + k) % 12] += v;
            }
        }
    }
    (out.iter().sum::<f32>() > 1.0).then_some(out)
}

/// The model's note activations (0..1) per frame, for the 88 piano keys from A0.
pub fn activations(x: &[f32]) -> Option<Vec<[f32; 88]>> {
    let model = model()?;
    let x = &x[..x.len().min(MAX_SAMPLES)];
    let mut padded = vec![0.0f32; PAD];
    padded.extend_from_slice(x);
    let mut out = Vec::new();
    let mut start = 0;
    while start < x.len() + PAD {
        let mut win = vec![0.0f32; WINDOW];
        let end = (start + WINDOW).min(padded.len());
        win[..end - start].copy_from_slice(&padded[start..end]);
        let input = Tensor::from(tract_ndarray::Array3::from_shape_vec((1, WINDOW, 1), win).ok()?);
        let result = model.run(tvec!(input.into())).ok()?;
        // Outputs: onsets, notes, contour. Notes are frames x 88 keys from A0.
        let notes = result.get(1)?.to_plain_array_view::<f32>().ok()?;
        let frames = notes.shape().get(1).copied()?;
        for f in EDGE_FRAMES..frames.saturating_sub(EDGE_FRAMES) {
            out.push(std::array::from_fn(|k| notes[[0, f, k]]));
        }
        start += HOP;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hears_the_notes_of_a_chord() {
        // A minor triad (A3 C4 E4) with a few harmonics, 3 s.
        let x: Vec<f32> = (0..3 * 22050)
            .map(|i| {
                let t = i as f32 / 22050.0;
                [57.0f32, 60.0, 64.0]
                    .iter()
                    .map(|m| {
                        let f = 440.0 * 2f32.powf((m - 69.0) / 12.0);
                        (1..=4).map(|h| (2.0 * std::f32::consts::PI * f * h as f32 * t).sin() / h as f32).sum::<f32>()
                    })
                    .sum::<f32>()
                    * 0.2
            })
            .collect();
        let c = chroma(&x).expect("notes");
        let mut top: Vec<usize> = (0..12).collect();
        top.sort_by(|a, b| c[*b].total_cmp(&c[*a]));
        let mut three = top[..3].to_vec();
        three.sort();
        assert_eq!(three, [0, 4, 9], "{c:?}");
    }
}
