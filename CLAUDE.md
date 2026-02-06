# RLCollector

## What This Is
A cross-platform desktop app (Tauri v2 + React + Rust) that captures screenshots of user activity, uses AI vision (Claude API) to detect and annotate distinct tasks, and stores structured task data locally in SQLite. Future: marketplace for selling anonymized data.

## Architecture

```
Tauri App
├── src-tauri/          # Rust backend
│   ├── src/
│   │   ├── main.rs             # Tauri entry, app setup
│   │   ├── capture.rs          # Screen capture engine (xcap crate)
│   │   ├── storage.rs          # SQLite operations (rusqlite)
│   │   ├── ai.rs               # Claude API vision calls
│   │   ├── tray.rs             # System tray setup
│   │   ├── commands.rs         # Tauri IPC commands exposed to frontend
│   │   └── models.rs           # Shared data structures
│   ├── migrations/             # SQL migrations
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                # React frontend
│   ├── App.tsx
│   ├── components/
│   │   ├── Dashboard.tsx       # Main view: task list, stats
│   │   ├── TaskDetail.tsx      # View/edit a single task
│   │   ├── CaptureControls.tsx # Start/stop/pause, interval config
│   │   ├── Settings.tsx        # API keys, capture prefs, storage
│   │   └── Popup.tsx           # Clarification popup window
│   ├── hooks/
│   │   ├── useCapture.ts       # Capture state management
│   │   └── useTasks.ts         # Task CRUD via Tauri commands
│   ├── lib/
│   │   └── tauri.ts            # Typed wrappers around invoke()
│   └── types.ts                # Shared TypeScript types
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## Data Model (SQLite)

```sql
-- Screenshots stored as files on disk, metadata in DB
CREATE TABLE screenshots (
    id INTEGER PRIMARY KEY,
    filepath TEXT NOT NULL,          -- relative path to image file
    captured_at TEXT NOT NULL,       -- ISO 8601 timestamp
    active_window_title TEXT,
    monitor_index INTEGER DEFAULT 0
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    category TEXT,                   -- e.g. "coding", "browsing", "writing"
    started_at TEXT NOT NULL,
    ended_at TEXT,
    ai_reasoning TEXT,               -- raw AI explanation of why this is a task
    user_verified INTEGER DEFAULT 0, -- user confirmed/edited this task
    metadata TEXT                    -- JSON blob for extensibility
);

CREATE TABLE task_screenshots (
    task_id INTEGER REFERENCES tasks(id),
    screenshot_id INTEGER REFERENCES screenshots(id),
    PRIMARY KEY (task_id, screenshot_id)
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

## IPC Commands (Rust → Frontend)

- `start_capture(interval_ms: u64)` — begin screen capture loop
- `stop_capture()` — stop capture
- `get_capture_status()` → `{ active: bool, interval_ms: u64, count: u64 }`
- `get_tasks(limit, offset)` → `Vec<Task>`
- `get_task(id)` → `Task` with screenshots
- `update_task(id, fields)` — user edits
- `delete_task(id)`
- `analyze_pending()` — trigger AI analysis on unprocessed screenshots
- `get_settings()` / `update_settings(key, value)`

## Key Crates
- `tauri` v2 — app framework
- `xcap` — cross-platform screen capture
- `rusqlite` with `bundled` feature — SQLite
- `reqwest` — HTTP client for Claude API
- `serde` / `serde_json` — serialization
- `tokio` — async runtime (comes with Tauri)
- `base64` — encoding screenshots for API
- `image` — image processing/compression

## Build & Run
```bash
npm install          # install frontend deps
npm run tauri dev    # dev mode with hot reload
npm run tauri build  # production build
```

## Testing Strategy

### Rust (backend)
- Unit tests in each module (`#[cfg(test)]` blocks)
- `capture.rs`: test that capture returns valid image bytes (can mock on CI)
- `storage.rs`: test all CRUD against an in-memory SQLite (`:memory:`)
- `ai.rs`: test request construction, mock HTTP responses
- `commands.rs`: integration tests via tauri-test
- Run: `cd src-tauri && cargo test`

### React (frontend)
- Vitest + React Testing Library
- Component tests for Dashboard, TaskDetail, CaptureControls
- Mock Tauri `invoke()` calls via `@tauri-apps/api` mocks
- Run: `npm test`

### E2E (later)
- Tauri's WebDriver support or Playwright for full app testing

## Best Practices

### Rust
- All public functions return `Result<T, E>` — use `thiserror` for custom errors
- No `unwrap()` in production code — use `?` operator or explicit error handling
- Keep Tauri commands thin: delegate to module functions for testability
- Use `#[tauri::command]` with serde types for automatic serialization

### React/TypeScript
- Strict TypeScript (`strict: true`) — no `any` types
- All Tauri invoke calls go through typed wrappers in `lib/tauri.ts`
- State: React context or zustand if state gets complex — no prop drilling past 2 levels
- Components are functional with hooks only

### SQLite
- All schema changes via numbered migration files
- Use parameterized queries — never string-interpolate SQL
- WAL mode enabled for concurrent read/write
- Foreign keys enforced (`PRAGMA foreign_keys = ON`)

### General
- Screenshots stored in app data dir, not in DB (keeps DB fast)
- Compress screenshots to WebP before saving (smaller, fast)
- Configurable capture interval: 1s minimum, 5min maximum, default 30s
- AI calls are batched and async — never block the capture loop
