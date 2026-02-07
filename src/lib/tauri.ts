import { invoke } from "@tauri-apps/api/core";
import type { CaptureStatus, Task, TaskUpdate } from "../types";

export async function startCapture(intervalMs?: number): Promise<void> {
  return invoke("start_capture", { intervalMs });
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

export async function analyzePending(): Promise<void> {
  return invoke("analyze_pending");
}
