export interface Screenshot {
  id: number;
  filepath: string;
  captured_at: string;
  active_window_title: string | null;
  monitor_index: number;
  capture_group: string | null;
}

export interface MonitorInfo {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  is_primary: boolean;
}

export interface Task {
  id: number;
  title: string;
  description: string | null;
  category: string | null;
  started_at: string;
  ended_at: string | null;
  ai_reasoning: string | null;
  user_verified: boolean;
  metadata: string | null;
}

export interface CaptureStatus {
  active: boolean;
  interval_ms: number;
  count: number;
  monitor_mode: string;
  monitors_captured: number;
}

export interface CaptureSession {
  id: number;
  started_at: string;
  ended_at: string | null;
  screenshot_count: number;
  description: string | null;
  title: string | null;
  unanalyzed_count: number;
}

export interface OllamaStatus {
  available: boolean;
  models: string[];
  source: string;
}

export interface AnalysisStatus {
  analyzing: boolean;
  session_id: number | null;
}
