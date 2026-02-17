# Multi-Monitor Capture Implementation Plan

## Overview

Add four capture modes: **Default** (primary monitor), **Specific** (user-chosen monitor), **Active** (monitor with cursor), and **All** (every monitor). For all modes, the AI output is a single `TaskAnalysis` per capture tick. Multi-monitor mode uses per-monitor change tracking and text summaries to keep API costs close to single-monitor.

---

## Phase 1: Monitor Enumeration & Selection Infrastructure

### 1a. New setting: `capture_monitor_mode`

- Stored in the `settings` table. Values: `"default"`, `"specific"`, `"active"`, `"all"`.
- Additional setting `capture_monitor_id` (string) — the monitor name/id for "specific" mode.
- Default value: `"default"` (preserves current behavior).

### 1b. Monitor listing command

Add a new Tauri command `get_monitors() -> Vec<MonitorInfo>`:

```rust
// models.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}
```

Implementation in `capture.rs`:
```rust
pub fn list_monitors() -> Result<Vec<MonitorInfo>, CaptureError> {
    let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    Ok(monitors.iter().map(|m| MonitorInfo {
        id: m.id(), name: m.name().to_string(),
        x: m.x(), y: m.y(), width: m.width(), height: m.height(),
        is_primary: m.is_primary(),
    }).collect())
}
```

### 1c. Cursor position (for "active" mode)

Add a `get_cursor_position() -> (i32, i32)` function in `capture.rs` behind `#[cfg(target_os = ...)]`:

- **Windows**: `windows::Win32::UI::WindowsAndMessaging::GetCursorPos`. The `windows` crate is already an indirect dependency via xcap. Add it as a direct dependency with feature `Win32_UI_WindowsAndMessaging`.
- **macOS**: `core_graphics::event::CGEvent::mouseLocation()` — `core-graphics` is already a transitive dependency.
- **Linux**: `xcb` query pointer (already a transitive dep), or shell out to `xdotool getmouselocation`. The xcb approach is cleaner since xcap already depends on it.

Fallback: if cursor position fails, fall back to primary monitor.

---

## Phase 2: Multi-Monitor Capture

### 2a. Refactor `capture_screen` → `capture_monitors`

Current `capture_screen()` returns a single `PathBuf`. Replace with:

```rust
pub struct CapturedMonitor {
    pub monitor_id: u32,
    pub monitor_name: String,
    pub filepath: PathBuf,
    pub image: RgbaImage,  // kept in memory for hashing/diffing
}

/// Capture based on the configured mode.
pub fn capture_monitors(
    output_dir: &Path,
    timestamp: &str,
    mode: &str,              // "default", "specific", "active", "all"
    specific_id: Option<u32>, // for "specific" mode
) -> Result<Vec<CapturedMonitor>, CaptureError>
```

Logic:
- `"default"`: find monitor where `is_primary() == true` (not just `.first()` — fixes a latent bug). Return 1 item.
- `"specific"`: find monitor matching `specific_id`. Error if not found. Return 1 item.
- `"active"`: call `get_cursor_position()`, then `Monitor::from_point(x, y)`. Fallback to primary. Return 1 item.
- `"all"`: iterate `Monitor::all()`, capture each. Return N items.

File naming: `screenshot_{timestamp}_mon{id}.webp` (e.g. `screenshot_2025-01-01T10-00-00_mon65537.webp`). For single-monitor modes, keep the current naming: `screenshot_{timestamp}.webp`.

### 2b. DB changes

The `screenshots` table already has `monitor_index INTEGER`. We'll use it properly:
- Store the actual `monitor.id()` (u32) instead of always 0.
- Add a new column `capture_group TEXT` — a shared identifier (the timestamp string) that groups screenshots taken at the same tick. This lets us query "all screenshots from the same capture moment."

Migration in `storage.rs`:
```sql
ALTER TABLE screenshots ADD COLUMN capture_group TEXT;
```

New storage methods:
- `insert_screenshot()` — already takes `monitor: i32`, just pass real values now.
- `get_capture_group(capture_group: &str) -> Vec<Screenshot>` — get all screenshots from one tick.

### 2c. Update capture loop in `commands.rs`

The capture loop currently calls `capture::capture_screen()` and inserts one row. Change to:

```rust
let mode = app_state.db.get_setting("capture_monitor_mode")
    .unwrap_or(None).unwrap_or_else(|| "default".to_string());
let specific_id = app_state.db.get_setting("capture_monitor_id")
    .unwrap_or(None).and_then(|v| v.parse().ok());

let captures = capture::capture_monitors(
    &app_state.screenshots_dir, &filename_ts, &mode, specific_id
)?;

for cap in &captures {
    let relative_path = format!("screenshots/{}", cap.filepath.file_name()...);
    app_state.db.insert_screenshot(
        &relative_path, &db_timestamp, None,
        cap.monitor_id as i32, session_opt, Some(&capture_group),
    )?;
}
```

---

## Phase 3: Change Detection (Cost Reduction)

### 3a. Image hashing

Add a fast perceptual hash to detect whether a monitor's content has changed. We don't need anything fancy — a simple approach:

1. Downscale the image to 16x16 grayscale.
2. Compute a 256-bit hash (each pixel above/below mean = 1/0 bit).
3. Compare to the previous hash for that monitor using hamming distance.
4. Threshold: if hamming distance < 10 (out of 256), consider unchanged.

This is ~0ms per image and avoids sending duplicate images to the AI.

```rust
// capture.rs
pub fn perceptual_hash(image: &RgbaImage) -> [u8; 32] { ... }
pub fn hash_distance(a: &[u8; 32], b: &[u8; 32]) -> u32 { ... }
```

### 3b. Runtime state for per-monitor tracking

Add to `AppState`:

```rust
pub struct MonitorState {
    pub last_hash: [u8; 32],
    pub last_summary: String,      // AI's text description from previous tick
    pub last_screenshot_id: i64,   // DB id of last screenshot for this monitor
}

// In AppState:
pub monitor_states: Mutex<HashMap<u32, MonitorState>>,
```

On each capture tick, for each monitor:
1. Compute hash of new image.
2. Compare to `monitor_states[monitor_id].last_hash`.
3. If changed: mark as "changed", save new image to disk, insert into DB.
4. If unchanged: skip saving the image (don't waste disk space on duplicate screenshots). Reuse the previous screenshot's DB entry when linking to tasks.

### 3c. What gets sent to AI

For **single-monitor modes** (default, specific, active):
- Exactly one image per API call. No change to the current prompt.
- `monitor_states` still updated so switching to "all" mode mid-session has context.

For **all monitors** mode:
- **Changed monitors**: sent as images (base64).
- **Unchanged monitors**: sent as text descriptions from the previous analysis.
- If **no monitors changed**: skip the AI call entirely for this tick.
- If **all monitors changed**: send all images (first tick always does this).

---

## Phase 4: Prompt Design

### 4a. Unified prompt builder

Refactor `build_prompt()` to handle both single and multi-monitor:

```rust
fn build_prompt(
    previous_contexts: &[String],
    session_description: Option<&str>,
    monitor_context: MonitorContext,  // NEW
) -> String
```

Where `MonitorContext` is:
```rust
enum MonitorContext {
    Single,  // current behavior, no extra labeling needed
    Multi {
        changed_monitors: Vec<(u32, String)>,    // (id, name) — images attached
        unchanged_monitors: Vec<(u32, String, String)>,  // (id, name, summary)
        total_monitors: usize,
    },
}
```

### 4b. Multi-monitor prompt template

```
You are analyzing a multi-monitor desktop capture taken at a single moment.
The user has {N} monitors.

MONITORS WITH NEW SCREENSHOTS (images attached in order):
{for each changed monitor:}
- Monitor "{name}" ({width}x{height}{, primary if applicable}): see image {i}
{end for}

UNCHANGED MONITORS (text summary from last capture):
{for each unchanged monitor:}
- Monitor "{name}": {previous_summary}
{end for}

{session_description context if present}
{recent task history context}

Analyze what the user is doing across all monitors. Focus on the changed
monitor(s) — a change on any monitor may indicate a task switch.

Respond with JSON only:
{
  "task_title": "short title",
  "task_description": "what they're doing",
  "category": "coding|browsing|writing|communication|design|other",
  "reasoning": "why you think this",
  "is_new_task": true/false,
  "monitor_summaries": {
    "{monitor_name}": "1-sentence description of what this monitor shows"
  }
}
```

### 4c. Updated `TaskAnalysis` struct

```rust
pub struct TaskAnalysis {
    pub task_title: String,
    pub task_description: String,
    pub category: String,
    pub reasoning: String,
    pub is_new_task: bool,
    #[serde(default)]
    pub monitor_summaries: HashMap<String, String>,  // empty for single-monitor
}
```

The `monitor_summaries` field is optional/defaulted — single-monitor mode doesn't return it, so existing behavior is preserved. When present, the summaries are stored in `AppState.monitor_states` for the next tick.

### 4d. Single-monitor prompt (unchanged)

The existing `build_prompt()` logic stays the same for single-monitor modes. The only difference: after analysis, if the response happens to include `monitor_summaries`, we store them. This means the single-monitor path is a zero-change fallback.

---

## Phase 5: Analysis Pipeline Changes

### 5a. Refactor `analyze_screenshot` → `analyze_capture`

```rust
pub async fn analyze_capture(
    client: &Client,
    api_key: &str,
    changed_images: &[(u32, &Path, &str)],  // (monitor_id, path, monitor_name)
    unchanged_summaries: &[(u32, &str, &str)], // (monitor_id, name, summary)
    previous_contexts: &[String],
    session_description: Option<&str>,
    image_mode: &str,
) -> Result<TaskAnalysis, AiError>
```

For the Claude API call:
- Build `content: Vec<Content>` with one `Content::Image` per changed monitor, then one `Content::Text` with the prompt (which includes unchanged monitor summaries).
- Images are labeled in the prompt text by their order ("image 1", "image 2", ...).

For Ollama:
- `images: Vec<String>` gets all changed monitor images.
- The prompt text is the same multi-monitor template.

For single-monitor calls (default/specific/active modes), this simplifies to `changed_images.len() == 1` and `unchanged_summaries` is empty, producing an output identical to today.

### 5b. Update `analyze_screenshots` in `commands.rs`

Currently iterates screenshots one-by-one. Change to:

1. Group screenshots by `capture_group`.
2. For each group, determine which monitors changed (via `monitor_states`).
3. Call `analyze_capture()` with the changed images + unchanged summaries.
4. Store returned `monitor_summaries` back into `AppState.monitor_states`.
5. Link **all** screenshots in the group to the resulting task.

### 5c. Output quality parity

To ensure multi-monitor analysis quality matches single-monitor:

- **Always send at least one image**. If change detection says nothing changed but we're in "all" mode, send the primary monitor's image anyway (the model needs visual grounding).
- **Keep `monitor_summaries` short** — the prompt asks for 1-sentence descriptions. This prevents context bloat.
- **Image preprocessing is per-monitor** — each changed image goes through the same `preprocess_and_encode` (resize to 1280px width, WebP). This matches what single-monitor does today.
- **`is_new_task` semantics are identical** — the model still returns a single bool. The task represents the user's overall activity, not per-monitor activity.
- **Limit total images per call to 4** — if a user has 6 monitors and all changed, send the top 4 by change magnitude (largest hamming distance). Summarize the other 2 as text from the current tick (capture the image, generate a quick description, but don't send the image). This keeps token usage bounded.

---

## Phase 6: Frontend Changes

### 6a. Settings page

Add a "Monitor" section to Settings:
- Dropdown: "Default (primary)", "Specific monitor", "Active monitor (follows cursor)", "All monitors"
- When "Specific" is selected, show a secondary dropdown populated by `get_monitors()` listing each monitor by name and resolution.
- "Refresh monitors" button (calls `get_monitors()` again).

### 6b. Capture status

Update `CaptureStatus` to include `monitor_mode: String` and `monitors_captured: u32` so the UI can show "Capturing 3 monitors" or "Capturing: \\.\DISPLAY1".

### 6c. Screenshot viewer

When viewing a capture session's screenshots, group by `capture_group` timestamp. For "all" mode, show grouped screenshots side-by-side or in a tab layout per monitor, rather than a flat list.

---

## Phase 7: Testing

### 7a. Rust unit tests

- `capture.rs`: test `list_monitors()` returns non-empty (on machines with displays), test `perceptual_hash` produces consistent results, test `hash_distance` math.
- `capture.rs`: test `capture_monitors` with mode="default" returns 1 item.
- `storage.rs`: test `capture_group` insertion and query.
- `ai.rs`: test multi-monitor prompt construction (changed + unchanged monitors). Test that `TaskAnalysis` deserializes with and without `monitor_summaries`.
- `commands.rs`: test that capture loop groups screenshots correctly.

### 7b. Frontend tests

- Settings component: test monitor mode dropdown renders, specific monitor selector appears conditionally.
- Dashboard: test grouped screenshot display.

---

## File Change Summary

| File | Changes |
|------|---------|
| `src-tauri/src/models.rs` | Add `MonitorInfo`, update `CaptureStatus`, add `monitor_summaries` to frontend types |
| `src-tauri/src/capture.rs` | Add `list_monitors()`, `capture_monitors()`, `get_cursor_position()`, `perceptual_hash()`, `hash_distance()`, `CapturedMonitor` struct |
| `src-tauri/src/ai.rs` | Refactor to `analyze_capture()`, update prompt builder for multi-monitor, add `monitor_summaries` to `TaskAnalysis` |
| `src-tauri/src/commands.rs` | Add `MonitorState` to `AppState`, update capture loop, add `get_monitors` command, update analysis pipeline |
| `src-tauri/src/storage.rs` | Add `capture_group` migration + column, add `get_capture_group()` method |
| `src-tauri/src/lib.rs` | Register new commands (`get_monitors`) |
| `src-tauri/Cargo.toml` | Add `windows` crate feature for `GetCursorPos` (Windows-only dep) |
| `src/components/Settings.tsx` | Monitor mode dropdown, specific monitor picker |
| `src/components/Dashboard.tsx` | Grouped screenshot display |
| `src/lib/tauri.ts` | Add `getMonitors()`, update types |
| `src/types.ts` | Add `MonitorInfo`, update `CaptureStatus` |

## Implementation Order

1. Phase 1 (monitor enumeration) — foundation, no behavior change
2. Phase 2 (multi-capture + DB) — capture works, analysis still single-image
3. Phase 3 (change detection) — hashing infra, `monitor_states`
4. Phase 4 + 5 (prompt + analysis pipeline) — multi-monitor AI integration
5. Phase 6 (frontend) — can be partially parallelized with phases 3-5
6. Phase 7 (tests) — alongside each phase
