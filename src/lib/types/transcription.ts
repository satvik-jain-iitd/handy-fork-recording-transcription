import { ModelInfo, SessionStatus, TranscriptionProgress, TranscriptionSession, SessionSummary } from "@/bindings";

export interface ProgressPayload {
    session_id: string;
    progress: TranscriptionProgress;
}

export interface CompletePayload {
    session_id: string;
    status: SessionStatus;
}
