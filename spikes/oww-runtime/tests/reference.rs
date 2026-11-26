//! Per-frame wake scores must match openWakeWord's Python reference
//! (`scripts/reference.py`) on the same clips.
//!
//! Needs `OWW_DATA` pointing at a folder with `models/`, `clips/` and
//! `reference.csv` (see README.md for how to make it).

use std::collections::BTreeMap;
use std::path::PathBuf;

use oww_runtime_spike::{Backend, WakeStream, read_clip};

const TOLERANCE: f32 = 1e-3;

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("OWW_DATA").expect("set OWW_DATA (see README.md)"))
}

fn reference() -> BTreeMap<String, Vec<f32>> {
    let text = std::fs::read_to_string(data_dir().join("reference.csv")).unwrap();
    let mut clips: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    for line in text.lines().skip(1) {
        let mut cols = line.split(',');
        let clip = cols.next().unwrap().to_string();
        let frame: usize = cols.next().unwrap().parse().unwrap();
        let score: f32 = cols.next().unwrap().parse().unwrap();
        let scores = clips.entry(clip).or_default();
        assert_eq!(scores.len(), frame);
        scores.push(score);
    }
    clips
}

fn check(backend: Backend) {
    let models = data_dir().join("models");
    let mut worst = 0f32;
    for (clip, expected) in reference() {
        let audio = read_clip(&data_dir().join("clips").join(format!("{clip}.wav"))).unwrap();
        let mut stream = WakeStream::new(backend, &models).unwrap();
        let got: Vec<f32> = audio.chunks_exact(1280).map(|c| stream.push(c).unwrap()).collect();
        assert_eq!(got.len(), expected.len(), "{clip}: frame count");
        for (f, (g, e)) in got.iter().zip(&expected).enumerate() {
            let diff = (g - e).abs();
            worst = worst.max(diff);
            assert!(diff <= TOLERANCE, "{clip} frame {f}: got {g}, reference {e}");
        }
    }
    eprintln!("{backend:?}: worst per-frame difference {worst:e}");
}

#[test]
fn tract_scores_match_python_reference() {
    check(Backend::Tract);
}
