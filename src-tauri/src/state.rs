use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rhema_bible::{BibleDb, Translation};

pub struct AppState {
    pub bible_db: Option<BibleDb>,
    pub active_translation_id: i64,
    pub audio_active: Arc<AtomicBool>,
    pub stt_active: Arc<AtomicBool>,
    pub audio_session_generation: Arc<AtomicU64>,
    pub detection_paused: Arc<AtomicBool>,
    pub bible_detection_enabled: Arc<AtomicBool>,
    pub semantic_detection_enabled: Arc<AtomicBool>,
    pub auto_mode: Arc<AtomicBool>,
    pub live_output_enabled: Arc<AtomicBool>,
    stt_task_handles: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            bible_db: None,
            active_translation_id: 1, // Default to first translation (KJV)
            audio_active: Arc::new(AtomicBool::new(false)),
            stt_active: Arc::new(AtomicBool::new(false)),
            audio_session_generation: Arc::new(AtomicU64::new(0)),
            detection_paused: Arc::new(AtomicBool::new(false)),
            bible_detection_enabled: Arc::new(AtomicBool::new(true)),
            semantic_detection_enabled: Arc::new(AtomicBool::new(true)),
            auto_mode: Arc::new(AtomicBool::new(true)),
            live_output_enabled: Arc::new(AtomicBool::new(false)),
            stt_task_handles: Vec::new(),
        }
    }

    pub fn replace_stt_task_handles(
        &mut self,
        task_handles: Vec<tauri::async_runtime::JoinHandle<()>>,
    ) -> Vec<tauri::async_runtime::JoinHandle<()>> {
        let stale_handles = self.take_stt_task_handles();
        self.stt_task_handles = task_handles;
        stale_handles
    }

    pub fn take_stt_task_handles(&mut self) -> Vec<tauri::async_runtime::JoinHandle<()>> {
        let mut task_handles = Vec::with_capacity(self.stt_task_handles.len());
        while let Some(handle) = self.stt_task_handles.pop() {
            task_handles.push(handle);
        }
        task_handles
    }

    /// Retire the current native audio fanout session.
    pub fn invalidate_audio_session(&self) {
        self.audio_session_generation.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn initial_translation_id(translations: &[Translation]) -> Option<i64> {
    translations
        .iter()
        .filter(|translation| !translation.is_copyrighted && translation.is_downloaded)
        .find(|translation| translation.abbreviation.eq_ignore_ascii_case("KJV"))
        .or_else(|| {
            translations
                .iter()
                .find(|translation| !translation.is_copyrighted && translation.is_downloaded)
        })
        .map(|translation| translation.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translation(id: i64, abbreviation: &str) -> Translation {
        Translation {
            id,
            abbreviation: abbreviation.to_string(),
            title: abbreviation.to_string(),
            language: "English".to_string(),
            is_copyrighted: false,
            is_downloaded: true,
        }
    }

    #[test]
    fn initial_translation_id_prefers_kjv_when_available() {
        let translations = [translation(4, "NIV"), translation(7, "KJV")];

        assert_eq!(initial_translation_id(&translations), Some(7));
    }

    #[test]
    fn initial_translation_id_falls_back_to_first_translation() {
        let translations = [translation(4, "SpaRV"), translation(7, "FreJND")];

        assert_eq!(initial_translation_id(&translations), Some(4));
    }

    #[test]
    fn initial_translation_id_skips_locked_translations() {
        let mut locked = translation(4, "NIV");
        locked.is_copyrighted = true;
        let translations = [locked, translation(7, "SpaRV")];

        assert_eq!(initial_translation_id(&translations), Some(7));
    }
}
