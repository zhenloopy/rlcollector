import { invoke } from "@tauri-apps/api/core";
import type { AnalysisStatus, CaptureSession, CaptureStatus, MonitorInfo, OllamaStatus, Screenshot, Task } from "../types";

export async function startCapture(intervalMs?: number, description?: string, title?: string): Promise<void> {
  return invoke("start_capture", { intervalMs, description, title });
}

export async function stopCapture(): Promise<void> {
  return invoke("stop_capture");
}

export async function getCaptureStatus(): Promise<CaptureStatus> {
  return invoke("get_capture_status");
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke("get_setting", { key });
}

export async function updateSetting(
  key: string,
  value: string
): Promise<void> {
  return invoke("update_setting", { key, value });
}

export async function deleteSession(sessionId: number): Promise<number> {
  return invoke("delete_session", { sessionId });
}

export async function getAnalysisStatus(): Promise<AnalysisStatus> {
  return invoke("get_analysis_status");
}

export async function cancelAnalysis(): Promise<void> {
  return invoke("cancel_analysis");
}

export async function getLogPath(): Promise<string> {
  return invoke("get_log_path");
}

export async function getSessionScreenshots(
  sessionId: number
): Promise<Screenshot[]> {
  return invoke("get_session_screenshots", { sessionId });
}

export async function getScreenshotsDir(): Promise<string> {
  return invoke("get_screenshots_dir");
}

export async function getSessionTasks(
  sessionId: number
): Promise<Task[]> {
  return invoke("get_session_tasks", { sessionId });
}

export async function getTaskForScreenshot(
  screenshotId: number
): Promise<Task | null> {
  return invoke("get_task_for_screenshot", { screenshotId });
}

export async function analyzeSession(sessionId: number): Promise<number> {
  return invoke("analyze_session", { sessionId });
}

export async function analyzeAllPending(): Promise<number> {
  return invoke("analyze_all_pending");
}

export async function getPendingSessions(
  limit?: number,
  offset?: number
): Promise<CaptureSession[]> {
  return invoke("get_pending_sessions", { limit, offset });
}

export async function getCompletedSessions(
  limit?: number,
  offset?: number
): Promise<CaptureSession[]> {
  return invoke("get_completed_sessions", { limit, offset });
}

export async function checkOllama(): Promise<OllamaStatus> {
  return invoke("check_ollama");
}

export async function ensureOllama(): Promise<OllamaStatus> {
  return invoke("ensure_ollama");
}

export async function ollamaPull(model: string): Promise<void> {
  return invoke("ollama_pull", { model });
}

export async function getMonitors(): Promise<MonitorInfo[]> {
  return invoke("get_monitors");
}

export async function highlightMonitors(mode: string, monitorId?: number): Promise<void> {
  return invoke("highlight_monitors", { mode, monitorId });
}
