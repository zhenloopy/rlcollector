import { useState } from "react";
import { useCollections } from "../hooks/useCollections";
import { CollectionDetail } from "./CollectionDetail";
import type { CaptureSession } from "../types";

function SessionRow({
  session,
  onClick,
}: {
  session: CaptureSession;
  onClick: (id: number) => void;
}) {
  const started = new Date(session.started_at).toLocaleString();
  const ended = session.ended_at
    ? new Date(session.ended_at).toLocaleString()
    : "In progress";

  return (
    <tr>
      <td>
        <span
          className="task-link"
          onClick={() => onClick(session.id)}
          style={{ cursor: "pointer" }}
        >
          {started}
        </span>
      </td>
      <td>{ended}</td>
      <td>{session.screenshot_count}</td>
    </tr>
  );
}

export function Collections() {
  const { sessions, loading, page, hasMore, nextPage, prevPage, refresh } =
    useCollections();
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(
    null
  );

  if (selectedSessionId !== null) {
    return (
      <CollectionDetail
        sessionId={selectedSessionId}
        onClose={() => {
          setSelectedSessionId(null);
          refresh(page);
        }}
      />
    );
  }

  if (loading) {
    return <div>Loading collections...</div>;
  }

  if (sessions.length === 0 && page === 0) {
    return (
      <div className="dashboard">
        <h2>Collections</h2>
        <p>No capture sessions yet. Start capturing to begin.</p>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <h2>Collections</h2>
      <table>
        <thead>
          <tr>
            <th>Started</th>
            <th>Ended</th>
            <th>Screenshots</th>
          </tr>
        </thead>
        <tbody>
          {sessions.map((session) => (
            <SessionRow
              key={session.id}
              session={session}
              onClick={setSelectedSessionId}
            />
          ))}
        </tbody>
      </table>
      <div className="pagination">
        <button onClick={prevPage} disabled={page === 0}>
          Previous
        </button>
        <span>Page {page + 1}</span>
        <button onClick={nextPage} disabled={!hasMore}>
          Next
        </button>
      </div>
    </div>
  );
}
