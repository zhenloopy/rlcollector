import { useCallback, useEffect, useState } from "react";
import type { CaptureStatus } from "../types";
import {
  getCaptureStatus,
  startCapture,
  stopCapture,
} from "../lib/tauri";

export function useCapture() {
  const [status, setStatus] = useState<CaptureStatus>({
    active: false,
    interval_ms: 30000,
    count: 0,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const s = await getCaptureStatus();
    setStatus(s);
  }, []);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 2000);
    return () => clearInterval(interval);
  }, [refresh]);

  const start = useCallback(
    async (intervalMs?: number) => {
      setLoading(true);
      setError(null);
      try {
        await startCapture(intervalMs);
        await refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
      } finally {
        setLoading(false);
      }
    },
    [refresh]
  );

  const stop = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await stopCapture();
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [refresh]);

  return { status, start, stop, loading, error, refresh };
}
