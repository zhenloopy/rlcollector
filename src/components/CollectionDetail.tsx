import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Screenshot } from "../types";
import { getSessionScreenshots, getScreenshotsDir } from "../lib/tauri";

export function CollectionDetail({
  sessionId,
  onClose,
}: {
  sessionId: number;
  onClose: () => void;
}) {
  const [screenshots, setScreenshots] = useState<Screenshot[]>([]);
  const [screenshotsDir, setScreenshotsDir] = useState<string>("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [shots, dir] = await Promise.all([
        getSessionScreenshots(sessionId),
        getScreenshotsDir(),
      ]);
      setScreenshots(shots);
      setScreenshotsDir(dir);
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    load();
  }, [load]);

  if (loading) {
    return <div>Loading screenshots...</div>;
  }

  return (
    <div className="collection-detail">
      <button className="back-button" onClick={onClose}>
        Back to Collections
      </button>
      <h2>Session Screenshots</h2>
      {screenshots.length === 0 ? (
        <p>No screenshots in this session.</p>
      ) : (
        <div className="screenshot-grid">
          {screenshots.map((shot) => {
            const filename = shot.filepath.replace(/^screenshots\//, "");
            const filePath = screenshotsDir + "/" + filename;
            const src = convertFileSrc(filePath);
            return (
              <div key={shot.id} className="screenshot-card">
                <img src={src} alt={`Screenshot ${shot.id}`} loading="lazy" />
                <div className="screenshot-info">
                  <span>{new Date(shot.captured_at).toLocaleTimeString()}</span>
                  {shot.active_window_title && (
                    <span className="window-title">
                      {shot.active_window_title}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
