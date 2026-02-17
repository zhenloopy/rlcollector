import { useState } from "react";
import { useCapture } from "../hooks/useCapture";

export function CaptureControls({ onStop }: { onStop?: () => void }) {
  const { status, start, stop, loading, error } = useCapture();
  const [intervalSec, setIntervalSec] = useState(
    Math.round(status.interval_ms / 1000)
  );
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");

  return (
    <div className="capture-controls">
      <h2>Screen Capture</h2>
      <div className="status">
        <span className={`indicator ${status.active ? "active" : "inactive"}`} />
        <span>{status.active ? "Recording" : "Stopped"}</span>
        {status.active && <span> — {status.count} captures</span>}
      </div>
      {error && <div className="error-msg">{error}</div>}
      <div className="controls">
        <label>
          Session title
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="e.g. Auth page implementation"
            disabled={status.active}
          />
        </label>
        <label>
          What are you working on?
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="e.g. Building the auth page for my React app"
            disabled={status.active}
            rows={2}
          />
        </label>
        <label>
          Interval (seconds):
          <input
            type="number"
            min={1}
            max={300}
            value={intervalSec}
            onChange={(e) => setIntervalSec(Number(e.target.value))}
            disabled={status.active}
          />
        </label>
        {status.active ? (
          <button onClick={async () => { await stop(); onStop?.(); }} disabled={loading}>
            Stop Capture
          </button>
        ) : (
          <button
            onClick={() => start(intervalSec * 1000, title || undefined, description || undefined)}
            disabled={loading || !title.trim()}
          >
            Start Capture
          </button>
        )}
      </div>
    </div>
  );
}
