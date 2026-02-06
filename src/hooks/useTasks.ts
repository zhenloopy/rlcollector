import { useCallback, useEffect, useState } from "react";
import type { Task, TaskUpdate } from "../types";
import {
  deleteTask as apiDeleteTask,
  getTasks,
  updateTask as apiUpdateTask,
} from "../lib/tauri";

export function useTasks() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    const result = await getTasks(50, 0);
    setTasks(result);
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const update = useCallback(
    async (id: number, fields: TaskUpdate) => {
      await apiUpdateTask(id, fields);
      await refresh();
    },
    [refresh]
  );

  const remove = useCallback(
    async (id: number) => {
      await apiDeleteTask(id);
      await refresh();
    },
    [refresh]
  );

  return { tasks, loading, refresh, update, remove };
}
