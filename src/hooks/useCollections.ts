import { useCallback, useEffect, useState } from "react";
import type { CaptureSession } from "../types";
import { getSessions } from "../lib/tauri";

const PAGE_SIZE = 20;

export function useCollections() {
  const [sessions, setSessions] = useState<CaptureSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(false);

  const refresh = useCallback(async (currentPage?: number) => {
    const p = currentPage ?? 0;
    setLoading(true);
    try {
      const result = await getSessions(PAGE_SIZE, p * PAGE_SIZE);
      setSessions(result);
      setHasMore(result.length === PAGE_SIZE);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh(page);
  }, [refresh, page]);

  const nextPage = useCallback(() => {
    if (hasMore) {
      setPage((p) => p + 1);
    }
  }, [hasMore]);

  const prevPage = useCallback(() => {
    setPage((p) => Math.max(0, p - 1));
  }, []);

  return { sessions, loading, refresh, page, hasMore, nextPage, prevPage };
}
