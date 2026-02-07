import { useState } from "react";
import { useTasks } from "../hooks/useTasks";
import { analyzePending } from "../lib/tauri";
import { TaskDetail } from "./TaskDetail";
import type { Task } from "../types";

function TaskRow({
  task,
  onDelete,
  onClick,
}: {
  task: Task;
  onDelete: (id: number) => void;
  onClick: (id: number) => void;
}) {
  return (
    <tr>
      <td>
        <span className="task-link" onClick={() => onClick(task.id)} style={{ cursor: "pointer" }}>
          {task.title}
        </span>
      </td>
      <td>{task.category ?? "\u2014"}</td>
      <td>{new Date(task.started_at).toLocaleString()}</td>
      <td>{task.user_verified ? "Yes" : "No"}</td>
      <td>
        <button onClick={() => onDelete(task.id)}>Delete</button>
      </td>
    </tr>
  );
}

export function Dashboard() {
  const { tasks, loading, remove, page, hasMore, nextPage, prevPage } = useTasks();
  const [selectedTaskId, setSelectedTaskId] = useState<number | null>(null);

  if (selectedTaskId !== null) {
    return (
      <TaskDetail
        taskId={selectedTaskId}
        onClose={() => setSelectedTaskId(null)}
      />
    );
  }

  if (loading) {
    return <div>Loading tasks...</div>;
  }

  if (tasks.length === 0 && page === 0) {
    return (
      <div className="dashboard">
        <h2>Tasks</h2>
        <p>No tasks recorded yet. Start capturing to begin.</p>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <div className="dashboard-header">
        <h2>Tasks</h2>
        <button className="analyze-button" onClick={() => analyzePending()}>
          Analyze Pending
        </button>
      </div>
      <table>
        <thead>
          <tr>
            <th>Title</th>
            <th>Category</th>
            <th>Started</th>
            <th>Verified</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {tasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              onDelete={remove}
              onClick={setSelectedTaskId}
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
