import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CaptureControls } from '../CaptureControls';
import type { CaptureStatus } from '../../types';

const mockStart = vi.fn();
const mockStop = vi.fn();
const mockRefresh = vi.fn();

const mockUseCapture = vi.fn<() => {
  status: CaptureStatus;
  start: (intervalMs?: number, title?: string, description?: string) => Promise<void>;
  stop: () => Promise<void>;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}>();

vi.mock('../../hooks/useCapture', () => ({
  useCapture: (...args: unknown[]) => mockUseCapture(...args),
}));

describe('CaptureControls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders capture status indicator when stopped', () => {
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Stopped')).toBeInTheDocument();
  });

  it('renders capture status indicator when recording', () => {
    mockUseCapture.mockReturnValue({
      status: { active: true, interval_ms: 30000, count: 5, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Recording')).toBeInTheDocument();
  });

  it('shows "Start Capture" button when not capturing', () => {
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Start Capture')).toBeInTheDocument();
  });

  it('shows "Stop Capture" button when capturing', () => {
    mockUseCapture.mockReturnValue({
      status: { active: true, interval_ms: 30000, count: 3, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Stop Capture')).toBeInTheDocument();
  });

  it('disables Start Capture when title is empty', () => {
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Start Capture')).toBeDisabled();
  });

  it('enables Start Capture when title is provided', async () => {
    const user = userEvent.setup();
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);

    const titleInput = screen.getByPlaceholderText('e.g. Auth page implementation');
    await user.type(titleInput, 'My Session');
    expect(screen.getByText('Start Capture')).not.toBeDisabled();
  });

  it('calls start with title when Start Capture button is clicked', async () => {
    const user = userEvent.setup();
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);

    const titleInput = screen.getByPlaceholderText('e.g. Auth page implementation');
    await user.type(titleInput, 'My Session');
    await user.click(screen.getByText('Start Capture'));
    expect(mockStart).toHaveBeenCalledWith(30000, 'My Session', undefined);
  });

  it('calls stop when Stop Capture button is clicked', async () => {
    const user = userEvent.setup();
    mockUseCapture.mockReturnValue({
      status: { active: true, interval_ms: 30000, count: 5, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    await user.click(screen.getByText('Stop Capture'));
    expect(mockStop).toHaveBeenCalled();
  });

  it('shows capture count when active', () => {
    mockUseCapture.mockReturnValue({
      status: { active: true, interval_ms: 30000, count: 42, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: null,
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText(/42 captures/)).toBeInTheDocument();
  });

  it('displays error message when error is set', () => {
    mockUseCapture.mockReturnValue({
      status: { active: false, interval_ms: 30000, count: 0, monitor_mode: "default", monitors_captured: 1 },
      start: mockStart,
      stop: mockStop,
      loading: false,
      error: 'Failed to create screenshots directory',
      refresh: mockRefresh,
    });
    render(<CaptureControls />);
    expect(screen.getByText('Failed to create screenshots directory')).toBeInTheDocument();
  });
});
