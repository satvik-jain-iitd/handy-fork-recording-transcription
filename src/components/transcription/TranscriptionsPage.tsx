import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  FileAudio,
  History,
  Upload,
  X,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Download,
  ExternalLink,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";

import {
  commands,
  TranscriptionModelInfo,
  TranscriptionProgress,
  TranscriptionSession,
  SessionSummary,
  ModelFamily,
} from "@/bindings";
import { Button } from "../ui/Button";
import { Select, SelectOption } from "../ui/Select";
import Badge from "../ui/Badge";
import { ProgressPayload, CompletePayload } from "@/lib/types/transcription";

type ViewMode = "upload" | "history";

const TranscriptionsPage: React.FC = () => {
  const { t } = useTranslation();
  const [viewMode, setViewMode] = useState<ViewMode>("upload");
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [selectedFileInfo, setSelectedFileInfo] = useState<{
    name: string;
    size: number;
    type: string;
  } | null>(null);
  const [availableModels, setAvailableModels] = useState<
    TranscriptionModelInfo[]
  >([]);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [currentProgress, setCurrentProgress] =
    useState<TranscriptionProgress | null>(null);
  const [currentSession, setCurrentSession] =
    useState<TranscriptionSession | null>(null);
  const [sessionHistory, setSessionHistory] = useState<SessionSummary[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isModelsLoading, setIsModelsLoading] = useState(false);
  const [isHistoryLoading, setIsHistoryLoading] = useState(false);

  // Load models on mount
  useEffect(() => {
    loadModels();
    loadHistory();
  }, []);

  const loadModels = async () => {
    setIsModelsLoading(true);
    try {
      const result = await commands.listTranscriptionModels();
      if (result.status === "ok") {
        setAvailableModels(result.data);
        const firstSupported = result.data.find((m) => m.supported);
        if (firstSupported) {
          setSelectedModelId(firstSupported.id);
        }
      }
    } catch (e) {
      console.error("Failed to load models:", e);
      toast.error("Failed to load transcription models");
    } finally {
      setIsModelsLoading(false);
    }
  };

  const loadHistory = async () => {
    setIsHistoryLoading(true);
    try {
      const result = await commands.listTranscriptionSessions();
      if (result.status === "ok") {
        setSessionHistory(result.data);
      }
    } catch (e) {
      console.error("Failed to load history:", e);
    } finally {
      setIsHistoryLoading(false);
    }
  };

  // Event listeners
  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;

    const setupListeners = async () => {
      unlistenProgress = await listen<ProgressPayload>(
        "transcription-progress",
        (event) => {
          if (event.payload.session_id === currentSessionId) {
            setCurrentProgress(event.payload.progress);
          }
        },
      );

      unlistenComplete = await listen<CompletePayload>(
        "transcription-complete",
        async (event) => {
          if (event.payload.session_id === currentSessionId) {
            setIsLoading(false);
            const result = await commands.getTranscriptionSession(
              event.payload.session_id,
            );
            if (result.status === "ok") {
              setCurrentSession(result.data);
              toast.success("Transcription complete!");
            }
            loadHistory();
          }
        },
      );
    };

    if (currentSessionId) {
      setupListeners();
    }

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    };
  }, [currentSessionId]);

  const handleFileSelect = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Audio/Video",
            extensions: ["wav", "mp3", "m4a", "aac", "mp4", "mkv", "webm"],
          },
        ],
      });

      if (selected && !Array.isArray(selected)) {
        setSelectedFilePath(selected);
        const name = selected.split(/[\\/]/).pop() || selected;
        setSelectedFileInfo({
          name,
          size: 0,
          type: name.split(".").pop() || "",
        });
      }
    } catch (e) {
      console.error("File selection failed:", e);
    }
  };

  const handleTranscribe = async () => {
    if (!selectedFilePath || !selectedModelId) return;

    setIsLoading(true);
    setCurrentProgress(null);
    setCurrentSession(null);

    try {
      const result = await commands.startTranscription(
        selectedFilePath,
        selectedModelId,
        null,
      );
      if (result.status === "ok") {
        setCurrentSessionId(result.data);
      } else {
        toast.error(result.error);
        setIsLoading(false);
      }
    } catch (e) {
      toast.error("Failed to start transcription");
      setIsLoading(false);
    }
  };

  const handleExport = async (sessionId: string) => {
    try {
      const path = await save({
        filters: [{ name: "Text", extensions: ["txt"] }],
        defaultPath: "transcript.txt",
      });
      if (path) {
        const result = await commands.exportTranscript(sessionId, path);
        if (result.status === "ok") {
          toast.success("Transcript exported successfully");
        } else {
          toast.error(result.error);
        }
      }
    } catch (e) {
      toast.error("Export failed");
    }
  };

  const loadSession = async (sessionId: string) => {
    try {
      const result = await commands.getTranscriptionSession(sessionId);
      if (result.status === "ok") {
        setCurrentSession(result.data);
        setCurrentSessionId(result.data.session_id);
        setSelectedFilePath(result.data.source_file_path);
        setSelectedFileInfo({
          name: result.data.source_file_name,
          size: result.data.source_file_size,
          type: result.data.source_file_type,
        });
        setViewMode("upload");
      }
    } catch (e) {
      toast.error("Failed to load session details");
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return "Unknown size";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  const getFamilyName = (family: ModelFamily): string => {
    if (typeof family === "string") return family;
    return family.Unknown;
  };

  const modelOptions: SelectOption[] = availableModels.map((m) => ({
    value: m.id,
    label: `${m.display_name} (${getFamilyName(m.family)}) ${m.supported ? "" : "(Coming soon)"}`,
    isDisabled: !m.supported,
  }));

  return (
    <div className="flex flex-col h-full bg-background text-text overflow-hidden">
      <div className="flex border-b border-mid-gray/20">
        <button
          className={`flex-1 py-3 px-4 text-sm font-medium transition-colors ${viewMode === "upload" ? "text-logo-primary border-b-2 border-logo-primary" : "text-text/60 hover:text-text/80"}`}
          onClick={() => setViewMode("upload")}
        >
          <div className="flex items-center justify-center gap-2">
            <Upload size={18} />
            Transcribe File
          </div>
        </button>
        <button
          className={`flex-1 py-3 px-4 text-sm font-medium transition-colors ${viewMode === "history" ? "text-logo-primary border-b-2 border-logo-primary" : "text-text/60 hover:text-text/80"}`}
          onClick={() => setViewMode("history")}
        >
          <div className="flex items-center justify-center gap-2">
            <History size={18} />
            History
          </div>
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {viewMode === "upload" ? (
          <div className="max-w-2xl mx-auto space-y-8">
            {/* File Selection */}
            <div className="space-y-4">
              <h2 className="text-lg font-semibold flex items-center gap-2">
                <FileAudio className="text-logo-primary" />
                Select Audio or Video
              </h2>

              {!selectedFilePath ? (
                <div
                  className="border-2 border-dashed border-mid-gray/30 rounded-xl p-10 flex flex-col items-center justify-center gap-4 hover:border-logo-primary/50 hover:bg-logo-primary/5 transition-all cursor-pointer group"
                  onClick={handleFileSelect}
                >
                  <div className="bg-mid-gray/10 p-4 rounded-full group-hover:bg-logo-primary/20 transition-colors">
                    <Upload
                      size={32}
                      className="text-text/40 group-hover:text-logo-primary"
                    />
                  </div>
                  <p className="text-text/60 text-center">
                    Click to browse or drop an audio or video file here.
                    <br />
                    <span className="text-xs">
                      Supported: wav, mp3, m4a, aac, mp4, mkv, webm
                    </span>
                  </p>
                </div>
              ) : (
                <div className="bg-mid-gray/10 rounded-xl p-4 flex items-center justify-between border border-mid-gray/20">
                  <div className="flex items-center gap-3 overflow-hidden">
                    <div className="bg-logo-primary/20 p-2 rounded-lg shrink-0">
                      <FileAudio size={24} className="text-logo-primary" />
                    </div>
                    <div className="overflow-hidden">
                      <p
                        className="font-medium truncate"
                        title={selectedFilePath}
                      >
                        {selectedFileInfo?.name}
                      </p>
                      <p className="text-xs text-text/50">
                        {formatSize(selectedFileInfo?.size || 0)} •{" "}
                        {selectedFileInfo?.type.toUpperCase()}
                      </p>
                    </div>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setSelectedFilePath(null);
                      setCurrentSession(null);
                      setCurrentProgress(null);
                    }}
                    disabled={isLoading}
                  >
                    <X size={18} />
                  </Button>
                </div>
              )}
            </div>

            {/* Model Selection */}
            <div className="space-y-4">
              <h2 className="text-lg font-semibold flex items-center gap-2">
                <Loader2 className="text-logo-primary" />
                Transcription Model
              </h2>
              <Select
                value={selectedModelId}
                options={modelOptions}
                onChange={(val) => setSelectedModelId(val)}
                disabled={isLoading || isModelsLoading}
                placeholder="Select a model..."
              />
              {availableModels.length === 0 && !isModelsLoading && (
                <p className="text-xs text-red-400">
                  No transcription models installed. Place Whisper model files in
                  your models folder.
                </p>
              )}
            </div>

            {/* Actions & Progress */}
            <div className="pt-4 flex flex-col items-center gap-6">
              <Button
                variant="primary"
                size="lg"
                className="w-full h-12 text-base shadow-lg shadow-logo-primary/20"
                onClick={handleTranscribe}
                disabled={isLoading || !selectedFilePath || !selectedModelId}
              >
                {isLoading ? (
                  <div className="flex items-center gap-2">
                    <Loader2 size={20} className="animate-spin" />
                    Transcribing...
                  </div>
                ) : (
                  "Start Transcription"
                )}
              </Button>

              {isLoading && currentProgress && (
                <div className="w-full space-y-2">
                  <div className="flex justify-between text-sm text-text/60 font-medium">
                    <span>{currentProgress.message}</span>
                    <span>{Math.round(currentProgress.percent)}%</span>
                  </div>
                  <div className="w-full bg-mid-gray/20 h-2 rounded-full overflow-hidden">
                    <div
                      className="bg-logo-primary h-full transition-all duration-300 ease-out"
                      style={{ width: `${currentProgress.percent}%` }}
                    />
                  </div>
                </div>
              )}
            </div>

            {/* Results Viewer */}
            {currentSession && currentSession.status === "Completed" && (
              <div className="space-y-4 animate-in fade-in slide-in-from-bottom-4 duration-500">
                <div className="flex items-center justify-between">
                  <h2 className="text-lg font-semibold flex items-center gap-2">
                    <CheckCircle2 className="text-green-500" />
                    Completed Transcript
                  </h2>
                  <div className="flex gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => handleExport(currentSession.session_id)}
                    >
                      <Download size={16} className="mr-2" />
                      Export
                    </Button>
                  </div>
                </div>

                <div className="bg-mid-gray/5 border border-mid-gray/20 rounded-xl p-4 max-h-96 overflow-y-auto whitespace-pre-wrap text-sm leading-relaxed font-mono text-text">
                  {currentSession.transcript_text}
                </div>

                {currentSession.transcript_file_path && (
                  <p className="text-xs text-text/40 flex items-center gap-1">
                    <CheckCircle2 size={12} className="text-green-500/50" />
                    Automatically saved next to source file.
                  </p>
                )}
              </div>
            )}

            {currentSession && currentSession.status === "Failed" && (
              <div className="bg-red-500/10 border border-red-500/20 rounded-xl p-4 flex gap-3 items-start animate-in fade-in duration-300">
                <AlertCircle className="text-red-500 shrink-0" />
                <div>
                  <p className="font-semibold text-red-500">Transcription Failed</p>
                  <p className="text-sm text-red-400/80">
                    {currentSession.error_message}
                  </p>
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="max-w-4xl mx-auto space-y-6">
            <div className="flex items-center justify-between">
              <h2 className="text-xl font-bold">Transcription History</h2>
              <Button
                variant="ghost"
                size="sm"
                onClick={loadHistory}
                disabled={isHistoryLoading}
              >
                <Loader2
                  size={16}
                  className={isHistoryLoading ? "animate-spin" : ""}
                />
              </Button>
            </div>

            {isHistoryLoading && sessionHistory.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-20 gap-4 opacity-40">
                <Loader2 size={48} className="animate-spin text-logo-primary" />
                <p>Loading history...</p>
              </div>
            ) : sessionHistory.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-20 gap-4 opacity-40 border-2 border-dashed border-mid-gray/20 rounded-2xl">
                <History size={48} />
                <p>No transcription history yet.</p>
              </div>
            ) : (
              <div className="grid gap-3">
                {sessionHistory.map((session) => (
                  <div
                    key={session.session_id}
                    className="bg-mid-gray/10 border border-mid-gray/20 rounded-xl p-4 flex items-center justify-between hover:bg-mid-gray/15 hover:border-logo-primary/30 transition-all cursor-pointer group"
                    onClick={() => loadSession(session.session_id)}
                  >
                    <div className="flex items-center gap-4 overflow-hidden">
                      <div
                        className={`p-2 rounded-lg shrink-0 ${session.status === "Completed" ? "bg-green-500/10 text-green-500" : session.status === "Failed" ? "bg-red-500/10 text-red-500" : "bg-logo-primary/10 text-logo-primary"}`}
                      >
                        <FileAudio size={20} />
                      </div>
                      <div className="overflow-hidden">
                        <p className="font-medium truncate">
                          {session.source_file_name}
                        </p>
                        <div className="flex items-center gap-3 text-xs text-text/50">
                          <span>
                            {new Date(session.created_at).toLocaleString()}
                          </span>
                          <span>•</span>
                          <span>{getFamilyName(session.model_family)}</span>
                          {session.elapsed_seconds && (
                            <>
                              <span>•</span>
                              <span>{Math.round(session.elapsed_seconds)}s</span>
                            </>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-4">
                      <Badge
                        variant={
                          session.status === "Completed"
                            ? "success"
                            : session.status === "Failed"
                              ? "primary"
                              : "secondary"
                        }
                      >
                        {session.status}
                      </Badge>
                      <ExternalLink
                        size={16}
                        className="text-text/20 group-hover:text-logo-primary transition-colors"
                      />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default TranscriptionsPage;
