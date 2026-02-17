import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Screenshot, Task } from "../types";
import { getSessionScreenshots, getScreenshotsDir, getSessionTasks, getTaskForScreenshot } from "../lib/tauri";

export function CollectionDetail({
  sessionId,
  onClose,
  backLabel = "Back to Sessions",
}: {
  sessionId: number;
  onClose: () => void;
  backLabel?: string;
}) {
  const [screenshots, setScreenshots] = useState<Screenshot[]>([]);
  const [screenshotsDir, setScreenshotsDir] = useState<string>("");
  const [sessionTasks, setSessionTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [selectedTask, setSelectedTask] = useState<Task | null>(null);
  const [taskLoading, setTaskLoading] = useState(false);

  const selected = selectedIndex !== null ? screenshots[selectedIndex] ?? null : null;

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [shots, dir, tasks] = await Promise.all([
        getSessionScreenshots(sessionId),
        getScreenshotsDir(),
        getSessionTasks(sessionId),
      ]);
      setScreenshots(shots);
      setScreenshotsDir(dir);
      setSessionTasks(tasks);
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    load();
  }, [load]);

  const loadTask = useCallback(async (shot: Screenshot) => {
    setSelectedTask(null);
    setTaskLoading(true);
    try {
      const task = await getTaskForScreenshot(shot.id);
      setSelectedTask(task);
    } finally {
      setTaskLoading(false);
    }
  }, []);

  const openScreenshot = useCallback((index: number) => {
    setSelectedIndex(index);
    loadTask(screenshots[index]);
  }, [screenshots, loadTask]);

  const closeModal = useCallback(() => {
    setSelectedIndex(null);
    setSelectedTask(null);
  }, []);

  const goNext = useCallback(() => {
    if (selectedIndex === null) return;
    const next = selectedIndex + 1;
    if (next < screenshots.length) {
      setSelectedIndex(next);
      loadTask(screenshots[next]);
    }
  }, [selectedIndex, screenshots, loadTask]);

  const goPrev = useCallback(() => {
    if (selectedIndex === null) return;
    const prev = selectedIndex - 1;
    if (prev >= 0) {
      setSelectedIndex(prev);
      loadTask(screenshots[prev]);
    }
  }, [selectedIndex, screenshots, loadTask]);

  // Keyboard navigation
  useEffect(() => {
    if (selectedIndex === null) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeModal();
      else if (e.key === "ArrowRight") goNext();
      else if (e.key === "ArrowLeft") goPrev();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectedIndex, closeModal, goNext, goPrev]);

  const getImageSrc = (shot: Screenshot) => {
    const filename = shot.filepath.replace(/^screenshots\//, "");
    return convertFileSrc(screenshotsDir + "/" + filename);
  };

  const hasPrev = selectedIndex !== null && selectedIndex > 0;
  const hasNext = selectedIndex !== null && selectedIndex < screenshots.length - 1;

  if (loading) {
    return <div>Loading session...</div>;
  }

  return (
    <div className="collection-detail">
      <button className="back-button" onClick={onClose}>
        {backLabel}
      </button>

      {sessionTasks.length > 0 && (
        <div className="session-tasks-section">
          <h2>Tasks</h2>
          <div className="session-tasks-list">
            {sessionTasks.map((task) => (
              <div key={task.id} className="session-task-card">
                <div className="session-task-header">
                  <span className="session-task-title">{task.title}</span>
                  {task.category && (
                    <span className="badge verified">{task.category}</span>
                  )}
                  <span className="session-task-time">
                    {new Date(task.started_at).toLocaleTimeString()}
                  </span>
                </div>
                {task.description && (
                  <p className="session-task-description">{task.description}</p>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      <h2>Screenshots</h2>
      {screenshots.length === 0 ? (
        <p>No screenshots in this session.</p>
      ) : (
        <div className="screenshot-grid">
          {screenshots.map((shot, i) => (
            <div
              key={shot.id}
              className="screenshot-card clickable"
              onClick={() => openScreenshot(i)}
            >
              <img src={getImageSrc(shot)} alt={`Screenshot ${shot.id}`} loading="lazy" />
              <div className="screenshot-info">
                <span>{new Date(shot.captured_at).toLocaleTimeString()}</span>
                {shot.active_window_title && (
                  <span className="window-title">
                    {shot.active_window_title}
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {selected && (
        <div className="screenshot-modal-overlay" onClick={closeModal}>
          <button
            className="modal-arrow modal-arrow-left"
            disabled={!hasPrev}
            onClick={(e) => { e.stopPropagation(); goPrev(); }}
            aria-label="Previous screenshot"
          >
            &#8249;
          </button>
          <div className="screenshot-modal" onClick={(e) => e.stopPropagation()}>
            <button className="modal-close" onClick={closeModal}>
              &times;
            </button>
            <img
              className="screenshot-modal-image"
              src={getImageSrc(selected)}
              alt={`Screenshot ${selected.id}`}
            />
            <div className="screenshot-modal-info">
              <div className="screenshot-modal-meta">
                <span>{new Date(selected.captured_at).toLocaleString()}</span>
                {selected.active_window_title && (
                  <span className="window-title">{selected.active_window_title}</span>
                )}
                <span className="modal-counter">
                  {(selectedIndex ?? 0) + 1} / {screenshots.length}
                </span>
              </div>
              {taskLoading ? (
                <p className="task-loading">Loading analysis...</p>
              ) : selectedTask ? (
                <div className="screenshot-task-info">
                  <h3>{selectedTask.title}</h3>
                  {selectedTask.category && (
                    <span className="badge verified">{selectedTask.category}</span>
                  )}
                  {selectedTask.description && (
                    <div className="task-field">
                      <strong>Description</strong>
                      <p>{selectedTask.description}</p>
                    </div>
                  )}
                  {selectedTask.ai_reasoning && (
                    <div className="task-field">
                      <strong>AI Reasoning</strong>
                      <p>{selectedTask.ai_reasoning}</p>
                    </div>
                  )}
                </div>
              ) : (
                <p className="task-not-analyzed">Not yet analyzed</p>
              )}
            </div>
          </div>
          <button
            className="modal-arrow modal-arrow-right"
            disabled={!hasNext}
            onClick={(e) => { e.stopPropagation(); goNext(); }}
            aria-label="Next screenshot"
          >
            &#8250;
          </button>
        </div>
      )}
    </div>
  );
}
