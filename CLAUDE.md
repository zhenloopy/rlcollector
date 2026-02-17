# RLCollector

## What This Is
A cross-platform desktop app (Tauri v2 + React + Rust) that captures screenshots of user activity, uses AI vision (Claude or Ollama) to detect and annotate distinct tasks, and stores structured task data locally in SQLite. Future: marketplace for selling anonymized data.

## Architecture

```
Tauri App
├── src-tauri/          # Rust backend
│   ├── src/
│   │   ├── main.rs             # Entry point → calls lib::run()
│   │   ├── lib.rs              # App setup: plugins, state, command registration
│   │   ├── capture.rs          # Screen capture, image processing, perceptual hashing
│   │   ├── storage.rs          # SQLite CRUD (rusqlite, in-memory for tests)
│   │   ├── ai.rs               # Claude + Ollama vision API integration
│   │   ├── tray.rs             # System tray menu
│   │   ├── commands.rs         # Tauri IPC commands + capture/analysis loops
│   │   ├── models.rs           # Shared data structures (serde-serializable)
│   │   └── ollama_sidecar.rs   # Bundled Ollama process management
│   ├── Cargo.toml
│   └── tauri.conf.json         # App ID: com.rlmarket.rlcollector
├── src/                # React frontend
│   ├── App.tsx                 # Root: tab nav (sessions/settings), CaptureControls
│   ├── components/
│   │   ├── Dashboard.tsx       # Session list (pending/completed), analysis controls
│   │   ├── CollectionDetail.tsx # Screenshot gallery + task viewer for a session
│   │   ├── CaptureControls.tsx # Start/stop capture, session title/description, interval
│   │   └── Settings.tsx        # AI provider, monitor mode, image mode, analysis mode
│   ├── hooks/
│   │   ├── useCapture.ts       # Capture state polling (2s interval)
│   │   └── useSessions.ts      # Session list + analysis status polling (3s interval)
│   ├── lib/
│   │   └── tauri.ts            # Typed wrappers around invoke() — all IPC goes through here
│   └── types.ts                # TypeScript interfaces matching Rust models
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## Data Flow

### Capture → Save → Analyze → Display

```
1. CaptureControls "Start" → useCapture.start() → invoke("start_capture")
2. commands.rs: create session in DB, spawn async capture loop
3. Loop (every interval_ms):
   a. capture::capture_monitors(mode) → Vec<CapturedMonitor> (in-memory images)
   b. Per monitor: perceptual_hash() → compare to last hash (threshold=10 bits)
   c. Changed monitors: save WebP to disk, insert screenshot row, update monitor_states
   d. If auto-analysis enabled: spawn analyze_screenshots() in background
4. CaptureControls "Stop" → invoke("stop_capture") → end session, trigger final analysis
5. Dashboard shows pending sessions → user clicks "Analyze" → invoke("analyze_session")
6. analyze_screenshots():
   a. Group screenshots by capture_group (multi-monitor grouping)
   b. Per group: build changed monitors (images) + unchanged (text summaries)
   c. Call AI (Claude or Ollama) → get TaskAnalysis JSON
   d. If is_new_task: insert task + link screenshots; else: link to existing task
   e. Update monitor_states with returned monitor_summaries
7. Completed session → user clicks → CollectionDetail shows screenshot grid + tasks
```

## Data Model (SQLite)

```sql
CREATE TABLE capture_sessions (
    id INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL,        -- ISO 8601
    ended_at TEXT,
    description TEXT,
    title TEXT
);

CREATE TABLE screenshots (
    id INTEGER PRIMARY KEY,
    filepath TEXT NOT NULL,          -- relative path to WebP file
    captured_at TEXT NOT NULL,       -- ISO 8601
    active_window_title TEXT,
    monitor_index INTEGER DEFAULT 0, -- xcap monitor ID
    session_id INTEGER REFERENCES capture_sessions(id),
    capture_group TEXT               -- groups multi-monitor screenshots from same tick
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    category TEXT,                   -- coding, browsing, writing, communication, design, other
    started_at TEXT NOT NULL,
    ended_at TEXT,
    ai_reasoning TEXT,
    user_verified INTEGER DEFAULT 0,
    metadata TEXT                    -- JSON blob
);

CREATE TABLE task_screenshots (
    task_id INTEGER REFERENCES tasks(id),
    screenshot_id INTEGER REFERENCES screenshots(id),
    PRIMARY KEY (task_id, screenshot_id)
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

## IPC Commands (22 total, registered in lib.rs)

### Capture
- `start_capture(interval_ms?, description?, title?)` — create session, start capture loop
- `stop_capture()` — end session, trigger post-capture analysis
- `get_capture_status()` → `CaptureStatus { active, interval_ms, count, monitor_mode, monitors_captured }`
- `get_current_session()` → `Option<CaptureSession>`
- `get_monitors()` → `Vec<MonitorInfo>`

### Sessions
- `get_sessions(limit?, offset?)` — all sessions
- `get_pending_sessions(limit?, offset?)` — ended sessions with unanalyzed screenshots
- `get_completed_sessions(limit?, offset?)` — fully analyzed sessions
- `get_session_screenshots(session_id)` → `Vec<Screenshot>`
- `get_session_tasks(session_id)` → `Vec<Task>`
- `delete_session(session_id)` — deletes session, tasks, screenshots + files

### Tasks
- `get_tasks(limit?, offset?)`, `get_task(id)`, `update_task(id, update)`, `delete_task(id)`
- `get_task_for_screenshot(screenshot_id)` → `Option<Task>`

### Analysis
- `analyze_session(session_id)` — analyze one session
- `analyze_all_pending()` — analyze all pending sessions
- `analyze_pending()` — analyze global unanalyzed pool
- `get_analysis_status()` → `AnalysisStatus { analyzing, session_id }`
- `cancel_analysis()` — sets cancel flag
- `clear_pending()` — deletes unanalyzed screenshots + files

### Settings & Misc
- `get_setting(key)`, `update_setting(key, value)`
- `get_log_path()`, `get_screenshots_dir()`
- `check_ollama()`, `ensure_ollama()`, `ollama_pull(model)`

## Settings Keys
| Key | Values | Default | Description |
|-----|--------|---------|-------------|
| `ai_provider` | `claude`, `ollama` | `claude` | Which AI backend to use |
| `ai_api_key` | string | — | Claude API key |
| `ollama_model` | string | `qwen3-vl:8b` | Ollama model name |
| `capture_monitor_mode` | `default`, `specific`, `active`, `all` | `default` | Monitor capture strategy |
| `capture_monitor_id` | u32 | — | Monitor ID for "specific" mode |
| `image_mode` | `downscale`, `active_window` | `downscale` | Image preprocessing before AI |
| `analysis_mode` | `realtime`, `batch` | `realtime` | When to trigger auto-analysis |
| `batch_size` | 1–100 | 5 | Screenshots per batch (if batch mode) |

## Key Rust Modules

### capture.rs — Screen Capture & Change Detection
- `list_monitors()` → `Vec<MonitorInfo>` — wraps xcap `Monitor::all()`
- `capture_monitors(mode, specific_id)` → `Vec<CapturedMonitor>` — returns in-memory `RgbaImage`s
- `get_cursor_position()` → `(i32, i32)` — platform-specific (windows-sys / CoreGraphics / xdotool)
- `save_image_as_webp()`, `encode_webp_bytes()`, `resize_for_analysis(max_width=1280)`
- `perceptual_hash(image)` → `[u8; 32]` — 16x16 grayscale, mean-threshold, 256-bit hash
- `hash_distance(a, b)` → `u32` — XOR + popcount; threshold=10 means "changed"

### ai.rs — AI Vision Analysis
- `analyze_capture(client, api_key, changed, unchanged, contexts, ...)` — Claude API
- `analyze_capture_ollama(client, model, changed, unchanged, contexts, ...)` — Ollama API
- `preprocess_and_encode(path, mode)` — resize/crop → WebP base64
- `build_prompt()` / `build_multi_prompt()` — constructs prompts with context
- Returns `TaskAnalysis { task_title, task_description, category, reasoning, is_new_task, monitor_summaries }`
- Claude model: `claude-sonnet-4-5-20250929`, max_tokens: 1024
- Ollama: temp=0.3, num_predict=512, num_ctx=8192, retry on empty response

### commands.rs — IPC + Orchestration
- `AppState`: db, atomic flags (capturing, analyzing, cancel), monitor_states, ollama_process
- `MonitorState`: last_hash, last_summary, last_screenshot_id, name — per-monitor tracking
- Capture loop: async task reading settings each tick, capture → hash → save → auto-analyze
- `analyze_screenshots()`: groups by capture_group, builds changed/unchanged lists, calls AI, creates/links tasks
- `group_by_capture_group()`: BTreeMap-based grouping, NULL groups treated individually

### storage.rs — SQLite Layer
- `Database` wraps `Mutex<Connection>`, WAL mode, foreign keys ON
- Schema migrations run on init (ALTER TABLE for capture_group column)
- All CRUD for sessions, screenshots, tasks, settings
- `get_pending_sessions()` / `get_completed_sessions()` use subqueries on unanalyzed count

### ollama_sidecar.rs — Bundled Ollama
- `find_binary(app_data_dir)` — checks `{app_data_dir}/ollama` then system PATH
- `start(binary_path)` — spawns `ollama serve` with `OLLAMA_HOST=127.0.0.1:11434`
- `wait_for_ready()` — polls `/api/tags` with 500ms backoff
- Auto-stopped on app exit (Drop impl + Run exit event)

## Frontend Components

### App.tsx — Root
- Tab navigation: sessions / settings
- Always shows `CaptureControls` at top
- `sessionVersion` counter triggers Dashboard refresh on capture stop

### CaptureControls.tsx — Capture UI
- Session title (required) + description inputs
- Interval slider (1–300s)
- Start/Stop button; inputs disabled while capturing

### Dashboard.tsx — Session Management
- **Pending tab**: sessions with unanalyzed screenshots, "Analyze" / "Analyze All" / "Cancel" buttons
- **Completed tab**: paginated (20/page), click to open `CollectionDetail`
- Uses `useSessions` hook (3s polling for analysis status)

### CollectionDetail.tsx — Session Viewer
- Screenshot thumbnail grid
- Click → modal with full image + linked task info
- Arrow key navigation, Escape to close
- Uses `convertFileSrc` for Tauri asset protocol URLs

### Settings.tsx — Configuration
- AI provider radio (Claude/Ollama) with provider-specific config
- Ollama: ensure/pull/status, model dropdown
- Image mode, analysis mode, batch size
- Monitor mode selector with specific-monitor dropdown
- "Open Logs" button, save button

## Key Crates
- `tauri` v2 (tray-icon, protocol-asset) — app framework
- `xcap` v0.0.14 — cross-platform screen capture (pinned, newer versions break)
- `rusqlite` v0.31 (bundled) — SQLite
- `reqwest` v0.12 — HTTP client for Claude/Ollama APIs
- `image` v0.25 — image processing, WebP encoding
- `windows-sys` v0.59 — Windows cursor position (active monitor mode)
- `tauri-plugin-log` — file + stdout logging
- `dirs-next` — platform-specific app data dirs

## Build & Run
```bash
source env.sh        # REQUIRED — adds cargo to PATH
npm install          # install frontend deps
npm run tauri dev    # dev mode with hot reload
npm run tauri build  # production build
```

**Important (Claude):** Always run `source env.sh` before any `cargo` or `npm run tauri` commands. Always build after code changes.

## Testing
- **Rust**: `cd src-tauri && cargo test` (56 tests)
- **Frontend**: `npx vitest run` (51 tests)
- Storage tests use `:memory:` SQLite; AI tests verify serialization; capture tests verify hashing

## File Locations

### Executables (after `npm run tauri build`)
- **Windows**: `src-tauri/target/release/rlcollector.exe`
- **macOS**: `src-tauri/target/release/bundle/macos/RLCollector.app`
- **Linux**: `src-tauri/target/release/rlcollector`

### Logs (`tauri-plugin-log`)
- **Windows**: `%LOCALAPPDATA%\com.rlmarket.rlcollector\logs\`
- **macOS**: `~/Library/Logs/com.rlmarket.rlcollector/`
- **Linux**: `~/.config/com.rlmarket.rlcollector/logs/`

### App Data (screenshots + SQLite DB)
- **Windows**: `%APPDATA%\rlcollector\`
- **macOS**: `~/Library/Application Support/rlcollector/`
- **Linux**: `~/.local/share/rlcollector/`

## Multi-Monitor Capture

Four modes via `capture_monitor_mode` setting:
- **default**: Primary monitor only
- **specific**: User-chosen monitor via `capture_monitor_id`
- **active**: Monitor where cursor is located (platform-specific API, falls back to primary)
- **all**: Every connected monitor

Key architecture:
- `capture_monitors()` returns in-memory images; caller decides what to save after hashing
- `MonitorState` in commands.rs tracks per-monitor: last_hash, last_summary, last_screenshot_id
- `capture_group` column groups screenshots from same tick for multi-monitor analysis
- AI receives changed monitors as images + unchanged monitors as text summaries
- `monitor_summaries` in `TaskAnalysis` carries per-monitor descriptions between ticks

## Gotchas
- **Tauri v2 sync commands don't run on Tokio** — use `tauri::async_runtime::spawn` not `tokio::spawn`
- **tauri-plugin-log writes to `%LOCALAPPDATA%`** not `%APPDATA%` — use `app_handle.path().app_log_dir()`
- **`catch_unwind` on async** — wrapping future creation doesn't catch execution panics; use JoinHandle `.await` error
- xcap v0.0.14 is pinned (newer versions have different API)
- Cargo.toml lib name is `rlcollector_lib`, referenced in main.rs
- Timestamps stored as ISO 8601 strings (not chrono) for SQLite TEXT compatibility

## Note to Claude
After any major functionality or architecture change, update this file. Keep it simple — focus on: how the app is built/deployed, how it's tested, and where major features live. Don't let these docs go stale.
