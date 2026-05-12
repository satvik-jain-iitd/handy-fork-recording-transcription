use tokio::sync::mpsc::UnboundedSender;
use async_trait::async_trait;
use crate::transcription::engine::{TranscriptionEngine, TranscriptionOptions, TranscriptionResult};
use crate::transcription::session::{ModelFamily, TranscriptionProgress};

pub struct ParakeetAdapter;

#[async_trait]
impl TranscriptionEngine for ParakeetAdapter {
    async fn load(&mut self, _model_path: &str) -> Result<(), String> {
        Err("Parakeet transcription support is coming in a future update. \
             To use transcription now, install a Whisper model.".to_string())
    }

    async fn transcribe(
        &self,
        _audio_file_path: &str,
        _options: &TranscriptionOptions,
        _progress_tx: UnboundedSender<TranscriptionProgress>,
    ) -> Result<TranscriptionResult, String> {
        Err("Parakeet transcription is not yet available.".to_string())
    }

    fn family(&self) -> ModelFamily { ModelFamily::Parakeet }

    fn is_supported(&self) -> bool { false }
}
