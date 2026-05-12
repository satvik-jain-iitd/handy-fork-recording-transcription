use tauri::AppHandle;
use uuid::Uuid;
use chrono::Utc;
use std::path::Path;
use log::error;

use super::session::{TranscriptionSession, SessionStatus, SessionSummary, TranscriptionProgress};
use super::registry::{TranscriptionModelInfo, scan_models_directory};
use super::pipeline::{run_transcription_pipeline};
use super::engine::TranscriptionOptions;

#[tauri::command]
#[specta::specta]
pub async fn list_transcription_models(
    app_handle: AppHandle,
) -> Result<Vec<TranscriptionModelInfo>, String> {
    let app_data_dir = crate::portable::app_data_dir(&app_handle)
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let models_path = app_data_dir.join("models");
    Ok(scan_models_directory(&models_path))
}

#[tauri::command]
#[specta::specta]
pub async fn start_transcription(
    app_handle: tauri::AppHandle,
    file_path: String,
    model_id: String,
    language: Option<String>,
) -> Result<String, String> {
    // 1. Validate file existence
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // 2. Validate file extension
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let supported_exts = vec!["wav", "mp3", "m4a", "aac", "mp4", "mkv", "webm"];
    if !supported_exts.contains(&ext.as_str()) {
        return Err(format!("Unsupported file type: .{}. Supported: wav, mp3, m4a, aac, mp4, mkv, webm", ext));
    }

    // 3. Look up model
    let app_data_dir = crate::portable::app_data_dir(&app_handle)
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let models_path = app_data_dir.join("models");
    let models = scan_models_directory(&models_path);
    
    let model_info = models.into_iter().find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.supported {
        return Err(format!("Model {} is not yet supported for transcription. Please use a Whisper model.", model_info.display_name));
    }

    // 4. Create session
    let session_id = Uuid::new_v4().to_string();
    let session = TranscriptionSession {
        session_id: session_id.clone(),
        source_file_path: file_path.clone(),
        source_file_name: path.file_name().unwrap().to_string_lossy().into_owned(),
        source_file_size: path.metadata().map(|m| m.len()).unwrap_or(0),
        source_file_type: ext,
        selected_model_id: model_id,
        model_family: model_info.family.clone(),
        created_at: Utc::now().to_rfc3339(),
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

    session.save(&app_handle).map_err(|e| e.to_string())?;

    // 5. Spawn background task
    let app_clone = app_handle.clone();
    let session_id_clone = session_id.clone();
    let model_info_clone = model_info.clone();
    
    let options = TranscriptionOptions {
        language,
        ..Default::default()
    };

    tokio::spawn(async move {
        if let Err(e) = run_transcription_pipeline(app_clone, session_id_clone, model_info_clone, options).await {
            error!("Transcription pipeline failed: {}", e);
        }
    });

    Ok(session_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_session(
    app_handle: AppHandle,
    session_id: String,
) -> Result<TranscriptionSession, String> {
    TranscriptionSession::load(&app_handle, &session_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn list_transcription_sessions(
    app_handle: AppHandle,
) -> Result<Vec<SessionSummary>, String> {
    TranscriptionSession::list_summaries(&app_handle).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn export_transcript(
    app_handle: AppHandle,
    session_id: String,
    output_path: Option<String>,
) -> Result<String, String> {
    let session = TranscriptionSession::load(&app_handle, &session_id).map_err(|e| e.to_string())?;
    let text = session.transcript_text.ok_or_else(|| "No transcript available for this session.".to_string())?;

    if let Some(path) = output_path {
        std::fs::write(&path, text).map_err(|e| format!("Failed to write to path {}: {}", path, e))?;
        Ok(path)
    } else {
        Err("Export requires an output path in this version.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn delete_transcription_session(
    app_handle: AppHandle,
    session_id: String,
    delete_transcript_file: bool,
) -> Result<(), String> {
    TranscriptionSession::delete(&app_handle, &session_id, delete_transcript_file).map_err(|e| e.to_string())
}
