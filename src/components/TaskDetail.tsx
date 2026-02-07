import { useCallback, useEffect, useState } from "react";
import { getTask, updateTask } from "../lib/tauri";
import type { Task, TaskUpdate } from "../types";

interface TaskDetailProps {
  taskId: number;
  onClose: () => void;
}

export function TaskDetail({ taskId, onClose }: TaskDetailProps) {
  const [task, setTask] = useState<Task | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [editTitle, setEditTitle] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editCategory, setEditCategory] = useState("");

  const fetchTask = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getTask(taskId);
      setTask(data);
      setEditTitle(data.title);
      setEditDescription(data.description ?? "");
      setEditCategory(data.category ?? "");
    } finally {
      setLoading(false);
    }
  }, [taskId]);

  useEffect(() => {
    fetchTask();
  }, [fetchTask]);

  const handleSave = async () => {
    if (!task) return;
    const update: TaskUpdate = {
      title: editTitle,
      description: editDescription,
      category: editCategory,
    };
    await updateTask(task.id, update);
    setEditing(false);
    await fetchTask();
  };

  const handleCancelEdit = () => {
    if (task) {
      setEditTitle(task.title);
      setEditDescription(task.description ?? "");
      setEditCategory(task.category ?? "");
    }
    setEditing(false);
  };

  const handleToggleVerify = async () => {
    if (!task) return;
    await updateTask(task.id, { user_verified: !task.user_verified });
    await fetchTask();
  };

  if (loading) {
    return <div>Loading task...</div>;
  }

  if (!task) {
    return <div>Task not found.</div>;
  }

  return (
    <div className="task-detail">
      <button className="back-button" onClick={onClose}>
        Back to Tasks
      </button>

      {editing ? (
        <div className="task-edit-form">
          <label>
            Title:
            <input
              type="text"
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
            />
          </label>
          <label>
            Description:
            <textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
            />
          </label>
          <label>
            Category:
            <input
              type="text"
              value={editCategory}
              onChange={(e) => setEditCategory(e.target.value)}
            />
          </label>
          <div className="task-edit-actions">
            <button onClick={handleSave}>Save</button>
            <button onClick={handleCancelEdit}>Cancel</button>
          </div>
        </div>
      ) : (
        <div className="task-info">
          <h2>{task.title}</h2>
          <span className={`badge ${task.user_verified ? "verified" : "not-verified"}`}>
            {task.user_verified ? "Verified" : "Not verified"}
          </span>

          {task.description && (
            <div className="task-field">
              <strong>Description</strong>
              <p>{task.description}</p>
            </div>
          )}

          {task.category && (
            <div className="task-field">
              <strong>Category</strong>
              <p>{task.category}</p>
            </div>
          )}

          <div className="task-field">
            <strong>Started</strong>
            <p>{new Date(task.started_at).toLocaleString()}</p>
          </div>

          {task.ended_at && (
            <div className="task-field">
              <strong>Ended</strong>
              <p>{new Date(task.ended_at).toLocaleString()}</p>
            </div>
          )}

          {task.ai_reasoning && (
            <div className="task-field">
              <strong>AI Reasoning</strong>
              <p>{task.ai_reasoning}</p>
            </div>
          )}

          <div className="task-actions">
            <button onClick={() => setEditing(true)}>Edit</button>
            <button onClick={handleToggleVerify}>
              {task.user_verified ? "Mark as Unverified" : "Mark as Verified"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
