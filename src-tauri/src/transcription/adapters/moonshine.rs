use tokio::sync::mpsc::UnboundedSender;
use async_trait::async_trait;
use crate::transcription::engine::{TranscriptionEngine, TranscriptionOptions, TranscriptionResult};
use crate::transcription::session::{ModelFamily, TranscriptionProgress};

pub struct MoonshineAdapter;

#[async_trait]
impl TranscriptionEngine for MoonshineAdapter {
    async fn load(&mut self, _model_path: &str) -> Result<(), String> {
        Err("Moonshine transcription support is coming in a future update. \
             To use transcription now, install a Whisper model.".to_string())
    }

    async fn transcribe(
        &self,
        _audio_file_path: &str,
        _options: &TranscriptionOptions,
        _progress_tx: UnboundedSender<TranscriptionProgress>,
    ) -> Result<TranscriptionResult, String> {
        Err("Moonshine transcription is not yet available.".to_string())
    }

    fn family(&self) -> ModelFamily { ModelFamily::Moonshine }

    fn is_supported(&self) -> bool { false }
}
