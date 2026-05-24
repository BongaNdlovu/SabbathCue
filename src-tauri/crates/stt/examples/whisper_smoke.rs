use std::env;
use std::fs;
use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn read_pcm16_wav(path: &Path) -> Result<(u32, Vec<f32>), String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read WAV: {e}"))?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("expected a RIFF/WAVE file".to_string());
    }

    let mut cursor = 12;
    let mut sample_rate = None;
    let mut channels = None;
    let mut bits_per_sample = None;
    let mut data = None;

    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        cursor += 8;
        if cursor + size > bytes.len() {
            return Err("malformed WAV chunk".to_string());
        }

        match id {
            b"fmt " => {
                if size < 16 {
                    return Err("malformed fmt chunk".to_string());
                }
                let audio_format =
                    u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
                channels = Some(u16::from_le_bytes(
                    bytes[cursor + 2..cursor + 4].try_into().unwrap(),
                ));
                sample_rate = Some(u32::from_le_bytes(
                    bytes[cursor + 4..cursor + 8].try_into().unwrap(),
                ));
                bits_per_sample = Some(u16::from_le_bytes(
                    bytes[cursor + 14..cursor + 16].try_into().unwrap(),
                ));
                if audio_format != 1 {
                    return Err("expected PCM WAV".to_string());
                }
            }
            b"data" => data = Some(bytes[cursor..cursor + size].to_vec()),
            _ => {}
        }

        cursor += size + (size % 2);
    }

    let sample_rate = sample_rate.ok_or("missing sample rate")?;
    let channels = channels.ok_or("missing channel count")?;
    let bits_per_sample = bits_per_sample.ok_or("missing bits per sample")?;
    let data = data.ok_or("missing data chunk")?;

    if channels != 1 {
        return Err(format!("expected mono WAV, got {channels} channels"));
    }
    if bits_per_sample != 16 {
        return Err(format!("expected 16-bit WAV, got {bits_per_sample} bits"));
    }
    if sample_rate != 16_000 {
        return Err(format!("expected 16 kHz WAV, got {sample_rate} Hz"));
    }

    let samples = data
        .chunks_exact(2)
        .map(|chunk| f32::from(i16::from_le_bytes([chunk[0], chunk[1]])) / 32768.0)
        .collect();

    Ok((sample_rate, samples))
}

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let model_path = args
        .next()
        .ok_or("usage: whisper_smoke <model-path> <wav-path>")?;
    let wav_path = args
        .next()
        .ok_or("usage: whisper_smoke <model-path> <wav-path>")?;

    let (_sample_rate, samples) = read_pcm16_wav(Path::new(&wav_path))?;
    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .map_err(|e| format!("failed to load model: {e}"))?;
    let mut state = ctx
        .create_state()
        .map_err(|e| format!("failed to create Whisper state: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_n_threads(4);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_single_segment(true);
    params.set_token_timestamps(false);
    params.set_audio_ctx(384);
    params.set_max_tokens(96);
    params.set_no_speech_thold(0.6);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);

    state
        .full(params, &samples)
        .map_err(|e| format!("Whisper inference failed: {e}"))?;

    let mut text = String::new();
    for i in 0..state.full_n_segments() {
        if let Some(segment) = state.get_segment(i) {
            text.push_str(&segment.to_str_lossy().unwrap_or_default());
        }
    }

    println!("{}", text.trim());
    Ok(())
}
