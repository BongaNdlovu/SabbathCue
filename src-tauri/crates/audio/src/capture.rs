use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossbeam_channel::Sender;

use crate::device::device_name;
use crate::error::AudioError;
use crate::types::{read_gain, AudioConfig, AudioFrame, GainHandle};

/// Holds a live audio capture stream.
/// Dropping this struct (or calling `stop`) will end the capture.
pub struct AudioCapture {
    _stream: Stream,
    dropped_frames: Arc<AtomicU64>,
}

impl std::fmt::Debug for AudioCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCapture").finish_non_exhaustive()
    }
}

impl AudioCapture {
    /// Stop the audio capture, consuming the struct.
    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        let dropped = self.dropped_frames.load(Ordering::Relaxed);
        if dropped > 0 {
            log::warn!("[AUDIO] Dropped {dropped} capture frame(s) because the consumer stalled");
        }
    }
}

/// Start capturing audio from the given device (or default) and send frames
/// through the provided crossbeam sender.
///
/// Audio is converted to mono 16-bit PCM at 16 kHz, with the shared gain
/// handle read live by the audio callback.
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
    gain_handle: GainHandle,
) -> Result<AudioCapture, AudioError> {
    let host = cpal::default_host();

    // Select the device
    log::info!("[AUDIO] Requested device_id: {:?}", config.device_id);

    let device = match &config.device_id {
        Some(id) if !id.is_empty() => {
            let mut found = None;
            let input_devices = host.input_devices().map_err(|e| {
                AudioError::StreamError(format!("Failed to enumerate devices: {e}"))
            })?;
            for d in input_devices {
                if let Ok(name) = device_name(&d) {
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
            let default_device_name = device_name(&d).unwrap_or_default();
            log::info!("[AUDIO] Using default device: '{default_device_name}'");
            d
        }
    };

    let supported_config = device
        .default_input_config()
        .map_err(|e| AudioError::StreamError(format!("Failed to get default input config: {e}")))?;

    let source_sample_rate = supported_config.sample_rate();
    let source_channels = supported_config.channels() as usize;
    let sample_format = supported_config.sample_format();

    let target_sample_rate: u32 = 16_000;
    let stream_config: StreamConfig = supported_config.into();
    let dropped_frames = Arc::new(AtomicU64::new(0));

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
                gain_handle.clone(),
                dropped_frames.clone(),
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
                gain_handle.clone(),
                dropped_frames.clone(),
            );
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Convert f32 -> i16 into the reused scratch buffer: the
                    // real-time callback must not allocate.
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "clamped f32 to i16 range is intentional for audio conversion"
                    )]
                    {
                        processor.scratch_f32.clear();
                        processor.scratch_f32.extend_from_slice(data);
                        processor.scratch_i16.clear();
                        processor
                            .scratch_i16
                            .reserve(processor.scratch_f32.len());
                        for &s in &processor.scratch_f32 {
                            let clamped = s.clamp(-1.0, 1.0);
                            processor
                                .scratch_i16
                                .push((clamped * f32::from(i16::MAX)) as i16);
                        }
                        let i16_data = std::mem::take(&mut processor.scratch_i16);
                        processor.process_i16_and_send(&i16_data, &sender);
                        processor.scratch_i16 = i16_data;
                    }
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
                gain_handle.clone(),
                dropped_frames.clone(),
            );
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    // Convert u16 -> i16 (u16 midpoint is 32768) into the
                    // reused scratch buffer: the real-time callback must not
                    // allocate.
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "u16-to-i16 offset conversion is intentional for audio"
                    )]
                    {
                        processor.scratch_i16.clear();
                        processor.scratch_i16.reserve(data.len());
                        for &s in data {
                            processor.scratch_i16.push((i32::from(s) - 32768) as i16);
                        }
                        let i16_data = std::mem::take(&mut processor.scratch_i16);
                        processor.process_i16_and_send(&i16_data, &sender);
                        processor.scratch_i16 = i16_data;
                    }
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

    Ok(AudioCapture {
        _stream: stream,
        dropped_frames,
    })
}

struct AudioProcessor {
    source_channels: usize,
    source_rate: u32,
    target_rate: u32,
    gain: GainHandle,
    resampler: LinearResampler,
    pending_samples: Vec<i16>,
    /// Reused across callbacks so the real-time audio thread never allocates.
    /// cpal callbacks run on the OS audio thread; a heap allocation there can
    /// exceed the callback deadline and produce an audible click or dropout.
    scratch_i16: Vec<i16>,
    scratch_f32: Vec<f32>,
    dropped_frames: Arc<AtomicU64>,
}

impl AudioProcessor {
    fn new(
        source_channels: usize,
        source_rate: u32,
        target_rate: u32,
        gain: GainHandle,
        dropped_frames: Arc<AtomicU64>,
    ) -> Self {
        Self {
            source_channels,
            source_rate,
            target_rate,
            gain,
            resampler: LinearResampler::new(source_rate, target_rate),
            pending_samples: Vec::new(),
            scratch_i16: Vec::new(),
            scratch_f32: Vec::new(),
            dropped_frames,
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

        let combined_samples;
        let input_samples = if self.pending_samples.is_empty() {
            samples
        } else {
            // Reuse the pending buffer itself as scratch: chain the new
            // samples behind what is already buffered instead of building a
            // fresh Vec every callback.
            self.pending_samples.extend_from_slice(samples);
            combined_samples = std::mem::take(&mut self.pending_samples);
            combined_samples.as_slice()
        };
        let frames = input_samples.chunks_exact(self.source_channels);
        let remainder = frames.remainder();
        let gain = read_gain(&self.gain);

        // Reuse the pending buffer as the output scratch so a steady stream
        // of callbacks performs no heap allocation on the audio thread.
        let gained = &mut self.pending_samples;
        gained.clear();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "audio sample conversions are intentionally truncating"
        )]
        {
            gained.extend(frames.map(|frame| {
                let sum: i32 = frame.iter().map(|&s| i32::from(s)).sum();
                let mono = sum / self.source_channels as i32;
                ((mono as f32) * gain).clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
            }));
        }

        let processed = if self.source_rate == self.target_rate {
            std::mem::take(gained)
        } else {
            self.resampler.resample(gained)
        };

        // Stash whatever trailing samples belong to the next callback
        // (`remainder` is a suffix of `input_samples`; when no complete frame
        // existed this callback it is the whole input).
        self.pending_samples.clear();
        self.pending_samples.extend_from_slice(remainder);
        self.pending_samples.shrink_to(16_384);

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

        // try_send never blocks, but it can fail when the consumer stalls or
        // is gone. Record telemetry with a lock-free increment; logging is
        // deferred to AudioCapture::drop on the non-realtime owner thread.
        if sender.try_send(frame).is_err() {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
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
    use crate::types::{new_gain_handle, set_gain};
    use std::sync::atomic::AtomicU64;

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

    #[test]
    fn downmix_carries_incomplete_frame_between_callbacks() {
        let (sender, receiver) = crossbeam_channel::bounded(4);
        let mut processor = AudioProcessor::new(
            2,
            16_000,
            16_000,
            new_gain_handle(1.0),
            Arc::new(AtomicU64::new(0)),
        );

        processor.process_i16_and_send(&[10], &sender);
        assert!(receiver.try_recv().is_err());

        processor.process_i16_and_send(&[30, 20, 40], &sender);
        let frame = receiver.recv().expect("frame should be sent");

        assert_eq!(frame.samples, vec![20, 30]);
    }

    #[test]
    fn processor_reads_live_gain_updates() {
        let (sender, receiver) = crossbeam_channel::bounded(4);
        let gain = new_gain_handle(1.0);
        let mut processor = AudioProcessor::new(
            1,
            16_000,
            16_000,
            gain.clone(),
            Arc::new(AtomicU64::new(0)),
        );

        processor.process_i16_and_send(&[100], &sender);
        assert_eq!(receiver.recv().expect("first frame").samples, vec![100]);

        set_gain(&gain, 2.0);
        processor.process_i16_and_send(&[100], &sender);
        assert_eq!(receiver.recv().expect("second frame").samples, vec![200]);
    }

    #[test]
    fn full_output_queue_records_drop_for_non_realtime_reporting() {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let mut processor = AudioProcessor::new(
            1,
            16_000,
            16_000,
            new_gain_handle(1.0),
            dropped.clone(),
        );

        processor.process_i16_and_send(&[100], &sender);
        processor.process_i16_and_send(&[200], &sender);

        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(receiver.recv().expect("first frame").samples, vec![100]);
    }
}
