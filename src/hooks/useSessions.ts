import { useCallback, useEffect, useRef, useState } from "react";
import type { CaptureSession } from "../types";
import { getPendingSessions, getCompletedSessions, getAnalysisStatus } from "../lib/tauri";

const PAGE_SIZE = 20;
const POLL_INTERVAL_MS = 3000;

export function useSessions(refreshTrigger?: number) {
  const [pending, setPending] = useState<CaptureSession[]>([]);
  const [completed, setCompleted] = useState<CaptureSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [completedPage, setCompletedPage] = useState(0);
  const [hasMoreCompleted, setHasMoreCompleted] = useState(false);
  const [analyzingSessionId, setAnalyzingSessionId] = useState<number | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async (cPage?: number) => {
    const p = cPage ?? completedPage;
    setLoading(true);
    try {
      const [pend, comp] = await Promise.all([
        getPendingSessions(50, 0),
        getCompletedSessions(PAGE_SIZE, p * PAGE_SIZE),
      ]);
      setPending(pend);
      setCompleted(comp);
      setHasMoreCompleted(comp.length === PAGE_SIZE);
    } finally {
      setLoading(false);
    }
  }, [completedPage]);

  // Poll analysis status and refresh sessions while analysis is active
  const pollAnalysis = useCallback(async () => {
    try {
      const status = await getAnalysisStatus();
      setAnalyzingSessionId(status.session_id);
      if (status.analyzing) {
        // Refresh session data to update unanalyzed counts
        const p = completedPage;
        const [pend, comp] = await Promise.all([
          getPendingSessions(50, 0),
          getCompletedSessions(PAGE_SIZE, p * PAGE_SIZE),
        ]);
        setPending(pend);
        setCompleted(comp);
        setHasMoreCompleted(comp.length === PAGE_SIZE);
      }
    } catch {
      // Ignore polling errors
    }
  }, [completedPage]);

  // Start/stop polling based on analysis state
  useEffect(() => {
    // Check immediately on mount and when refreshTrigger changes
    pollAnalysis();
  }, [pollAnalysis, refreshTrigger]);

  useEffect(() => {
    // Always run a poll interval — it's lightweight when nothing is analyzing
    pollRef.current = setInterval(pollAnalysis, POLL_INTERVAL_MS);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [pollAnalysis]);

  // When analysis finishes (transitions from active to inactive), do a final refresh
  const prevAnalyzing = useRef<number | null>(null);
  useEffect(() => {
    if (prevAnalyzing.current !== null && analyzingSessionId === null) {
      // Analysis just finished — refresh to move session from pending to completed
      refresh(completedPage);
    }
    prevAnalyzing.current = analyzingSessionId;
  }, [analyzingSessionId, refresh, completedPage]);

  useEffect(() => {
    refresh(completedPage);
  }, [refresh, completedPage, refreshTrigger]);

  const nextCompletedPage = useCallback(() => {
    if (hasMoreCompleted) {
      setCompletedPage((p) => p + 1);
    }
  }, [hasMoreCompleted]);

  const prevCompletedPage = useCallback(() => {
    setCompletedPage((p) => Math.max(0, p - 1));
  }, []);

  return {
    pending,
    completed,
    loading,
    refresh,
    completedPage,
    hasMoreCompleted,
    nextCompletedPage,
    prevCompletedPage,
    analyzingSessionId,
  };
}
