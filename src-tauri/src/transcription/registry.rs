use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::Path;
use log::warn;

use super::session::ModelFamily;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscriptionModelInfo {
    /// Stable identifier derived from directory name or file stem.
    /// e.g., "whisper-base-en", "whisper-medium-v3", "parakeet-tdt-0.6b-v2"
    pub id: String,
    pub family: ModelFamily,
    /// User-facing display name.
    pub display_name: String,
    /// Absolute path to the model's primary file or directory.
    pub path: String,
    /// True only if the adapter for this family is implemented and functional.
    pub supported: bool,
    pub capabilities: TranscriptionModelCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscriptionModelCapabilities {
    /// Supported language codes, e.g. ["en"] or ["multilingual"].
    pub languages: Vec<String>,
    /// Whether the model produces word/segment timestamps.
    pub timestamps: bool,
    /// Recommended audio chunk size in seconds for this model.
    pub recommended_chunk_size_secs: f64,
    /// Human-readable description for the UI tooltip.
    pub description: String,
    /// Known limitations, e.g. ["English only", "No timestamps"].
    pub limitations: Vec<String>,
}

pub fn scan_models_directory(base_path: &Path) -> Vec<TranscriptionModelInfo> {
    if !base_path.exists() {
        return Vec::new();
    }

    let mut models = Vec::new();

    let entries = match fs::read_dir(base_path) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read models directory {}: {}", base_path.display(), e);
            return Vec::new();
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if let Some(model) = detect_model(&path, &name) {
            models.push(model);
        }
    }

    // Sort: supported first, then alphabetically by display_name
    models.sort_by(|a, b| {
        b.supported.cmp(&a.supported)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    models
}

fn detect_model(path: &Path, name: &str) -> Option<TranscriptionModelInfo> {
    // Whisper detection
    if is_whisper_model(path, name) {
        let id = path.file_stem()?.to_string_lossy().into_owned();
        let display_name = humanize_name(&id);
        let is_en = id.to_lowercase().contains("en");
        
        return Some(TranscriptionModelInfo {
            id,
            family: ModelFamily::Whisper,
            display_name,
            path: path.to_string_lossy().into_owned(),
            supported: true,
            capabilities: TranscriptionModelCapabilities {
                languages: if is_en { vec!["en".to_string()] } else { vec!["multilingual".to_string()] },
                timestamps: true,
                recommended_chunk_size_secs: 60.0,
                description: "OpenAI Whisper model for robust speech recognition.".to_string(),
                limitations: if is_en { vec!["English only".to_string()] } else { Vec::new() },
            },
        });
    }

    // Parakeet detection
    if name.to_lowercase().contains("parakeet") || has_parakeet_files(path) {
        let is_v3 = name.to_lowercase().contains("v3");
        return Some(TranscriptionModelInfo {
            id: name.to_string(),
            family: ModelFamily::Parakeet,
            display_name: humanize_name(name),
            path: path.to_string_lossy().into_owned(),
            supported: false, // Stub only in v1
            capabilities: TranscriptionModelCapabilities {
                languages: if is_v3 { vec!["multilingual".to_string()] } else { vec!["en".to_string()] },
                timestamps: true,
                recommended_chunk_size_secs: 1200.0,
                description: "NVIDIA Parakeet model for fast, high-quality transcription.".to_string(),
                limitations: Vec::new(),
            },
        });
    }

    // Moonshine detection
    if name.to_lowercase().contains("moonshine") || has_moonshine_files(path) {
        return Some(TranscriptionModelInfo {
            id: name.to_string(),
            family: ModelFamily::Moonshine,
            display_name: humanize_name(name),
            path: path.to_string_lossy().into_owned(),
            supported: false, // Stub only in v1
            capabilities: TranscriptionModelCapabilities {
                languages: vec!["en".to_string()],
                timestamps: true,
                recommended_chunk_size_secs: 60.0,
                description: "Useful Sensors Moonshine model for low-latency English transcription.".to_string(),
                limitations: vec!["English only".to_string()],
            },
        });
    }

    // Unknown
    if path.is_dir() {
        return Some(TranscriptionModelInfo {
            id: name.to_string(),
            family: ModelFamily::Unknown(name.to_string()),
            display_name: name.to_string(),
            path: path.to_string_lossy().into_owned(),
            supported: false,
            capabilities: TranscriptionModelCapabilities {
                languages: Vec::new(),
                timestamps: false,
                recommended_chunk_size_secs: 60.0,
                description: "Unrecognized model format. Check the Handy documentation for supported model types.".to_string(),
                limitations: Vec::new(),
            },
        });
    }

    None
}

fn is_whisper_model(path: &Path, name: &str) -> bool {
    if path.is_file() {
        name.ends_with(".bin") || name.ends_with(".ggml")
    } else if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(e) = entry {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if n.ends_with(".bin") || n.ends_with(".ggml") {
                        return true;
                    }
                }
            }
        }
        false
    } else {
        false
    }
}

fn has_parakeet_files(path: &Path) -> bool {
    if !path.is_dir() { return false; }
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(e) = entry {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.contains("parakeet") && n.ends_with(".onnx") {
                    return true;
                }
            }
        }
    }
    false
}

fn has_moonshine_files(path: &Path) -> bool {
    if !path.is_dir() { return false; }
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            if let Ok(e) = entry {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.contains("moonshine") && (n.ends_with(".onnx") || n.ends_with(".bin")) {
                    return true;
                }
            }
        }
    }
    false
}

fn humanize_name(name: &str) -> String {
    let name = name.replace('-', " ").replace('_', " ");
    let mut words: Vec<String> = name
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect();
    
    // Special case for ggml prefix
    if words.len() > 1 && words[0].to_lowercase() == "ggml" {
        words.remove(0);
        let first = words.get_mut(0).unwrap();
        *first = format!("Whisper {}", first);
    }

    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_models_empty() {
        let dir = tempdir().unwrap();
        let models = scan_models_directory(dir.path());
        assert!(models.is_empty());
    }

    #[test]
    fn test_scan_whisper_file() {
        let dir = tempdir().unwrap();
        let model_path = dir.path().join("ggml-base.bin");
        fs::write(&model_path, "dummy").unwrap();
        
        let models = scan_models_directory(dir.path());
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].family, ModelFamily::Whisper);
        assert_eq!(models[0].display_name, "Whisper Base");
        assert!(models[0].supported);
    }
}
