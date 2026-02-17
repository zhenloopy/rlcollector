import { useState } from "react";
import { useSessions } from "../hooks/useSessions";
import { analyzeSession, analyzeAllPending, cancelAnalysis, deleteSession } from "../lib/tauri";
import { CollectionDetail } from "./CollectionDetail";
import type { CaptureSession } from "../types";

function SessionCard({
  session,
  analyzing,
  onAnalyze,
  onDelete,
}: {
  session: CaptureSession;
  analyzing: boolean;
  onAnalyze: (id: number) => void;
  onDelete: (id: number) => void;
}) {
  const started = new Date(session.started_at).toLocaleString();
  const analyzed = session.screenshot_count - session.unanalyzed_count;

  return (
    <div className="session-card">
      <div className="session-card-info">
        <h3>{session.title || "Untitled Session"}</h3>
        {session.description && (
          <p className="session-description">{session.description}</p>
        )}
        <div className="session-meta">
          <span>{started}</span>
          <span>{session.screenshot_count} screenshots</span>
          {analyzing ? (
            <span>{analyzed}/{session.screenshot_count} analyzed</span>
          ) : (
            <span>{session.unanalyzed_count} unanalyzed</span>
          )}
        </div>
      </div>
      <div className="session-card-actions">
        <button
          className="analyze-button"
          onClick={() => onAnalyze(session.id)}
          disabled={analyzing}
        >
          {analyzing ? <><span className="spinner" /> Analyzing...</> : "Analyze"}
        </button>
        <button
          className="delete-button"
          onClick={() => onDelete(session.id)}
          disabled={analyzing}
        >
          Delete
        </button>
      </div>
    </div>
  );
}

function CompletedSessionCard({
  session,
  onClick,
  onDelete,
}: {
  session: CaptureSession;
  onClick: (id: number) => void;
  onDelete: (id: number) => void;
}) {
  const started = new Date(session.started_at).toLocaleString();

  return (
    <div className="session-card clickable" onClick={() => onClick(session.id)}>
      <div className="session-card-info">
        <h3>{session.title || "Untitled Session"}</h3>
        {session.description && (
          <p className="session-description">{session.description}</p>
        )}
        <div className="session-meta">
          <span>{started}</span>
          <span>{session.screenshot_count} screenshots</span>
        </div>
      </div>
      <div className="session-card-actions">
        <button
          className="delete-button"
          onClick={(e) => { e.stopPropagation(); onDelete(session.id); }}
        >
          Delete
        </button>
      </div>
    </div>
  );
}

export function Dashboard({ refreshTrigger }: { refreshTrigger?: number }) {
  const {
    pending,
    completed,
    loading,
    refresh,
    completedPage,
    hasMoreCompleted,
    nextCompletedPage,
    prevCompletedPage,
    analyzingSessionId: backendAnalyzingId,
  } = useSessions(refreshTrigger);
  const [userAnalyzeAll, setUserAnalyzeAll] = useState(false);
  const [analyzeMsg, setAnalyzeMsg] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(null);

  const isAnalyzing = backendAnalyzingId !== null || userAnalyzeAll;

  const handleAnalyzeSession = async (sessionId: number) => {
    setAnalyzeMsg(null);
    try {
      const count = await analyzeSession(sessionId);
      setAnalyzeMsg(
        count > 0 ? `Analyzed ${count} screenshot${count > 1 ? "s" : ""}` : "No pending screenshots"
      );
      refresh(completedPage);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setAnalyzeMsg(`Error: ${msg}`);
    } finally {
      setTimeout(() => setAnalyzeMsg(null), 4000);
    }
  };

  const handleAnalyzeAll = async () => {
    setUserAnalyzeAll(true);
    setAnalyzeMsg(null);
    try {
      const count = await analyzeAllPending();
      setAnalyzeMsg(
        count > 0 ? `Analyzed ${count} screenshot${count > 1 ? "s" : ""}` : "No pending screenshots"
      );
      refresh(completedPage);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setAnalyzeMsg(`Error: ${msg}`);
    } finally {
      setUserAnalyzeAll(false);
      setTimeout(() => setAnalyzeMsg(null), 4000);
    }
  };

  const handleCancelAnalysis = async () => {
    await cancelAnalysis();
  };

  const handleDeleteSession = async (sessionId: number) => {
    try {
      await deleteSession(sessionId);
      refresh(completedPage);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setAnalyzeMsg(`Error: ${msg}`);
      setTimeout(() => setAnalyzeMsg(null), 4000);
    }
  };

  if (selectedSessionId !== null) {
    return (
      <CollectionDetail
        sessionId={selectedSessionId}
        onClose={() => {
          setSelectedSessionId(null);
          refresh(completedPage);
        }}
        backLabel="Back to Sessions"
      />
    );
  }

  if (loading) {
    return <div>Loading sessions...</div>;
  }

  return (
    <div className="dashboard">
      {/* Pending Sessions */}
      <div className="dashboard-section">
        <div className="dashboard-header">
          <h2>Pending Sessions</h2>
          <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
            {pending.length > 0 && (
              <button
                className="analyze-button"
                onClick={isAnalyzing ? handleCancelAnalysis : handleAnalyzeAll}
                disabled={false}
              >
                {isAnalyzing ? (
                  <><span className="spinner" /> Cancel</>
                ) : (
                  "Analyze All"
                )}
              </button>
            )}
            {analyzeMsg && (
              <span className={analyzeMsg.startsWith("Error") ? "analyze-error" : "saved-msg"}>
                {analyzeMsg}
              </span>
            )}
          </div>
        </div>
        {pending.length === 0 ? (
          <p>No pending sessions. Start a capture to create one.</p>
        ) : (
          <div className="session-cards">
            {pending.map((session) => (
              <SessionCard
                key={session.id}
                session={session}
                analyzing={
                  backendAnalyzingId === session.id || userAnalyzeAll
                }
                onAnalyze={handleAnalyzeSession}
                onDelete={handleDeleteSession}
              />
            ))}
          </div>
        )}
      </div>

      {/* Completed Sessions */}
      <div className="dashboard-section">
        <h2>Completed Sessions</h2>
        {completed.length === 0 ? (
          <p>No completed sessions yet.</p>
        ) : (
          <>
            <div className="session-cards">
              {completed.map((session) => (
                <CompletedSessionCard
                  key={session.id}
                  session={session}
                  onClick={setSelectedSessionId}
                  onDelete={handleDeleteSession}
                />
              ))}
            </div>
            <div className="pagination">
              <button onClick={prevCompletedPage} disabled={completedPage === 0}>
                Previous
              </button>
              <span>Page {completedPage + 1}</span>
              <button onClick={nextCompletedPage} disabled={!hasMoreCompleted}>
                Next
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
