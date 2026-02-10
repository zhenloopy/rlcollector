import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Collections } from "../Collections";
import type { CaptureSession } from "../../types";

const mockRefresh = vi.fn();
const mockNextPage = vi.fn();
const mockPrevPage = vi.fn();

const mockUseCollections = vi.fn<
  () => {
    sessions: CaptureSession[];
    loading: boolean;
    refresh: (page?: number) => Promise<void>;
    page: number;
    hasMore: boolean;
    nextPage: () => void;
    prevPage: () => void;
  }
>();

vi.mock("../../hooks/useCollections", () => ({
  useCollections: (...args: unknown[]) => mockUseCollections(...args),
}));

// Mock CollectionDetail to avoid loading real screenshots
vi.mock("../CollectionDetail", () => ({
  CollectionDetail: ({
    sessionId,
    onClose,
  }: {
    sessionId: number;
    onClose: () => void;
  }) => (
    <div data-testid="collection-detail">
      <span>Session {sessionId}</span>
      <button onClick={onClose}>Back</button>
    </div>
  ),
}));

describe("Collections", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows loading state", () => {
    mockUseCollections.mockReturnValue({
      sessions: [],
      loading: true,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);
    expect(screen.getByText("Loading collections...")).toBeInTheDocument();
  });

  it("shows empty state when no sessions", () => {
    mockUseCollections.mockReturnValue({
      sessions: [],
      loading: false,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);
    expect(
      screen.getByText("No capture sessions yet. Start capturing to begin.")
    ).toBeInTheDocument();
  });

  it("renders session list", () => {
    mockUseCollections.mockReturnValue({
      sessions: [
        {
          id: 1,
          started_at: "2025-01-01T10:00:00",
          ended_at: "2025-01-01T10:30:00",
          screenshot_count: 5,
        },
        {
          id: 2,
          started_at: "2025-01-02T14:00:00",
          ended_at: null,
          screenshot_count: 3,
        },
      ],
      loading: false,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);
    expect(screen.getByText("Collections")).toBeInTheDocument();
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("In progress")).toBeInTheDocument();
  });

  it("navigates to collection detail on click", async () => {
    const user = userEvent.setup();
    mockUseCollections.mockReturnValue({
      sessions: [
        {
          id: 42,
          started_at: "2025-01-01T10:00:00",
          ended_at: "2025-01-01T10:30:00",
          screenshot_count: 5,
        },
      ],
      loading: false,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);

    // Click the session row link (the started date)
    const link = screen.getByText(
      new Date("2025-01-01T10:00:00").toLocaleString()
    );
    await user.click(link);

    // Should show collection detail
    expect(screen.getByTestId("collection-detail")).toBeInTheDocument();
    expect(screen.getByText("Session 42")).toBeInTheDocument();
  });

  it("handles pagination", () => {
    mockUseCollections.mockReturnValue({
      sessions: [
        {
          id: 1,
          started_at: "2025-01-01T10:00:00",
          ended_at: "2025-01-01T10:30:00",
          screenshot_count: 5,
        },
      ],
      loading: false,
      refresh: mockRefresh,
      page: 1,
      hasMore: true,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);

    expect(screen.getByText("Page 2")).toBeInTheDocument();
    expect(screen.getByText("Previous")).not.toBeDisabled();
    expect(screen.getByText("Next")).not.toBeDisabled();
  });

  it("disables Previous on first page", () => {
    mockUseCollections.mockReturnValue({
      sessions: [
        {
          id: 1,
          started_at: "2025-01-01T10:00:00",
          ended_at: null,
          screenshot_count: 1,
        },
      ],
      loading: false,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Collections />);
    expect(screen.getByText("Previous")).toBeDisabled();
    expect(screen.getByText("Next")).toBeDisabled();
  });
});
