use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tauri::{AppHandle, Emitter};
use anyhow::{Context, Result};
use chrono::Utc;
use log::{info, error, warn};
use rodio::Decoder;
use rodio::source::UniformSourceIterator;
use cpal::Sample;
use std::io::BufReader;
use std::fs::File;

use super::session::{TranscriptionSession, SessionStatus, TranscriptionProgress, ChunkInfo, ModelFamily};
use super::engine::{TranscriptionEngine, TranscriptionOptions, TranscriptionResult};
use super::registry::TranscriptionModelInfo;

pub async fn run_transcription_pipeline(
    app: AppHandle,
    session_id: String,
    model_info: TranscriptionModelInfo,
    options: TranscriptionOptions,
) -> Result<()> {
    let mut session = TranscriptionSession::load(&app, &session_id)?;
    session.status = SessionStatus::InProgress;
    session.save(&app)?;

    let start_time = std::time::Instant::now();

    // 1. Decode audio
    info!("Decoding audio: {}", session.source_file_path);
    let samples = match decode_audio(&session.source_file_path) {
        Ok(s) => s,
        Err(e) => {
            session.status = SessionStatus::Failed;
            session.error_message = Some(format!("Failed to decode audio: {}", e));
            let _ = session.save(&app);
            return Err(e);
        }
    };

    let total_samples = samples.len();
    let sample_rate = 16000;
    let chunk_size_samples = (options.chunk_size_secs * sample_rate as f64) as usize;
    let overlap_samples = (options.overlap_secs * sample_rate as f64) as usize;
    let step_samples = chunk_size_samples - overlap_samples;

    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < total_samples {
        let mut end = offset + chunk_size_samples;
        if end > total_samples {
            end = total_samples;
        }

        // If the remaining part is very short, just include it in the last chunk
        if total_samples - end < (5.0 * sample_rate as f64) as usize {
            end = total_samples;
        }

        chunks.push(ChunkInfo {
            index: chunks.len() as u32,
            start_sec: offset as f64 / sample_rate as f64,
            end_sec: end as f64 / sample_rate as f64,
            byte_offset: (offset * 4) as u64, // f32 = 4 bytes
            byte_length: ((end - offset) * 4) as u64,
            text: None,
        });

        if end == total_samples {
            break;
        }
        offset += step_samples;
    }

    let total_chunks = chunks.len();
    session.chunk_count = total_chunks as u32;
    session.save(&app)?;

    // 2. Initialize engine
    let mut engine = create_engine(&app, &model_info.family);
    engine.load(&model_info.path).await.map_err(|e| anyhow::anyhow!(e))?;

    // 3. Process chunks
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    
    // Spawn a task to relay progress from engine if it sends any
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = app_clone.emit("transcription-progress", serde_json::json!({
                "session_id": session_id_clone,
                "progress": progress
            }));
        }
    });

    let temp_dir = std::env::temp_dir().join(format!("handy_transcription_{}", session_id));
    if !temp_dir.exists() {
        fs::create_dir_all(&temp_dir)?;
    }

    let mut full_text_parts = Vec::new();

    for (i, chunk_info) in chunks.iter_mut().enumerate() {
        info!("Processing chunk {} of {}", i + 1, total_chunks);
        
        let progress = TranscriptionProgress {
            percent: (i as f64 / total_chunks as f64) * 100.0,
            current_chunk: (i + 1) as u32,
            total_chunks: total_chunks as u32,
            message: format!("Transcribing chunk {} of {}...", i + 1, total_chunks),
        };
        session.progress = progress.clone();
        session.save(&app)?;
        
        let _ = app.emit("transcription-progress", serde_json::json!({
            "session_id": session_id,
            "progress": progress
        }));

        let start_idx = (chunk_info.byte_offset / 4) as usize;
        let end_idx = (chunk_info.byte_offset / 4 + chunk_info.byte_length / 4) as usize;
        let chunk_samples = &samples[start_idx..end_idx];
        let chunk_path = temp_dir.join(format!("chunk_{}.wav", i));
        save_wav_file(&chunk_path, chunk_samples)?;

        let result = match engine.transcribe(chunk_path.to_str().unwrap(), &options, progress_tx.clone()).await {
            Ok(res) => res,
            Err(e) => {
                warn!("Chunk {} failed: {}. Retrying once...", i, e);
                // Retry once
                match engine.transcribe(chunk_path.to_str().unwrap(), &options, progress_tx.clone()).await {
                    Ok(res) => res,
                    Err(e) => {
                        error!("Chunk {} failed again: {}", i, e);
                        TranscriptionResult {
                            full_text: format!("[Error transcribing chunk {}]", i),
                            segments: Vec::new(),
                            processing_time_secs: 0.0,
                        }
                    }
                }
            }
        };

        chunk_info.text = Some(result.full_text.clone());
        full_text_parts.push(result.full_text);
        
        // Clean up chunk file
        let _ = fs::remove_file(chunk_path);
    }

    // 4. Stitch and finalize
    let stitched_text = stitch_text(&full_text_parts);
    session.transcript_text = Some(stitched_text);
    session.chunk_metadata = chunks;
    session.status = SessionStatus::Completed;
    session.completed_at = Some(Utc::now().to_rfc3339());
    session.elapsed_seconds = Some(start_time.elapsed().as_secs_f64());
    
    // Save .txt file
    match save_transcript_file(&app, &session) {
        Ok(path) => session.transcript_file_path = Some(path.to_string_lossy().into_owned()),
        Err(e) => {
            error!("Failed to save transcript file: {}", e);
            session.error_message = Some(format!("Transcription succeeded but failed to save .txt file: {}", e));
        }
    }

    session.save(&app)?;

    // Clean up temp dir
    let _ = fs::remove_dir_all(temp_dir);

    let _ = app.emit("transcription-complete", serde_json::json!({
        "session_id": session_id,
        "status": "Completed"
    }));

    info!("Transcription pipeline completed for session {}", session_id);
    Ok(())
}

fn decode_audio(path: &str) -> Result<Vec<f32>> {
    let file = File::open(path).context("Failed to open audio file")?;
    let source = Decoder::new(BufReader::new(file)).context("Failed to create decoder")?;
    
    // UniformSourceIterator converts to target sample rate and channels
    let resampled = UniformSourceIterator::new(source, 1, 16000);
    
    Ok(resampled.map(|s| s.to_float_sample()).collect())
}

fn save_wav_file(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude)?;
    }
    writer.finalize()?;
    Ok(())
}

fn save_transcript_file(app: &AppHandle, session: &TranscriptionSession) -> Result<PathBuf> {
    let original_path = Path::new(&session.source_file_path);
    let transcript_path = original_path.with_extension("transcript.txt");
    
    let text = session.transcript_text.as_ref().cloned().unwrap_or_default();
    
    if let Err(_) = fs::write(&transcript_path, &text) {
        // Fallback to app data transcripts dir
        let app_data_dir = crate::portable::app_data_dir(app)
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?;
        let transcripts_dir = app_data_dir.join("transcripts");
        if !transcripts_dir.exists() {
            fs::create_dir_all(&transcripts_dir)?;
        }
        let fallback_path = transcripts_dir.join(format!("{}.txt", session.session_id));
        fs::write(&fallback_path, &text)?;
        Ok(fallback_path)
    } else {
        Ok(transcript_path)
    }
}

fn stitch_text(parts: &[String]) -> String {
    if parts.is_empty() { return String::new(); }
    
    let mut result = parts[0].clone();
    
    for i in 0..parts.len() - 1 {
        let current = &parts[i];
        let next = &parts[i+1];
        
        let overlap = find_overlap(current, next, 20, 5);
        if let Some(overlap_len) = overlap {
            let next_trimmed: String = next.chars().skip(overlap_len).collect();
            result.push_str(" ");
            result.push_str(&next_trimmed);
        } else {
            result.push_str(" ");
            result.push_str(next);
        }
    }
    
    result
}

fn find_overlap(s1: &str, s2: &str, max_words: usize, min_words: usize) -> Option<usize> {
    let words1: Vec<&str> = s1.split_whitespace().collect();
    let words2: Vec<&str> = s2.split_whitespace().collect();

    if words1.is_empty() || words2.is_empty() {
        return None;
    }

    let n1 = words1.len();
    let n2 = words2.len();

    let check_len = std::cmp::min(n1, std::cmp::min(n2, max_words));

    for len in (min_words..=check_len).rev() {
        let tail = &words1[n1 - len..];
        let head = &words2[..len];

        if tail == head {
            // Found overlap! Calculate character offset in s2
            let mut char_count = 0;
            // Safer way to find the character offset
            let mut s2_iter = s2.split_whitespace();
            for _ in 0..len {
                if let Some(word) = s2_iter.next() {
                    if let Some(pos) = s2[char_count..].find(word) {
                        char_count += pos + word.len();
                    }
                }
            }
            return Some(char_count);
        }
    }

    None
}

fn create_engine(app: &AppHandle, family: &ModelFamily) -> Box<dyn TranscriptionEngine> {
    match family {
        ModelFamily::Whisper => Box::new(super::adapters::whisper::WhisperAdapter::new(app.clone())),
        ModelFamily::Parakeet => Box::new(super::adapters::parakeet::ParakeetAdapter),
        ModelFamily::Moonshine => Box::new(super::adapters::moonshine::MoonshineAdapter),
        ModelFamily::Unknown(_) => Box::new(super::adapters::whisper::WhisperAdapter::new(app.clone())), // Fallback
    }
}
