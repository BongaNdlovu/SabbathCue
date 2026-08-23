use cpal::traits::{DeviceTrait, HostTrait};

use crate::error::AudioError;
use crate::types::DeviceInfo;

/// Enumerate all available audio input devices.
/// The default input device is marked with `is_default: true`.
pub fn enumerate_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let host = cpal::default_host();

    let default_device_name = host
        .default_input_device()
        .and_then(|d| device_name(&d).ok());

    let input_devices = host
        .input_devices()
        .map_err(|e| AudioError::StreamError(format!("Failed to enumerate input devices: {e}")))?;

    let mut devices = Vec::new();
    let mut skipped = 0usize;

    for device in input_devices {
        let name = device_name(&device).unwrap_or_else(|_| "Unknown Device".to_string());

        // One faulty device (an unplugged USB mic that still enumerates is
        // common on Windows) must not hide every healthy device from the
        // settings UI — skip it and keep listing the rest.
        let default_config = match device.default_input_config() {
            Ok(config) => config,
            Err(error) => {
                skipped += 1;
                log::warn!("Skipping audio device '{name}' (config query failed): {error}");
                continue;
            }
        };

        let is_default = default_device_name.as_ref().is_some_and(|dn| dn == &name);

        devices.push(DeviceInfo {
            id: name.clone(),
            name: name.clone(),
            sample_rate: default_config.sample_rate(),
            channels: default_config.channels(),
            is_default,
        });
    }

    if skipped > 0 {
        log::warn!("Skipped {skipped} audio device(s) that failed their config query");
    }

    if devices.is_empty() {
        return Err(AudioError::NoInputDevices);
    }

    Ok(devices)
}

pub(crate) fn device_name(device: &cpal::Device) -> Result<String, cpal::DeviceNameError> {
    device
        .description()
        .map(|description| description.name().to_string())
}
