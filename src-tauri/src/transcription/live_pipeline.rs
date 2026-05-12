use crossbeam_channel::Receiver;
use tauri::{AppHandle, Emitter};
use log::info;

use crate::audio_toolkit::VoiceActivityDetector;
use super::engine::{TranscriptionEngine, TranscriptionOptions};

pub struct LiveTranscriptionPipeline {
    app: AppHandle,
    engine: Box<dyn TranscriptionEngine>,
    vad: Box<dyn VoiceActivityDetector>,
    audio_rx: Receiver<Vec<f32>>,
    session_id: String,
}

impl LiveTranscriptionPipeline {
    pub fn new(
        app: AppHandle,
        engine: Box<dyn TranscriptionEngine>,
        vad: Box<dyn VoiceActivityDetector>,
        audio_rx: Receiver<Vec<f32>>,
        session_id: String,
    ) -> Self {
        Self {
            app,
            engine,
            vad,
            audio_rx,
            session_id,
        }
    }

    pub async fn run(mut self) {
        let mut audio_buffer = Vec::new();
        let mut rolling_window = Vec::new();
        let window_size = 16000 * 10; // 10 seconds rolling window for Whisper
        
        info!("Live transcription pipeline started for session {}", self.session_id);

        while let Ok(samples) = self.audio_rx.recv() {
            audio_buffer.extend_from_slice(&samples);
            
            // Feed to VAD in 30ms chunks
            let is_speech = self.vad.is_voice(&samples).unwrap_or(false);
            
            if audio_buffer.len() >= 16000 { // Process every 1 second of audio
                rolling_window.extend_from_slice(&audio_buffer);
                audio_buffer.clear();

                // Keep window size
                if rolling_window.len() > window_size {
                    let truncate_len = rolling_window.len() - window_size;
                    rolling_window.drain(0..truncate_len);
                }

                let (progress_tx, _) = tokio::sync::mpsc::unbounded_channel();
                let temp_path = std::env::temp_dir().join(format!("live_{}.wav", self.session_id));
                if let Ok(_) = crate::audio_toolkit::audio::save_wav_file(&temp_path, &rolling_window) {
                    let options = TranscriptionOptions::default();
                    if let Ok(result) = self.engine.transcribe(temp_path.to_str().unwrap(), &options, progress_tx).await {
                        let _ = self.app.emit("live-transcript-update", serde_json::json!({
                            "session_id": self.session_id,
                            "text": result.full_text,
                            "is_final": !is_speech
                        }));
                    }
                }
            }
        }
        
        info!("Live transcription pipeline stopped");
    }
}
