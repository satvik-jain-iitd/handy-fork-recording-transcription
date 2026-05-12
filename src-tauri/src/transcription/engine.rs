use tokio::sync::mpsc::UnboundedSender;
use async_trait::async_trait;

use super::session::{ModelFamily, TranscriptionProgress};

/// Options passed to the transcription engine for each job.
#[derive(Debug, Clone)]
pub struct TranscriptionOptions {
    /// BCP-47 language code ("en", "hi", "auto"), or None for auto-detect.
    pub language: Option<String>,
    /// Duration of each audio chunk in seconds.
    pub chunk_size_secs: f64,
    /// Overlap between consecutive chunks in seconds.
    pub overlap_secs: f64,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            language: None,
            chunk_size_secs: 300.0, // 5 minutes
            overlap_secs: 2.0,
        }
    }
}

/// Output from the engine for a single transcription job.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// The full stitched transcript text.
    pub full_text: String,
    /// Per-segment results if the model provides timestamps.
    #[allow(dead_code)]
    pub segments: Vec<TranscriptionSegment>,
    /// Wall-clock time for the entire job in seconds.
    #[allow(dead_code)]
    pub processing_time_secs: f64,
}

#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    #[allow(dead_code)]
    pub start_sec: f64,
    #[allow(dead_code)]
    pub end_sec: f64,
    #[allow(dead_code)]
    pub text: String,
}

/// The unified interface all model backends must implement.
/// The orchestrator (start_transcription command) calls only this trait.
/// The UI never sees which adapter is running.
#[async_trait]
pub trait TranscriptionEngine: Send + Sync {
    /// Load the model from the given directory/file path.
    /// Must be idempotent: if the model is already loaded, return Ok(()) immediately.
    async fn load(&mut self, model_path: &str) -> Result<(), String>;

    /// Transcribe the audio file at the given path.
    /// Send TranscriptionProgress updates through progress_tx after each chunk.
    /// The orchestrator will relay these to the Tauri event bus.
    async fn transcribe(
        &self,
        audio_file_path: &str,
        options: &TranscriptionOptions,
        progress_tx: UnboundedSender<TranscriptionProgress>,
    ) -> Result<TranscriptionResult, String>;

    /// Return this engine's model family.
    #[allow(dead_code)]
    fn family(&self) -> ModelFamily;

    /// Return true if this engine is fully implemented and ready for use.
    #[allow(dead_code)]
    fn is_supported(&self) -> bool;
}
