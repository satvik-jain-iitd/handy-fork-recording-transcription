use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tauri::{AppHandle, Manager};
use async_trait::async_trait;
use crate::managers::transcription::{TranscriptionManager, LoadedEngine};
use crate::managers::model::ModelManager;
use crate::transcription::engine::{TranscriptionEngine, TranscriptionOptions, TranscriptionResult, TranscriptionSegment};
use crate::transcription::session::{ModelFamily, TranscriptionProgress};
use transcribe_rs::whisper_cpp::WhisperInferenceParams;
use crate::settings::get_settings;
use log::debug;
use anyhow::Result;

pub struct WhisperAdapter {
    app_handle: AppHandle,
    model_id: Option<String>,
}

impl WhisperAdapter {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            model_id: None,
        }
    }

    fn get_model_id_from_path(&self, model_path: &str) -> Option<String> {
        let model_manager = self.app_handle.state::<Arc<ModelManager>>();
        let models = model_manager.get_available_models();
        for info in models {
            if let Ok(path) = model_manager.get_model_path(&info.id) {
                if path.to_string_lossy() == model_path {
                    return Some(info.id);
                }
            }
        }
        None
    }
}

#[async_trait]
impl TranscriptionEngine for WhisperAdapter {
    async fn load(&mut self, model_path: &str) -> Result<(), String> {
        let tm = self.app_handle.state::<Arc<TranscriptionManager>>();
        
        let model_id = self.get_model_id_from_path(model_path)
            .ok_or_else(|| format!("Model not found in registry for path: {}", model_path))?;
        
        self.model_id = Some(model_id.clone());

        // Check if already loaded
        if tm.get_current_model() == Some(model_id.clone()) && tm.is_model_loaded() {
            return Ok(());
        }

        tm.load_model(&model_id).map_err(|e| e.to_string())
    }

    async fn transcribe(
        &self,
        audio_file_path: &str,
        options: &TranscriptionOptions,
        _progress_tx: UnboundedSender<TranscriptionProgress>,
    ) -> Result<TranscriptionResult, String> {
        let tm = self.app_handle.state::<Arc<TranscriptionManager>>().clone();
        
        // Decode audio
        let samples = crate::audio_toolkit::read_wav_samples(audio_file_path)
            .map_err(|e| format!("Failed to read audio file: {}", e))?;

        if samples.is_empty() {
            return Ok(TranscriptionResult {
                full_text: String::new(),
                segments: Vec::new(),
                processing_time_secs: 0.0,
            });
        }

        let start_time = std::time::Instant::now();
        
        // Wait for engine to be available
        let mut engine = loop {
            let mut is_loading = tm.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = tm.loading_condvar.wait(is_loading).unwrap();
            }

            let mut engine_guard = tm.lock_engine();
            if let Some(e) = engine_guard.take() {
                break e;
            }
            
            // Wait for engine to be put back
            debug!("Engine busy, waiting...");
            let _guard = tm.engine_condvar.wait(engine_guard).unwrap();
        };

        let settings = get_settings(&self.app_handle);
        
        // Transcribe
        let result = catch_unwind(AssertUnwindSafe(|| {
            match &mut engine {
                LoadedEngine::Whisper(whisper_engine) => {
                    let whisper_language = options.language.as_ref().map(|l| {
                        if l == "auto" { None } else { Some(l.clone()) }
                    }).flatten();

                    let params = WhisperInferenceParams {
                        language: whisper_language,
                        translate: settings.translate_to_english,
                        initial_prompt: if settings.custom_words.is_empty() {
                            None
                        } else {
                            Some(settings.custom_words.join(", "))
                        },
                        ..Default::default()
                    };

                    whisper_engine
                        .transcribe_with(&samples, &params)
                        .map_err(|e| format!("Whisper transcription failed: {}", e))
                }
                _ => Err("Engine is not a Whisper engine".to_string()),
            }
        }));

        // Put engine back
        let mut engine_guard = tm.lock_engine();
        *engine_guard = Some(engine);
        tm.engine_condvar.notify_all();

        match result {
            Ok(Ok(transcription_result)) => {
                let segments = transcription_result.segments.unwrap_or_default().into_iter().map(|s| TranscriptionSegment {
                    start_sec: s.start as f64,
                    end_sec: s.end as f64,
                    text: s.text,
                }).collect();

                Ok(TranscriptionResult {
                    full_text: transcription_result.text,
                    segments,
                    processing_time_secs: start_time.elapsed().as_secs_f64(),
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("Whisper engine panicked during transcription".to_string()),
        }
    }

    fn family(&self) -> ModelFamily { ModelFamily::Whisper }

    fn is_supported(&self) -> bool { true }
}
