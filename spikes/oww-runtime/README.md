# Spike: tract vs ort for openWakeWord (issue #3)

Throwaway. Outside the main workspace. Result: **tract works**, see the wake-word section of `docs/DESIGN.md`.

`src/lib.rs` ports openWakeWord 0.6.0's streaming path (`AudioFeatures._streaming_features` + `Model.predict`, 80 ms chunks) and runs the three ONNX models (`melspectrogram`, `embedding_model`, `hey_jarvis_v0.1`) with `tract-onnx`. `tests/reference.rs` checks every per-frame score against openWakeWord's own Python code on the same clips.

## Reproduce

```sh
D=/tmp/oww; mkdir -p $D/models $D/clips $D/voices
python3 -m venv $D/venv
$D/venv/bin/pip install onnxruntime onnx tqdm scipy scikit-learn requests piper-tts
$D/venv/bin/pip install --no-deps openwakeword==0.6.0   # its tflite-runtime dep has no wheel for py3.12
$D/venv/bin/python -c "import openwakeword.utils as u; u.download_models(['hey_jarvis'])"
cp $D/venv/lib/python3.*/site-packages/openwakeword/resources/models/{melspectrogram,embedding_model,hey_jarvis_v0.1}.onnx $D/models/
# Piper voices from https://huggingface.co/rhasspy/piper-voices :
#   en_US-lessac-medium, en_US-ryan-high, en_GB-alba-medium, en_US-amy-medium (.onnx + .onnx.json) into $D/voices
$D/venv/bin/python -I scripts/make_clips.py $D/voices $D/clips
$D/venv/bin/python -I scripts/reference.py $D/models $D/clips/*.wav > $D/reference.csv
OWW_DATA=$D cargo test --release -- --nocapture
cargo run --release -- $D/models $D/clips/*.wav   # max score per clip + CPU per frame
```

Model SHA-256 (openWakeWord v0.5.1 release assets, as downloaded by 0.6.0):

```
ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f  melspectrogram.onnx
70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f  embedding_model.onnx
94a13cfe60075b132f6a472e7e462e8123ee70861bc3fb58434a73712ee0d2cb  hey_jarvis_v0.1.onnx
```
