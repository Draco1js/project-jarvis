//! Prints the max hey_jarvis score per clip and the steady-state cost per 80 ms frame.
//!
//! Usage: oww-runtime-spike <models-dir> <clip.wav>...

use std::path::PathBuf;
use std::time::{Duration, Instant};

use oww_runtime_spike::{Backend, CHUNK, WakeStream, read_clip};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let models = PathBuf::from(args.next().expect("models dir"));
    let mut total = Duration::ZERO;
    let mut frames = 0u32;
    for clip in args {
        let audio = read_clip(clip.as_ref())?;
        let mut stream = WakeStream::new(Backend::Tract, &models)?;
        let mut max = 0f32;
        for (i, c) in audio.chunks_exact(CHUNK).enumerate() {
            let t = Instant::now();
            max = max.max(stream.push(c)?);
            if i >= 2 {
                // skip the first frames, which compile new plans
                total += t.elapsed();
                frames += 1;
            }
        }
        println!("{clip}: max score {max:.4}");
    }
    let per = total / frames.max(1);
    println!(
        "{frames} frames, {per:?} per 80 ms frame ({:.2}% of one core)",
        per.as_secs_f64() / 0.08 * 100.0
    );
    Ok(())
}
