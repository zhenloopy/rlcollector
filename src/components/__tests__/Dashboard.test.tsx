import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Dashboard } from '../Dashboard';
import type { Task } from '../../types';

// Mock useTasks hook
const mockRemove = vi.fn();
const mockUpdate = vi.fn();
const mockRefresh = vi.fn();

const mockUseTasks = vi.fn<() => {
  tasks: Task[];
  loading: boolean;
  remove: (id: number) => Promise<void>;
  update: (id: number, fields: Record<string, unknown>) => Promise<void>;
  refresh: () => Promise<void>;
  page: number;
  hasMore: boolean;
  nextPage: () => void;
  prevPage: () => void;
}>();

vi.mock('../../hooks/useTasks', () => ({
  useTasks: (...args: unknown[]) => mockUseTasks(...args),
}));

// Mock tauri module - including getTask for TaskDetail rendering
const mockGetTask = vi.fn();
const mockUpdateTask = vi.fn();
const mockAnalyzePending = vi.fn<() => Promise<number>>();

vi.mock('../../lib/tauri', () => ({
  getTasks: vi.fn(),
  updateTask: (...args: unknown[]) => mockUpdateTask(...args),
  deleteTask: vi.fn(),
  getTask: (...args: unknown[]) => mockGetTask(...args),
  analyzePending: (...args: unknown[]) => mockAnalyzePending(...(args as [])),
}));

const sampleTask: Task = {
  id: 1,
  title: 'Test Task',
  description: 'A test task description',
  category: 'coding',
  started_at: '2025-01-01T10:00:00Z',
  ended_at: null,
  ai_reasoning: null,
  user_verified: false,
  metadata: null,
};

const sampleTask2: Task = {
  id: 2,
  title: 'Another Task',
  description: null,
  category: 'browsing',
  started_at: '2025-01-01T11:00:00Z',
  ended_at: '2025-01-01T12:00:00Z',
  ai_reasoning: 'AI detected browsing activity',
  user_verified: true,
  metadata: null,
};

describe('Dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetTask.mockResolvedValue(sampleTask);
    mockUpdateTask.mockResolvedValue(undefined);
    mockAnalyzePending.mockResolvedValue(0);
  });

  it('renders loading state', () => {
    mockUseTasks.mockReturnValue({
      tasks: [],
      loading: true,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    expect(screen.getByText('Loading tasks...')).toBeInTheDocument();
  });

  it('renders "No tasks" message when task list is empty', () => {
    mockUseTasks.mockReturnValue({
      tasks: [],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    expect(screen.getByText('No tasks recorded yet. Start capturing to begin.')).toBeInTheDocument();
  });

  it('renders task table when tasks exist', () => {
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask, sampleTask2],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    expect(screen.getByText('Test Task')).toBeInTheDocument();
    expect(screen.getByText('Another Task')).toBeInTheDocument();
    expect(screen.getByText('coding')).toBeInTheDocument();
    expect(screen.getByText('browsing')).toBeInTheDocument();
  });

  it('calls remove when delete button clicked', async () => {
    const user = userEvent.setup();
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    const deleteButton = screen.getByText('Delete');
    await user.click(deleteButton);
    expect(mockRemove).toHaveBeenCalledWith(1);
  });

  it('opens TaskDetail when a task row is clicked', async () => {
    const user = userEvent.setup();
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    const taskTitle = screen.getByText('Test Task');
    await user.click(taskTitle);
    // After clicking, TaskDetail should be rendered (we'll check for back button)
    await waitFor(() => {
      expect(screen.getByText('Back to Tasks')).toBeInTheDocument();
    });
  });

  it('renders pagination controls', () => {
    const mockNextPage = vi.fn();
    const mockPrevPage = vi.fn();
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: true,
      nextPage: mockNextPage,
      prevPage: mockPrevPage,
    });
    render(<Dashboard />);
    expect(screen.getByText('Page 1')).toBeInTheDocument();
    expect(screen.getByText('Next')).toBeInTheDocument();
    expect(screen.getByText('Previous')).toBeInTheDocument();
  });

  it('calls nextPage when Next button is clicked', async () => {
    const user = userEvent.setup();
    const mockNextPage = vi.fn();
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: true,
      nextPage: mockNextPage,
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Next'));
    expect(mockNextPage).toHaveBeenCalled();
  });

  it('disables Previous button on first page', () => {
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: true,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    expect(screen.getByText('Previous')).toBeDisabled();
  });

  it('disables Next button when there are no more pages', () => {
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    expect(screen.getByText('Next')).toBeDisabled();
  });

  it('shows success message after analyzing pending screenshots', async () => {
    const user = userEvent.setup();
    mockAnalyzePending.mockResolvedValue(3);
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Analyze Pending'));
    await waitFor(() => {
      expect(screen.getByText('Analyzed 3 screenshots')).toBeInTheDocument();
    });
  });

  it('shows error message when analysis fails', async () => {
    const user = userEvent.setup();
    mockAnalyzePending.mockRejectedValue(new Error('No API key configured'));
    mockUseTasks.mockReturnValue({
      tasks: [sampleTask],
      loading: false,
      remove: mockRemove,
      update: mockUpdate,
      refresh: mockRefresh,
      page: 0,
      hasMore: false,
      nextPage: vi.fn(),
      prevPage: vi.fn(),
    });
    render(<Dashboard />);
    await user.click(screen.getByText('Analyze Pending'));
    await waitFor(() => {
      expect(screen.getByText('Error: No API key configured')).toBeInTheDocument();
    });
  });
});
