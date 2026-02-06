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
      await startCapture(intervalMs);
      await refresh();
      setLoading(false);
    },
    [refresh]
  );

  const stop = useCallback(async () => {
    setLoading(true);
    await stopCapture();
    await refresh();
    setLoading(false);
  }, [refresh]);

  return { status, start, stop, loading, refresh };
}
