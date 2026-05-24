use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossbeam_channel::Sender;

use crate::error::AudioError;
use crate::types::{AudioConfig, AudioFrame};

/// Holds a live audio capture stream.
/// Dropping this struct (or calling `stop`) will end the capture.
pub struct AudioCapture {
    stream: Stream,
}

impl std::fmt::Debug for AudioCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCapture").finish_non_exhaustive()
    }
}

impl AudioCapture {
    /// Stop the audio capture, consuming the struct.
    pub fn stop(self) {
        drop(self.stream);
    }
}

/// Start capturing audio from the given device (or default) and send frames
/// through the provided crossbeam sender.
///
/// Audio is converted to mono 16-bit PCM at 16 kHz, with the specified gain
/// applied.
///
/// `device_lost` is an out-parameter the caller passes in: it is set to `true`
/// when cpal's stream-error callback fires (typically because the OS device
/// vanished). The caller's watchdog loop polls this to know when to drop the
/// `AudioCapture` and rebuild it once the device returns.
///
/// When `config.device_id` names a device that isn't currently enumerable,
/// this returns `AudioError::DeviceNotFound` rather than silently falling back
/// to the system default — the watchdog should retry instead of switching to
/// the laptop mic. With `device_id` unset (`None` or empty) the system default
/// is used as before.
#[expect(
    clippy::too_many_lines,
    reason = "audio setup is inherently sequential with many format branches"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "config fields are read and sender is cloned into closures"
)]
pub fn start(
    config: AudioConfig,
    sender: Sender<AudioFrame>,
    device_lost: Arc<AtomicBool>,
) -> Result<AudioCapture, AudioError> {
    let host = cpal::default_host();

    // Select the device
    log::info!("[AUDIO] Requested device_id: {:?}", &config.device_id);

    let device = match &config.device_id {
        Some(id) if !id.is_empty() => {
            let mut found = None;
            let input_devices = host.input_devices().map_err(|e| {
                AudioError::StreamError(format!("Failed to enumerate devices: {e}"))
            })?;
            for d in input_devices {
                if let Ok(name) = d.name() {
                    log::info!("[AUDIO]   Available device: '{name}'");
                    if name == *id {
                        log::info!("[AUDIO]   ✓ MATCH: '{name}'");
                        found = Some(d);
                        break;
                    }
                }
            }
            if let Some(d) = found {
                log::info!("[AUDIO] Using requested device: '{id}'");
                d
            } else {
                log::warn!("[AUDIO] Device '{id}' not currently available — caller should wait or change selection.");
                return Err(AudioError::DeviceNotFound(id.clone()));
            }
        }
        _ => {
            let d = host
                .default_input_device()
                .ok_or(AudioError::NoInputDevices)?;
            log::info!(
                "[AUDIO] Using default device: '{}'",
                &d.name().unwrap_or_default()
            );
            d
        }
    };

    let supported_config = device
        .default_input_config()
        .map_err(|e| AudioError::StreamError(format!("Failed to get default input config: {e}")))?;

    let source_sample_rate = supported_config.sample_rate().0;
    let source_channels = supported_config.channels() as usize;
    let sample_format = supported_config.sample_format();

    let target_sample_rate: u32 = 16_000;
    let gain = config.gain;

    let stream_config: StreamConfig = supported_config.into();

    // Build a fresh err callback per match arm. cpal takes the callback by
    // value, and our closure captures `Arc<AtomicBool>` so each arm needs
    // its own clone.
    let make_err_fn = || {
        let device_lost = device_lost.clone();
        move |err: cpal::StreamError| {
            log::error!("Audio stream error: {err}");
            device_lost.store(true, Ordering::SeqCst);
        }
    };

    let stream = match sample_format {
        SampleFormat::I16 => {
            let sender = sender.clone();
            let mut processor = AudioProcessor::new(
                source_channels,
                source_sample_rate,
                target_sample_rate,
                gain,
            );
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    processor.process_i16_and_send(data, &sender);
                },
                make_err_fn(),
                None,
            )
        }
        SampleFormat::F32 => {
            let sender = sender.clone();
            let mut processor = AudioProcessor::new(
                source_channels,
                source_sample_rate,
                target_sample_rate,
                gain,
            );
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Convert f32 -> i16
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "clamped f32 to i16 range is intentional for audio conversion"
                    )]
                    let i16_data: Vec<i16> = data
                        .iter()
                        .map(|&s| {
                            let clamped = s.clamp(-1.0, 1.0);
                            (clamped * f32::from(i16::MAX)) as i16
                        })
                        .collect();
                    processor.process_i16_and_send(&i16_data, &sender);
                },
                make_err_fn(),
                None,
            )
        }
        SampleFormat::U16 => {
            let sender = sender.clone();
            let mut processor = AudioProcessor::new(
                source_channels,
                source_sample_rate,
                target_sample_rate,
                gain,
            );
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    // Convert u16 -> i16 (u16 midpoint is 32768)
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "u16-to-i16 offset conversion is intentional for audio"
                    )]
                    let i16_data: Vec<i16> = data
                        .iter()
                        .map(|&s| (i32::from(s) - 32768) as i16)
                        .collect();
                    processor.process_i16_and_send(&i16_data, &sender);
                },
                make_err_fn(),
                None,
            )
        }
        _ => {
            return Err(AudioError::StreamError(format!(
                "Unsupported sample format: {sample_format:?}"
            )));
        }
    }
    .map_err(|e| AudioError::StreamError(format!("Failed to build input stream: {e}")))?;

    stream
        .play()
        .map_err(|e| AudioError::StreamError(format!("Failed to start stream: {e}")))?;

    Ok(AudioCapture { stream })
}

struct AudioProcessor {
    source_channels: usize,
    source_rate: u32,
    target_rate: u32,
    gain: f32,
    resampler: LinearResampler,
}

impl AudioProcessor {
    fn new(source_channels: usize, source_rate: u32, target_rate: u32, gain: f32) -> Self {
        Self {
            source_channels,
            source_rate,
            target_rate,
            gain,
            resampler: LinearResampler::new(source_rate, target_rate),
        }
    }

    /// Downmix to mono, apply gain, resample to target rate, and send as `AudioFrame`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "audio sample conversions are intentionally truncating"
    )]
    #[expect(
        clippy::cast_precision_loss,
        reason = "i16 audio samples fit exactly enough for gain scaling"
    )]
    #[expect(clippy::cast_possible_wrap, reason = "channel count fits in i32")]
    fn process_i16_and_send(&mut self, samples: &[i16], sender: &Sender<AudioFrame>) {
        if samples.is_empty() || self.source_channels == 0 {
            return;
        }

        let gained: Vec<i16> = samples
            .chunks_exact(self.source_channels)
            .map(|frame| {
                let sum: i32 = frame.iter().map(|&s| i32::from(s)).sum();
                let mono = sum / self.source_channels as i32;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "clamped audio sample intentionally narrows to i16"
                )]
                {
                    ((mono as f32) * self.gain).clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                        as i16
                }
            })
            .collect();

        let processed = if self.source_rate == self.target_rate {
            gained
        } else {
            self.resampler.resample(&gained)
        };

        if processed.is_empty() {
            return;
        }

        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let frame = AudioFrame {
            samples: processed,
            timestamp_ms,
        };

        let _ = sender.try_send(frame);
    }
}

/// Stateful linear-interpolation resampler.
///
/// The previous implementation restarted interpolation at every cpal callback.
/// Keeping position across callbacks avoids subtle timing jitter at 44.1 kHz and
/// other non-16 kHz source rates.
struct LinearResampler {
    ratio: f64,
    next_input_index: f64,
    samples_seen: u64,
    last_sample: Option<i16>,
}

impl LinearResampler {
    fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            ratio: f64::from(from_rate) / f64::from(to_rate),
            next_input_index: 0.0,
            samples_seen: 0,
            last_sample: None,
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "resampling math intentionally truncates to i16/usize"
    )]
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample indices and rates fit comfortably in f64"
    )]
    #[expect(
        clippy::cast_sign_loss,
        reason = "global sample positions are non-negative"
    )]
    fn resample(&mut self, input: &[i16]) -> Vec<i16> {
        if input.is_empty() {
            return Vec::new();
        }

        let start = self.samples_seen as f64;
        let end = start + input.len() as f64;
        let estimate = ((input.len() as f64) / self.ratio).ceil() as usize;
        let mut output = Vec::with_capacity(estimate);

        while self.next_input_index + 1.0 < end {
            let idx = self.next_input_index.floor() as u64;
            let frac = self.next_input_index - idx as f64;

            let Some(a) = self.sample_at(input, start, idx) else {
                self.next_input_index += self.ratio;
                continue;
            };
            let Some(b) = self.sample_at(input, start, idx + 1) else {
                break;
            };

            output.push((f64::from(a) + (f64::from(b) - f64::from(a)) * frac) as i16);
            self.next_input_index += self.ratio;
        }

        self.samples_seen += input.len() as u64;
        self.last_sample = input.last().copied();
        output
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "requested indices are bounded by the current audio chunk"
    )]
    #[expect(clippy::cast_sign_loss, reason = "requested indices are non-negative")]
    fn sample_at(&self, input: &[i16], start: f64, index: u64) -> Option<i16> {
        let start_index = start as u64;
        if index + 1 == start_index {
            return self.last_sample;
        }
        if index < start_index {
            return None;
        }
        input.get((index - start_index) as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_resampler_matches_single_pass_across_callback_boundaries() {
        let input = (0..5000)
            .map(|i| i16::try_from((i % 200) - 100).unwrap())
            .collect::<Vec<_>>();

        let mut single_pass = LinearResampler::new(44_100, 16_000);
        let expected = single_pass.resample(&input);

        let mut streaming = LinearResampler::new(44_100, 16_000);
        let mut actual = Vec::new();
        for chunk in input.chunks(137) {
            actual.extend(streaming.resample(chunk));
        }

        assert_eq!(actual, expected);
    }
}
