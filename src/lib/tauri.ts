import { invoke } from "@tauri-apps/api/core";
import type { CaptureSession, CaptureStatus, OllamaStatus, Screenshot, Task, TaskUpdate } from "../types";

export async function startCapture(intervalMs?: number, description?: string): Promise<void> {
  return invoke("start_capture", { intervalMs, description });
}

export async function getCurrentSession(): Promise<CaptureSession | null> {
  return invoke("get_current_session");
}

export async function stopCapture(): Promise<void> {
  return invoke("stop_capture");
}

export async function getCaptureStatus(): Promise<CaptureStatus> {
  return invoke("get_capture_status");
}

export async function getTasks(
  limit?: number,
  offset?: number
): Promise<Task[]> {
  return invoke("get_tasks", { limit, offset });
}

export async function getTask(id: number): Promise<Task> {
  return invoke("get_task", { id });
}

export async function updateTask(
  id: number,
  update: TaskUpdate
): Promise<void> {
  return invoke("update_task", { id, update });
}

export async function deleteTask(id: number): Promise<void> {
  return invoke("delete_task", { id });
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

export async function analyzePending(): Promise<number> {
  return invoke("analyze_pending");
}

export async function cancelAnalysis(): Promise<void> {
  return invoke("cancel_analysis");
}

export async function clearPending(): Promise<number> {
  return invoke("clear_pending");
}

export async function getLogPath(): Promise<string> {
  return invoke("get_log_path");
}

export async function getSessions(
  limit?: number,
  offset?: number
): Promise<CaptureSession[]> {
  return invoke("get_sessions", { limit, offset });
}

export async function getSessionScreenshots(
  sessionId: number
): Promise<Screenshot[]> {
  return invoke("get_session_screenshots", { sessionId });
}

export async function getScreenshotsDir(): Promise<string> {
  return invoke("get_screenshots_dir");
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
