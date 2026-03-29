# CLAUDE.md — Minutes (James's Fork)

> Forked from [silverstein/minutes](https://github.com/silverstein/minutes) (v0.5.0) with a custom React frontend and Obsidian vault integration.

## What This Is

**Minutes** is an open-source, privacy-first meeting transcription tool. It captures audio locally, transcribes with whisper.cpp, identifies speakers with pyannote, and outputs structured markdown. Everything runs on-device. No cloud, no API keys required for core functionality.

**This fork** (`genelco-james/minutes`) replaces the original vanilla HTML/CSS/JS frontend with a modern React + TypeScript UI and integrates with James's Obsidian vault for meeting knowledge management.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│ Tauri v2 Desktop App (menu bar)                         │
│                                                         │
│  ┌──────────────────┐    ┌────────────────────────────┐ │
│  │ React Frontend   │◄──►│ Rust Backend (IPC)         │ │
│  │ tauri/src/        │    │ tauri/src-tauri/src/       │ │
│  │ - Inline styles   │    │ - commands.rs (40+ cmds)   │ │
│  │ - Single-file     │    │ - main.rs (tray, windows)  │ │
│  │   build (Vite)    │    │ - call_detect.rs           │ │
│  └──────────────────┘    │ - pty.rs                   │ │
│                           └────────┬───────────────────┘ │
│                                    │                     │
│                           ┌────────▼───────────────────┐ │
│                           │ minutes-core (Rust engine)  │ │
│                           │ crates/core/src/            │ │
│                           │ - capture.rs (audio)        │ │
│                           │ - transcribe.rs (whisper)   │ │
│                           │ - diarize.rs (pyannote)     │ │
│                           │ - pipeline.rs (orchestrator)│ │
│                           │ - vault.rs (Obsidian sync)  │ │
│                           └─────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
         │
         ▼ vault sync (copy strategy)
┌─────────────────────────────────────────────────────────┐
│ Obsidian Vault (a-life)                                 │
│ ~/Documents/Obsidian/a-life/01-Inbox/                   │
│ → Processed via /process-meeting Claude Code skill      │
└─────────────────────────────────────────────────────────┘
```

## What Changed From Upstream

| Component | Upstream | This Fork |
|-----------|----------|-----------|
| Frontend | Vanilla HTML/CSS/JS in `tauri/src/` | React 19 + TypeScript + Vite in `tauri/src/` |
| Styling | Inline CSS, CSS variables | Inline React styles (no Tailwind in prod build due to Tauri embedding limitations) |
| Build output | Raw HTML served directly | Single-file HTML via vite-plugin-singlefile (all JS/CSS inlined) |
| Icons | Original icons | Custom minimal waveform icons (tray: 44x44, app: 512x512) |
| Vault target | Configurable | Hardcoded to `01-Inbox/` in Obsidian vault |
| Summarization | Optional Claude/Ollama/OpenAI API | Disabled. Done via Claude Code sessions instead. |
| Old frontend | N/A | Preserved at `tauri/src-legacy/` for reference |

**All Rust crates are unchanged.** The backend, CLI, MCP server, and core engine are identical to upstream.

## Project Structure

```
minutes/                           # Repository root
├── CLAUDE.md                      # This file
├── Cargo.toml                     # Rust workspace root
├── crates/
│   ├── core/src/                  # 26 Rust modules — the engine (UNCHANGED)
│   │   ├── capture.rs             # Audio capture via cpal
│   │   ├── transcribe.rs          # Whisper.cpp transcription
│   │   ├── diarize.rs             # Speaker diarization (pyannote-rs)
│   │   ├── pipeline.rs            # Full audio → markdown flow
│   │   ├── vault.rs               # Obsidian/Logseq vault sync
│   │   ├── summarize.rs           # LLM summarization (disabled in our config)
│   │   ├── search.rs              # Full-text search across meetings
│   │   ├── config.rs              # TOML config with defaults
│   │   ├── voice.rs               # Voice profile matching
│   │   └── ... (26 modules total)
│   ├── cli/                       # CLI binary (`minutes` command)
│   ├── mcp/                       # MCP server (Claude Desktop/Cursor integration)
│   ├── reader/                    # Read-only meeting parser
│   ├── sdk/                       # TypeScript SDK
│   ├── whisper-guard/             # Anti-hallucination toolkit
│   └── assets/                    # Bundled assets (demo.wav)
├── tauri/
│   ├── src/                       # ★ CUSTOM REACT FRONTEND
│   │   ├── index.html             # Entry point (Vite transforms this)
│   │   ├── vite.config.ts         # Vite + React + single-file build
│   │   ├── package.json           # npm deps (React, Tauri API, Lucide, Radix)
│   │   ├── tsconfig.json          # TypeScript config
│   │   ├── src/
│   │   │   ├── main.tsx           # React entry point
│   │   │   ├── App.tsx            # ★ Main app component (all views)
│   │   │   ├── lib/
│   │   │   │   └── tauri.ts       # Typed IPC wrappers for Rust commands
│   │   │   ├── components/        # Component files (currently unused — App.tsx has inline components)
│   │   │   └── index.css          # Design system tokens (Tailwind, used in dev mode)
│   │   └── dist/                  # Build output (single index.html with inlined JS/CSS)
│   ├── src-legacy/                # Original vanilla frontend (preserved for reference)
│   │   ├── index.html             # Original main window
│   │   ├── note.html              # Original quick note popup
│   │   ├── terminal.html          # Original terminal view
│   │   └── ...
│   └── src-tauri/                 # Rust backend for Tauri (UNCHANGED)
│       ├── src/
│       │   ├── main.rs            # App init, tray menu, window management, hotkeys
│       │   ├── commands.rs        # 40+ IPC command handlers
│       │   ├── call_detect.rs     # Auto-detect Teams/Zoom/FaceTime calls
│       │   └── pty.rs             # Terminal session management
│       ├── tauri.conf.json        # Tauri config (frontendDist: ../src/dist)
│       ├── icons/                 # ★ Custom tray + app icons
│       │   ├── icon.png           # Tray: 44x44 waveform (template, auto light/dark)
│       │   ├── icon-recording.png # Tray: red waveform + red dot
│       │   ├── icon-live.png      # Tray: waveform + green dot
│       │   ├── app-icon.png       # App: 512x512 waveform on dark square
│       │   └── icon.icns          # macOS bundle icon
│       └── Cargo.toml             # Rust deps (tauri 2, minutes-core)
├── docs/                          # Documentation
├── plugin/                        # Claude Code plugin (upstream)
└── site/                          # Marketing website (upstream)
```

## Frontend Architecture

### Why Single-File Build

Tauri v2 embeds frontend files into the Rust binary. The embedded filesystem doesn't resolve relative `./assets/` paths for separate JS/CSS files. The solution: `vite-plugin-singlefile` inlines all JavaScript and CSS directly into `index.html`, producing a single self-contained file (~207KB) that Tauri embeds and serves correctly.

### Why Inline Styles (Not Tailwind)

Tailwind CSS 4 works perfectly in development (Vite dev server), but the compiled Tailwind classes caused issues in the single-file embedded build (class names referencing CSS custom properties that weren't resolving). The production App.tsx uses inline React `style` objects for reliability. The Tailwind design tokens remain in `index.css` for dev mode reference.

### Design System (reference, defined in index.css @theme)

| Token | Dark Value | Light Value | Usage |
|-------|-----------|-------------|-------|
| `bg` | `#1a1a1c` | `#fafafa` | App background |
| `elevated` | `#242426` | `#ffffff` | Cards, panels, banners |
| `hover` | `#2e2e30` | `#f0f0f2` | Hover states |
| `border` | `#333335` | `#e5e5e7` | Borders |
| `text` | `#ececee` | `#1a1a1c` | Primary text |
| `text-secondary` | `#8e8e93` | `#6e6e73` | Secondary text |
| `text-tertiary` | `#5c5c60` | `#aeaeb2` | Labels, timestamps |
| `accent-red` | `#ef4444` | same | Recording indicator |
| `accent-green` | `#34d399` | same | Success, status dot |
| `accent-blue` | `#60a5fa` | same | Info, processing |

Typography: System font stack (`-apple-system, BlinkMacSystemFont, system-ui, sans-serif`). Mono: `SF Mono, Menlo, monospace`.

Spacing: 8px base grid (4, 8, 12, 16, 24, 32px).

### IPC Interface (Frontend → Rust)

All communication uses `window.__TAURI__.core.invoke("cmd_name", args)`. Typed wrappers in `src/lib/tauri.ts`:

```typescript
// Key commands used by the React frontend
startRecording(mode?)          // Start audio capture
stopRecording()                // Stop and process
getStatus()                    // { recording, processing, processing_stage }
listMeetings()                 // Array of meeting metadata
getMeetingDetail(path)         // Full meeting data with sections
getSettings()                  // Config key-value pairs
setSetting(key, value)         // Update config
addNote(text)                  // Add note to current recording
vaultStatus()                  // { enabled, path, strategy }
getStorageStats()              // { total_mb, meetings, memos }
```

Events (Rust → Frontend):
```typescript
onRecordingStatus(cb)          // recording started/stopped + elapsed seconds
onProcessingStatus(cb)         // transcription progress
onLatestArtifact(cb)           // new transcript available
```

### Meeting Detail Response Shape

The `cmd_get_meeting_detail` command returns:

```json
{
  "path": "/Users/.../meeting.md",
  "title": "Meeting Title",
  "date": "2026-03-29T11:52:25.484430-04:00",
  "duration": "15s",
  "content_type": "meeting",
  "status": "transcript-only",
  "context": null,
  "attendees": [],
  "calendar_event": null,
  "sections": [
    { "heading": "Transcript", "content": "[0:00] Speaker text..." },
    { "heading": "Summary", "content": "..." },
    { "heading": "Action Items", "content": "..." }
  ],
  "speaker_map": []
}
```

Note: sections is an array of `{ heading, content }` pairs, NOT a flat body string.

## Build Commands

### Frontend Only

```bash
cd tauri/src
npm install                    # First time only
npm run build                  # TypeScript check + Vite build + strip crossorigin attrs
npm run dev                    # Start Vite dev server on port 1420
```

### Full App (Frontend + Rust + Bundle)

```bash
# Prerequisites: Rust toolchain, Tauri CLI
cargo install tauri-cli --version "^2"

# Build production .app
cd tauri/src && npm run build
cd tauri/src-tauri && cargo tauri build --bundles app

# Install to /Applications
rm -rf /Applications/Minutes.app
cp -R target/release/bundle/macos/Minutes.app /Applications/
xattr -cr /Applications/Minutes.app
open -a "Minutes"
```

### Development Mode

```bash
# Terminal 1: Start Vite dev server
cd tauri/src && npm run dev

# Terminal 2: Start Tauri dev build (connects to Vite)
cd tauri/src-tauri && cargo tauri dev
```

First Rust build takes ~3-5 minutes (579 crates). Subsequent builds take ~6-25 seconds.

### CLI Only (no UI changes)

```bash
cargo build --release -p minutes-cli
# Binary at target/release/minutes
```

## Configuration

Config file: `~/.config/minutes/config.toml`

### James's Configuration

```toml
output_dir = "/Users/jgaynor/meetings"

[transcription]
model = "medium"                    # Whisper medium (1.5 GB, good accuracy)

[diarization]
engine = "none"                     # Not configured yet (needs HuggingFace token)

[summarization]
engine = "none"                     # Disabled — summarization via Claude Code sessions

[call_detection]
enabled = true                      # Auto-detect calls every 3 seconds
apps = ["zoom.us", "Microsoft Teams", "FaceTime", "Webex", "Slack"]

[vault]
enabled = true
path = "/Users/jgaynor/Documents/Obsidian/a-life"
meetings_subdir = "01-Inbox"        # Transcripts land in Obsidian inbox
strategy = "copy"
```

### Key Config Decisions

- **Summarization disabled**: We use Claude Code's `/process-meeting` skill instead of API-based summarization. This keeps everything local and gives James control over the processing.
- **Vault sync to 01-Inbox**: Raw transcripts land in the Obsidian inbox, where they wait to be processed via the `process-meeting` skill. After processing, they migrate to domain-specific folders.
- **Call detection enabled**: Minutes detects Teams/Zoom calls and prompts to record.
- **Medium Whisper model**: Balance of accuracy and speed. Better with proper nouns than the small model.

## Obsidian Integration Pipeline

```
Meeting happens
     │
     ├── Click Record in Minutes menu bar
     │         │
     │         ▼
     │   whisper.cpp transcribes locally (medium model)
     │         │
     │         ▼
     │   Markdown with YAML frontmatter saved to ~/meetings/
     │         │
     │         ▼
     │   Vault sync copies to 01-Inbox/ in Obsidian vault
     │
     ▼
Run /process-meeting in Claude Code
     │
     ├── Scan inbox for meeting captures
     ├── Vocabulary correction (fix Whisper misspellings)
     ├── Verify participants, initiative, project (guided questions)
     ├── Create: transcript (central folder) + summary (project folder)
     ├── Create: task notes, person notes (CRM)
     ├── Update: journal, vault index
     └── Delete inbox capture
```

Full details in the Obsidian vault at `02-Projects/Meeting Knowledge Base Plan.md`.

## Data Flow

- **Audio**: Captured locally via `cpal`, saved as WAV
- **Transcription**: whisper.cpp (GPU-accelerated via Metal on Apple Silicon)
- **Output**: Markdown files in `~/meetings/` (meetings) and `~/meetings/memos/` (voice memos)
- **Vault sync**: Copy strategy duplicates markdown to Obsidian vault's `01-Inbox/`
- **Storage**: All local. Audio files, transcripts, SQLite index (`~/.minutes/`)
- **No cloud**: Summarization disabled. No API keys used. No telemetry.

## Background Services

- **Minutes.app**: Menu bar app, call detection, one-click recording
- **Watcher service**: `~/Library/LaunchAgents/dev.getminutes.watcher.plist` — auto-starts on login, processes audio files in `~/.minutes/inbox/`
- **CLI**: `minutes` command at `/opt/homebrew/bin/minutes` (symlinked from Cellar)

## Known Limitations (This Fork)

1. **Single-file build required**: Tauri's embedded filesystem doesn't resolve relative paths for separate JS/CSS files. Everything must be inlined into `index.html`.
2. **Inline styles in production**: Tailwind CSS custom properties don't resolve in the embedded webview. Production App.tsx uses inline `style` objects.
3. **No note.html or terminal.html**: The original app had separate HTML files for note popups and terminal views. Our React SPA handles notes via a modal in the main window. Terminal views are not yet ported.
4. **Diarization not configured**: Speaker identification via pyannote requires a HuggingFace token. Currently `engine = "none"`.
5. **Unsigned app**: No Apple Developer certificate. Requires `xattr -cr` after install. Gatekeeper bypass on first launch.

## Upstream Sync

To pull upstream changes (Rust backend improvements, new features):

```bash
git fetch upstream
git merge upstream/main
# Resolve conflicts in tauri/src/ (our React frontend vs their vanilla HTML)
# Rust crates should merge cleanly since we haven't modified them
```

The `tauri/src-legacy/` directory preserves the original frontend for reference during merges.

## Testing

### Frontend

```bash
cd tauri/src
npx tsc --noEmit              # Type check (no emit)
npm run build                  # Full build (type check + Vite + strip crossorigin)
```

### Rust Backend (unchanged from upstream)

```bash
cargo test -p minutes-core --no-default-features   # Fast (no whisper model)
cargo test -p minutes-core                          # Full (needs model)
cargo clippy --all --no-default-features -- -D warnings
cargo fmt --all -- --check
```

### Integration Test

```bash
minutes demo                   # Runs bundled audio through pipeline
minutes health                 # Check model, mic, vault, disk
minutes vault status           # Verify Obsidian sync is healthy
```

## File Locations

| What | Where |
|------|-------|
| App binary | `/Applications/Minutes.app` |
| CLI binary | `/opt/homebrew/bin/minutes` |
| Config | `~/.config/minutes/config.toml` |
| Whisper models | `~/.minutes/models/` (small: 466MB, medium: 1.5GB) |
| Meeting output | `~/meetings/` |
| Voice memos | `~/meetings/memos/` |
| SQLite index | `~/.minutes/` |
| Logs | `~/.minutes/logs/` |
| Watcher service | `~/Library/LaunchAgents/dev.getminutes.watcher.plist` |
| Vault sync target | `~/Documents/Obsidian/a-life/01-Inbox/` |
| Fork source | `~/Documents/Cursor_Projects/minutes/` |
