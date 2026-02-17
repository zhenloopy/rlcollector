import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Dashboard } from '../Dashboard';
import type { CaptureSession } from '../../types';

// Mock useSessions hook
const mockRefresh = vi.fn();
const mockNextCompletedPage = vi.fn();
const mockPrevCompletedPage = vi.fn();

const mockUseSessions = vi.fn<() => {
  pending: CaptureSession[];
  completed: CaptureSession[];
  loading: boolean;
  refresh: (page?: number) => Promise<void>;
  completedPage: number;
  hasMoreCompleted: boolean;
  nextCompletedPage: () => void;
  prevCompletedPage: () => void;
  analyzingSessionId: number | null;
}>();

vi.mock('../../hooks/useSessions', () => ({
  useSessions: (...args: unknown[]) => mockUseSessions(...args),
}));

// Mock tauri module
const mockAnalyzeSession = vi.fn<(sessionId: number) => Promise<number>>();
const mockAnalyzeAllPending = vi.fn<() => Promise<number>>();

vi.mock('../../lib/tauri', () => ({
  analyzeSession: (...args: unknown[]) => mockAnalyzeSession(...(args as [number])),
  analyzeAllPending: (...args: unknown[]) => mockAnalyzeAllPending(...(args as [])),
  cancelAnalysis: vi.fn(),
  deleteSession: vi.fn().mockResolvedValue(0),
  getSessionScreenshots: vi.fn().mockResolvedValue([]),
  getScreenshotsDir: vi.fn().mockResolvedValue('/tmp'),
}));

// Mock CollectionDetail
vi.mock('../CollectionDetail', () => ({
  CollectionDetail: ({
    onClose,
    backLabel,
  }: {
    sessionId: number;
    onClose: () => void;
    backLabel?: string;
  }) => (
    <div data-testid="collection-detail">
      <button onClick={onClose}>{backLabel || 'Back to Sessions'}</button>
    </div>
  ),
}));

const pendingSession: CaptureSession = {
  id: 1,
  started_at: '2025-01-01T10:00:00Z',
  ended_at: '2025-01-01T10:30:00Z',
  screenshot_count: 5,
  description: 'Working on auth',
  title: 'Auth Feature',
  unanalyzed_count: 3,
};

const completedSession: CaptureSession = {
  id: 2,
  started_at: '2025-01-01T11:00:00Z',
  ended_at: '2025-01-01T11:30:00Z',
  screenshot_count: 10,
  description: 'Finished testing',
  title: 'Testing Sprint',
  unanalyzed_count: 0,
};

describe('Dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAnalyzeSession.mockResolvedValue(0);
    mockAnalyzeAllPending.mockResolvedValue(0);
  });

  it('renders loading state', () => {
    mockUseSessions.mockReturnValue({
      pending: [],
      completed: [],
      loading: true,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    expect(screen.getByText('Loading sessions...')).toBeInTheDocument();
  });

  it('renders empty state when no sessions', () => {
    mockUseSessions.mockReturnValue({
      pending: [],
      completed: [],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    expect(screen.getByText('No pending sessions. Start a capture to create one.')).toBeInTheDocument();
    expect(screen.getByText('No completed sessions yet.')).toBeInTheDocument();
  });

  it('renders pending sessions with title and counts', () => {
    mockUseSessions.mockReturnValue({
      pending: [pendingSession],
      completed: [],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    expect(screen.getByText('Auth Feature')).toBeInTheDocument();
    expect(screen.getByText('Working on auth')).toBeInTheDocument();
    expect(screen.getByText('5 screenshots')).toBeInTheDocument();
    expect(screen.getByText('3 unanalyzed')).toBeInTheDocument();
    expect(screen.getByText('Analyze')).toBeInTheDocument();
  });

  it('renders completed sessions', () => {
    mockUseSessions.mockReturnValue({
      pending: [],
      completed: [completedSession],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    expect(screen.getByText('Testing Sprint')).toBeInTheDocument();
    expect(screen.getByText('10 screenshots')).toBeInTheDocument();
  });

  it('calls analyzeSession when Analyze button clicked', async () => {
    const user = userEvent.setup();
    mockAnalyzeSession.mockResolvedValue(3);
    mockUseSessions.mockReturnValue({
      pending: [pendingSession],
      completed: [],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Analyze'));
    await waitFor(() => {
      expect(mockAnalyzeSession).toHaveBeenCalledWith(1);
    });
    await waitFor(() => {
      expect(screen.getByText('Analyzed 3 screenshots')).toBeInTheDocument();
    });
  });

  it('calls analyzeAllPending when Analyze All clicked', async () => {
    const user = userEvent.setup();
    mockAnalyzeAllPending.mockResolvedValue(5);
    mockUseSessions.mockReturnValue({
      pending: [pendingSession],
      completed: [],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Analyze All'));
    await waitFor(() => {
      expect(mockAnalyzeAllPending).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByText('Analyzed 5 screenshots')).toBeInTheDocument();
    });
  });

  it('shows error message when analysis fails', async () => {
    const user = userEvent.setup();
    mockAnalyzeSession.mockRejectedValue(new Error('No API key configured'));
    mockUseSessions.mockReturnValue({
      pending: [pendingSession],
      completed: [],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Analyze'));
    await waitFor(() => {
      expect(screen.getByText('Error: No API key configured')).toBeInTheDocument();
    });
  });

  it('opens CollectionDetail when completed session is clicked', async () => {
    const user = userEvent.setup();
    mockUseSessions.mockReturnValue({
      pending: [],
      completed: [completedSession],
      loading: false,
      refresh: mockRefresh,
      completedPage: 0,
      hasMoreCompleted: false,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Testing Sprint'));
    expect(screen.getByTestId('collection-detail')).toBeInTheDocument();
    expect(screen.getByText('Back to Sessions')).toBeInTheDocument();
  });

  it('renders pagination for completed sessions', () => {
    mockUseSessions.mockReturnValue({
      pending: [],
      completed: [completedSession],
      loading: false,
      refresh: mockRefresh,
      completedPage: 1,
      hasMoreCompleted: true,
      nextCompletedPage: mockNextCompletedPage,
      prevCompletedPage: mockPrevCompletedPage,
      analyzingSessionId: null,
    });
    render(<Dashboard />);
    expect(screen.getByText('Page 2')).toBeInTheDocument();
    expect(screen.getByText('Previous')).not.toBeDisabled();
    expect(screen.getByText('Next')).not.toBeDisabled();
  });
});
