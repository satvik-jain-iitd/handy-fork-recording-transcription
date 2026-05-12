# Handy (Fork) - Local Meeting Transcription & File Support

[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?style=for-the-badge&logo=discord&logoColor=white)](https://discord.com/invite/WVBeWsNXK4)

**A privacy-first, offline-only tool for recording and transcribing meetings and files.**

This is a specialized fork of [Handy](https://github.com/cjpais/Handy) that adds support for local file transcription and live dual-channel meeting capture (Mic + System Audio).

## 🚀 Major Improvements in this Fork

### 1. 🎙️ Live Meeting Transcription (Offline)
Transcribe meetings from **Webex, Zoom, Teams, Slack, Skype**, or any browser-based meeting app entirely on your machine.
- **Dual-Channel Capture**: Simultaneously records your voice (Mic) and the voices of other participants (System/Speaker Output).
- **macOS Support**: Uses modern **ScreenCaptureKit** (macOS 13.0+) for high-quality system audio capture without virtual drivers.
- **Windows Support**: Uses **WASAPI Loopback** for high-performance speaker output capture.
- **Real-time Mixing**: Fuses both audio channels into a single 16kHz stream for the AI engine.

### 2. 📁 Local File Transcription
Upload and transcribe pre-recorded audio or video files.
- **Supported Formats**: `.wav`, `.mp3`, `.m4a`, `.aac`, `.mp4`, `.mkv`, and `.webm`.
- **Chunked Pipeline**: Efficiently processes long recordings (tested up to 30+ minutes) by splitting audio into manageable chunks.
- **Smart Stitching**: Implements boundary deduplication to ensure seamless text transitions between chunks.

### 3. 📜 Transcription History
- **Session Database**: All transcriptions are saved locally in a JSON-based database.
- **Viewer**: Revisit any past session, read the transcript, and see metadata (model used, duration, date).
- **Export**: Export any transcript to a `.txt` file with one click.

---

## 🛠️ Installation & Setup (Offline / Proxy Friendly)

If you are behind a strict corporate proxy or have limited internet access, follow these steps to set up Handy in **Portable Mode**.

### Step 1: Download & Prepare
1. Download the latest `.exe` (Windows) or `.dmg` (Mac) from the [Releases](https://github.com/satvik-jain-iitd/handy-fork-recording-transcription/releases) page.
2. Create a dedicated folder for Handy (e.g., `C:\HandyApp`).
3. Place the executable inside this folder.

### Step 2: Enable Portable Mode
In the same folder, create an empty file named `portable` (no extension). This tells Handy to keep all data (settings, models, history) in a local `Data` folder instead of system directories.

### Step 3: Manual Model Installation
Since the app cannot download models through a proxy:
1. Create a folder named `Data/models/` inside your Handy folder.
2. Download Whisper GGML models (`.bin` files) from [Hugging Face](https://huggingface.co/models?search=whisper%20ggml) or use these links:
   - [Whisper Base](https://blob.handy.computer/ggml-base.bin)
   - [Whisper Medium](https://blob.handy.computer/whisper-medium-q4_1.bin)
3. Place the `.bin` files directly into `Data/models/`.

### Step 4: Run
Open `Handy.exe` (or `Handy.app`). Navigate to the **Transcription** tab in the sidebar to start transcribing files or live meetings.

---

## Architecture Additions

This fork introduces several new Rust modules for the transcription engine:
- `AudioMixer`: Handles real-time resampling and mixing of asynchronous audio streams.
- `MeetingRecorder`: Cross-platform loopback capture (WASAPI/ScreenCaptureKit).
- `TranscriptionSession`: JSON-backed persistence layer for history.
- `WhisperAdapter`: A trait-based wrapper around the core inference engine.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. Built upon the original work by [cjpais](https://github.com/cjpais).
