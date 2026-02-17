import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CollectionDetail } from "../CollectionDetail";
import type { Screenshot, Task } from "../../types";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
}));

const mockGetSessionScreenshots = vi.fn<(sessionId: number) => Promise<Screenshot[]>>();
const mockGetScreenshotsDir = vi.fn<() => Promise<string>>();
const mockGetSessionTasks = vi.fn<(sessionId: number) => Promise<Task[]>>();

vi.mock("../../lib/tauri", () => ({
  getSessionScreenshots: (...args: unknown[]) =>
    mockGetSessionScreenshots(...(args as [number])),
  getScreenshotsDir: (...args: unknown[]) =>
    mockGetScreenshotsDir(...(args as [])),
  getSessionTasks: (...args: unknown[]) =>
    mockGetSessionTasks(...(args as [number])),
  getTaskForScreenshot: () => Promise.resolve(null),
}));

describe("CollectionDetail", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetScreenshotsDir.mockResolvedValue("/app/data/screenshots");
    mockGetSessionTasks.mockResolvedValue([]);
  });

  it("shows loading state", () => {
    mockGetSessionScreenshots.mockReturnValue(new Promise(() => {})); // never resolves
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);
    expect(screen.getByText("Loading session...")).toBeInTheDocument();
  });

  it("shows empty state when no screenshots", async () => {
    mockGetSessionScreenshots.mockResolvedValue([]);
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);
    await waitFor(() => {
      expect(
        screen.getByText("No screenshots in this session.")
      ).toBeInTheDocument();
    });
  });

  it("renders screenshot grid", async () => {
    mockGetSessionScreenshots.mockResolvedValue([
      {
        id: 1,
        filepath: "screenshots/shot1.webp",
        captured_at: "2025-01-01T10:00:00",
        active_window_title: "VS Code",
        monitor_index: 0,
        capture_group: null,
      },
      {
        id: 2,
        filepath: "screenshots/shot2.webp",
        captured_at: "2025-01-01T10:00:30",
        active_window_title: null,
        monitor_index: 0,
        capture_group: null,
      },
    ]);
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Screenshots")).toBeInTheDocument();
    });

    const images = screen.getAllByRole("img");
    expect(images).toHaveLength(2);
    expect(screen.getByText("VS Code")).toBeInTheDocument();
  });

  it("renders session tasks above screenshots", async () => {
    mockGetSessionScreenshots.mockResolvedValue([]);
    mockGetSessionTasks.mockResolvedValue([
      {
        id: 1,
        title: "Writing Rust code",
        description: "Editing storage.rs",
        category: "coding",
        started_at: "2025-01-01T10:00:00",
        ended_at: null,
        ai_reasoning: "IDE open",
        user_verified: false,
        metadata: null,
      },
      {
        id: 2,
        title: "Browsing docs",
        description: "Reading Tauri v2 docs",
        category: "browsing",
        started_at: "2025-01-01T10:05:00",
        ended_at: null,
        ai_reasoning: "Browser open",
        user_verified: false,
        metadata: null,
      },
    ]);
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Tasks")).toBeInTheDocument();
    });

    expect(screen.getByText("Writing Rust code")).toBeInTheDocument();
    expect(screen.getByText("Editing storage.rs")).toBeInTheDocument();
    expect(screen.getByText("coding")).toBeInTheDocument();
    expect(screen.getByText("Browsing docs")).toBeInTheDocument();
    expect(screen.getByText("Reading Tauri v2 docs")).toBeInTheDocument();
  });

  it("hides tasks section when no tasks", async () => {
    mockGetSessionScreenshots.mockResolvedValue([]);
    mockGetSessionTasks.mockResolvedValue([]);
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Screenshots")).toBeInTheDocument();
    });

    expect(screen.queryByText("Tasks")).not.toBeInTheDocument();
  });

  it("calls onClose when back button is clicked", async () => {
    const onClose = vi.fn();
    mockGetSessionScreenshots.mockResolvedValue([]);
    render(<CollectionDetail sessionId={1} onClose={onClose} />);

    await waitFor(() => {
      expect(
        screen.getByText("Back to Sessions")
      ).toBeInTheDocument();
    });

    const user = userEvent.setup();
    await user.click(screen.getByText("Back to Sessions"));
    expect(onClose).toHaveBeenCalled();
  });
});
