import { useCallback, useEffect, useState } from "react";
import type { Task, TaskUpdate } from "../types";
import {
  deleteTask as apiDeleteTask,
  getTasks,
  updateTask as apiUpdateTask,
} from "../lib/tauri";

const PAGE_SIZE = 20;

export function useTasks() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(false);

  const refresh = useCallback(async (currentPage?: number) => {
    const p = currentPage ?? 0;
    setLoading(true);
    try {
      const result = await getTasks(PAGE_SIZE, p * PAGE_SIZE);
      setTasks(result);
      setHasMore(result.length === PAGE_SIZE);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh(page);
  }, [refresh, page]);

  const update = useCallback(
    async (id: number, fields: TaskUpdate) => {
      await apiUpdateTask(id, fields);
      await refresh(page);
    },
    [refresh, page]
  );

  const remove = useCallback(
    async (id: number) => {
      await apiDeleteTask(id);
      await refresh(page);
    },
    [refresh, page]
  );

  const nextPage = useCallback(() => {
    if (hasMore) {
      setPage((p) => p + 1);
    }
  }, [hasMore]);

  const prevPage = useCallback(() => {
    setPage((p) => Math.max(0, p - 1));
  }, []);

  return { tasks, loading, refresh, update, remove, page, hasMore, nextPage, prevPage };
}
