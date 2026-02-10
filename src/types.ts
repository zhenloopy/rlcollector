export interface Screenshot {
  id: number;
  filepath: string;
  captured_at: string;
  active_window_title: string | null;
  monitor_index: number;
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

export interface TaskUpdate {
  title?: string;
  description?: string;
  category?: string;
  ended_at?: string;
  user_verified?: boolean;
}

export interface CaptureStatus {
  active: boolean;
  interval_ms: number;
  count: number;
}

export interface CaptureSession {
  id: number;
  started_at: string;
  ended_at: string | null;
  screenshot_count: number;
}
