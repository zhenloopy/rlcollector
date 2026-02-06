import { useTasks } from "../hooks/useTasks";
import type { Task } from "../types";

function TaskRow({
  task,
  onDelete,
}: {
  task: Task;
  onDelete: (id: number) => void;
}) {
  return (
    <tr>
      <td>{task.title}</td>
      <td>{task.category ?? "—"}</td>
      <td>{new Date(task.started_at).toLocaleString()}</td>
      <td>{task.user_verified ? "Yes" : "No"}</td>
      <td>
        <button onClick={() => onDelete(task.id)}>Delete</button>
      </td>
    </tr>
  );
}

export function Dashboard() {
  const { tasks, loading, remove } = useTasks();

  if (loading) {
    return <div>Loading tasks...</div>;
  }

  if (tasks.length === 0) {
    return (
      <div className="dashboard">
        <h2>Tasks</h2>
        <p>No tasks recorded yet. Start capturing to begin.</p>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <h2>Tasks ({tasks.length})</h2>
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
            <TaskRow key={task.id} task={task} onDelete={remove} />
          ))}
        </tbody>
      </table>
    </div>
  );
}
