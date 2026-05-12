//! Transcription session data model and persistence.
//!
//! Architecture Map Findings:
//! 1. App Data Path: Resolved via `crate::portable::app_data_dir`. Portable mode uses `Data/` next to exe.
//! 2. Model Loading: Global/Shared via `TranscriptionManager`'s `engine` Mutex.
//! 3. History: Handy uses SQLite (`history.db`) for general transcription history.
//! 4. File Picker: Uses `tauri-plugin-dialog`.
//! 5. Event Bus: Uses Tauri's `Emitter` and `specta` for typed events.
//! 6. Frontend State: Managed by Zustand.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscriptionSession {
    /// UUIDv4 generated at session creation time.
    pub session_id: String,

    /// Absolute path to the source audio/video file on disk.
    pub source_file_path: String,

    /// File name only (no directory component), for display purposes.
    pub source_file_name: String,

    /// File size in bytes, recorded at session creation.
    pub source_file_size: u64,

    /// File extension or MIME type, used to determine decoding strategy.
    /// Use the lowercase file extension (e.g., "mp3", "wav", "mp4", "m4a").
    pub source_file_type: String,

    /// The model_id from ModelInfo, exactly as reported by the registry.
    pub selected_model_id: String,

    /// The model family, derived from registry discovery.
    pub model_family: ModelFamily,

    /// ISO 8601 UTC timestamp. Set when session is first created.
    pub created_at: String,

    /// ISO 8601 UTC timestamp. Set when status transitions to Completed or Failed.
    pub completed_at: Option<String>,

    /// Elapsed wall-clock time in seconds for the full transcription job.
    pub elapsed_seconds: Option<f64>,

    /// Current lifecycle state of this session.
    pub status: SessionStatus,

    /// Real-time progress snapshot, updated after each chunk.
    pub progress: TranscriptionProgress,

    /// Total number of chunks the audio was split into.
    pub chunk_count: u32,

    /// Per-chunk metadata including text and timing information.
    pub chunk_metadata: Vec<ChunkInfo>,

    /// The complete stitched transcript text. Populated on Completed.
    pub transcript_text: Option<String>,

    /// Absolute path to the saved .txt transcript file.
    pub transcript_file_path: Option<String>,

    /// Human-readable error description. Populated only on Failed.
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub enum ModelFamily {
    Whisper,
    Parakeet,
    Moonshine,
    /// For unrecognized model types. Contains a display label.
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub enum SessionStatus {
    Queued,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Type)]
pub struct TranscriptionProgress {
    /// 0.0 to 100.0
    pub percent: f64,
    pub current_chunk: u32,
    pub total_chunks: u32,
    /// Human-readable message, e.g. "Transcribing chunk 3 of 12…"
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ChunkInfo {
    pub index: u32,
    /// Start time of this chunk in the original audio, in seconds.
    pub start_sec: f64,
    /// End time of this chunk in the original audio, in seconds.
    pub end_sec: f64,
    /// Byte offset in the source file (used for progress/recovery).
    pub byte_offset: u64,
    /// Byte length of this chunk.
    pub byte_length: u64,
    /// Transcript text for this specific chunk, if available from the model.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SessionSummary {
    pub session_id: String,
    pub source_file_name: String,
    pub created_at: String,
    pub status: SessionStatus,
    pub model_family: ModelFamily,
    /// Stored for quick display without loading the full session file.
    pub elapsed_seconds: Option<f64>,
}

impl TranscriptionSession {
    pub fn get_sessions_dir(app: &AppHandle) -> Result<PathBuf> {
        let app_data_dir = crate::portable::app_data_dir(app)
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?;
        let sessions_dir = app_data_dir.join("sessions");
        if !sessions_dir.exists() {
            fs::create_dir_all(&sessions_dir).context("Failed to create sessions directory")?;
        }
        Ok(sessions_dir)
    }

    pub fn save(&self, app: &AppHandle) -> Result<()> {
        let sessions_dir = Self::get_sessions_dir(app)?;
        let file_path = sessions_dir.join(format!("{}.json", self.session_id));

        // Atomic write
        let tmp_path = file_path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).context("Failed to serialize session")?;
        fs::write(&tmp_path, json).context("Failed to write temporary session file")?;
        fs::rename(&tmp_path, &file_path).context("Failed to rename temporary session file")?;

        // Update index
        self.update_index(app)?;

        Ok(())
    }

    fn update_index(&self, app: &AppHandle) -> Result<()> {
        let sessions_dir = Self::get_sessions_dir(app)?;
        let index_path = sessions_dir.join("index.json");

        let mut index = if index_path.exists() {
            let content = fs::read_to_string(&index_path).context("Failed to read index.json")?;
            serde_json::from_str::<Vec<SessionSummary>>(&content)
                .context("Failed to parse index.json")?
        } else {
            Vec::new()
        };

        let summary = SessionSummary {
            session_id: self.session_id.clone(),
            source_file_name: self.source_file_name.clone(),
            created_at: self.created_at.clone(),
            status: self.status.clone(),
            model_family: self.model_family.clone(),
            elapsed_seconds: self.elapsed_seconds,
        };

        // Update or append
        if let Some(pos) = index.iter().position(|s| s.session_id == self.session_id) {
            index[pos] = summary;
        } else {
            index.push(summary);
            // Sort descending by created_at
            index.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        }

        // Atomic write index
        let tmp_path = index_path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&index).context("Failed to serialize index")?;
        fs::write(&tmp_path, json).context("Failed to write temporary index file")?;
        fs::rename(&tmp_path, &index_path).context("Failed to rename temporary index file")?;

        Ok(())
    }

    pub fn load(app: &AppHandle, session_id: &str) -> Result<Self> {
        let sessions_dir = Self::get_sessions_dir(app)?;
        let file_path = sessions_dir.join(format!("{}.json", session_id));

        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read session file for {}", session_id))?;

        serde_json::from_str::<Self>(&content)
            .with_context(|| format!("Failed to parse session file for {}", session_id))
    }

    pub fn delete(app: &AppHandle, session_id: &str, delete_transcript_file: bool) -> Result<()> {
        let sessions_dir = Self::get_sessions_dir(app)?;
        let file_path = sessions_dir.join(format!("{}.json", session_id));

        if file_path.exists() {
            let session = Self::load(app, session_id)?;
            if delete_transcript_file {
                if let Some(path) = session.transcript_file_path {
                    let path = Path::new(&path);
                    if path.exists() {
                        let _ = fs::remove_file(path);
                    }
                }
            }
            fs::remove_file(file_path).context("Failed to delete session file")?;
        }

        // Update index
        let index_path = sessions_dir.join("index.json");
        if index_path.exists() {
            let content = fs::read_to_string(&index_path).context("Failed to read index.json")?;
            let mut index = serde_json::from_str::<Vec<SessionSummary>>(&content)
                .context("Failed to parse index.json")?;

            if let Some(pos) = index.iter().position(|s| s.session_id == session_id) {
                index.remove(pos);
                let tmp_path = index_path.with_extension("json.tmp");
                let json =
                    serde_json::to_string_pretty(&index).context("Failed to serialize index")?;
                fs::write(&tmp_path, json).context("Failed to write temporary index file")?;
                fs::rename(&tmp_path, &index_path)
                    .context("Failed to rename temporary index file")?;
            }
        }

        Ok(())
    }

    pub fn list_summaries(app: &AppHandle) -> Result<Vec<SessionSummary>> {
        let sessions_dir = Self::get_sessions_dir(app)?;
        let index_path = sessions_dir.join("index.json");

        if !index_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&index_path).context("Failed to read index.json")?;
        serde_json::from_str::<Vec<SessionSummary>>(&content).context("Failed to parse index.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a dummy AppHandle or mock it
    // In unit tests, we can't easily get an AppHandle, so we might need to refactor
    // the methods to take a PathBuf instead of AppHandle, or just test serialization.

    #[test]
    fn test_session_serialization() {
        let session = TranscriptionSession {
            session_id: "test-id".to_string(),
            source_file_path: "/path/to/file.wav".to_string(),
            source_file_name: "file.wav".to_string(),
            source_file_size: 1024,
            source_file_type: "wav".to_string(),
            selected_model_id: "model-id".to_string(),
            model_family: ModelFamily::Whisper,
            created_at: "2026-05-12T00:00:00Z".to_string(),
            completed_at: None,
            elapsed_seconds: None,
            status: SessionStatus::Queued,
            progress: TranscriptionProgress::default(),
            chunk_count: 0,
            chunk_metadata: Vec::new(),
            transcript_text: None,
            transcript_file_path: None,
            error_message: None,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: TranscriptionSession = serde_json::from_str(&json).unwrap();
        assert_eq!(session.session_id, deserialized.session_id);
        assert_eq!(session.model_family, deserialized.model_family);
        assert_eq!(session.status, deserialized.status);
    }
}
