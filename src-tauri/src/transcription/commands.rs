use tauri::AppHandle;
use uuid::Uuid;
use chrono::Utc;
use std::path::Path;
use log::error;
use std::sync::Arc;

use super::session::{TranscriptionSession, SessionStatus, SessionSummary, TranscriptionProgress};
use super::registry::{TranscriptionModelInfo, scan_models_directory};
use super::pipeline::{run_transcription_pipeline};
use super::live_pipeline::LiveTranscriptionPipeline;
use super::engine::{TranscriptionOptions, TranscriptionEngine};
use super::live_manager::{LiveMeetingManager, LiveMeetingState};

#[tauri::command]
#[specta::specta]
pub async fn start_live_meeting(
    app_handle: AppHandle,
    manager: tauri::State<'_, LiveMeetingManager>,
    model_id: String,
) -> Result<String, String> {
    let app_data_dir = crate::portable::app_data_dir(&app_handle)
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let models_path = app_data_dir.join("models");
    let models = scan_models_directory(&models_path);
    
    let model_info = models.into_iter().find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    let session_id = Uuid::new_v4().to_string();
    
    let (mixed_tx, mixed_rx) = crossbeam_channel::unbounded();
    
    let mut recorder = crate::audio_toolkit::audio::MeetingRecorder::new(mixed_tx);
    recorder.start()?;

    let mut engine = create_engine(&app_handle, &model_info.family);
    engine.load(&model_info.path).await?;

    let vad = super::super::managers::transcription::TranscriptionManager::get_vad_detector(&app_handle);

    let pipeline = LiveTranscriptionPipeline::new(
        app_handle.clone(),
        engine,
        vad,
        mixed_rx,
        session_id.clone()
    );

    tokio::spawn(async move {
        pipeline.run().await;
    });

    manager.add_meeting(session_id.clone(), LiveMeetingState {
        recorder,
        session_id: session_id.clone(),
    });

    Ok(session_id)
}

#[tauri::command]
#[specta::specta]
pub async fn stop_live_meeting(
    manager: tauri::State<'_, LiveMeetingManager>,
    session_id: String,
) -> Result<(), String> {
    if let Some(mut state) = manager.remove_meeting(&session_id) {
        state.recorder.stop();
    }
    Ok(())
}

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
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let supported_exts = vec!["wav", "mp3", "m4a", "aac", "mp4", "mkv", "webm"];
    if !supported_exts.contains(&ext.as_str()) {
        return Err(format!("Unsupported file type: .{}. Supported: wav, mp3, m4a, aac, mp4, mkv, webm", ext));
    }

    let app_data_dir = crate::portable::app_data_dir(&app_handle)
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let models_path = app_data_dir.join("models");
    let models = scan_models_directory(&models_path);
    
    let model_info = models.into_iter().find(|m| m.id == model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.supported {
        return Err(format!("Model {} is not yet supported for transcription. Please use a Whisper model.", model_info.display_name));
    }

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

fn create_engine(app: &AppHandle, family: &super::session::ModelFamily) -> Box<dyn TranscriptionEngine> {
    match family {
        super::session::ModelFamily::Whisper => Box::new(super::adapters::whisper::WhisperAdapter::new(app.clone())),
        super::session::ModelFamily::Parakeet => Box::new(super::adapters::parakeet::ParakeetAdapter),
        super::session::ModelFamily::Moonshine => Box::new(super::adapters::moonshine::MoonshineAdapter),
        super::session::ModelFamily::Unknown(_) => Box::new(super::adapters::whisper::WhisperAdapter::new(app.clone())),
    }
}
