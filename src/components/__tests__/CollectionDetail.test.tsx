import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CollectionDetail } from "../CollectionDetail";
import type { Screenshot } from "../../types";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
}));

const mockGetSessionScreenshots = vi.fn<(sessionId: number) => Promise<Screenshot[]>>();
const mockGetScreenshotsDir = vi.fn<() => Promise<string>>();

vi.mock("../../lib/tauri", () => ({
  getSessionScreenshots: (...args: unknown[]) =>
    mockGetSessionScreenshots(...(args as [number])),
  getScreenshotsDir: (...args: unknown[]) =>
    mockGetScreenshotsDir(...(args as [])),
}));

describe("CollectionDetail", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetScreenshotsDir.mockResolvedValue("/app/data/screenshots");
  });

  it("shows loading state", () => {
    mockGetSessionScreenshots.mockReturnValue(new Promise(() => {})); // never resolves
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);
    expect(screen.getByText("Loading screenshots...")).toBeInTheDocument();
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
      },
      {
        id: 2,
        filepath: "screenshots/shot2.webp",
        captured_at: "2025-01-01T10:00:30",
        active_window_title: null,
        monitor_index: 0,
      },
    ]);
    render(<CollectionDetail sessionId={1} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Session Screenshots")).toBeInTheDocument();
    });

    const images = screen.getAllByRole("img");
    expect(images).toHaveLength(2);
    expect(screen.getByText("VS Code")).toBeInTheDocument();
  });

  it("calls onClose when back button is clicked", async () => {
    const onClose = vi.fn();
    mockGetSessionScreenshots.mockResolvedValue([]);
    render(<CollectionDetail sessionId={1} onClose={onClose} />);

    await waitFor(() => {
      expect(
        screen.getByText("Back to Collections")
      ).toBeInTheDocument();
    });

    const user = userEvent.setup();
    await user.click(screen.getByText("Back to Collections"));
    expect(onClose).toHaveBeenCalled();
  });
});
