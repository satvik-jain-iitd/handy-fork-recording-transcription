use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::audio_toolkit::audio::MeetingRecorder;
use super::live_pipeline::LiveTranscriptionPipeline;

pub struct LiveMeetingState {
    pub recorder: MeetingRecorder,
    pub session_id: String,
}

pub struct LiveMeetingManager {
    pub active_meetings: Arc<Mutex<HashMap<String, LiveMeetingState>>>,
}

impl LiveMeetingManager {
    pub fn new() -> Self {
        Self {
            active_meetings: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_meeting(&self, session_id: String, state: LiveMeetingState) {
        self.active_meetings.lock().unwrap().insert(session_id, state);
    }

    pub fn remove_meeting(&self, session_id: &str) -> Option<LiveMeetingState> {
        self.active_meetings.lock().unwrap().remove(session_id)
    }
}
