import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TaskDetail } from '../TaskDetail';
import type { Task } from '../../types';

const mockGetTask = vi.fn<(id: number) => Promise<Task>>();
const mockUpdateTask = vi.fn<(id: number, update: Record<string, unknown>) => Promise<void>>();

vi.mock('../../lib/tauri', () => ({
  getTask: (id: number) => mockGetTask(id),
  updateTask: (id: number, update: Record<string, unknown>) => mockUpdateTask(id, update),
}));

const sampleTask: Task = {
  id: 1,
  title: 'Test Task',
  description: 'A detailed description',
  category: 'coding',
  started_at: '2025-01-01T10:00:00Z',
  ended_at: '2025-01-01T11:00:00Z',
  ai_reasoning: 'AI detected coding activity in VS Code',
  user_verified: false,
  metadata: null,
};

const verifiedTask: Task = {
  ...sampleTask,
  id: 2,
  user_verified: true,
};

describe('TaskDetail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetTask.mockResolvedValue(sampleTask);
    mockUpdateTask.mockResolvedValue(undefined);
  });

  it('shows loading state initially', () => {
    // Make the promise hang
    mockGetTask.mockReturnValue(new Promise(() => {}));
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    expect(screen.getByText('Loading task...')).toBeInTheDocument();
  });

  it('renders task title, description, category, timestamps', async () => {
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Test Task')).toBeInTheDocument();
    });
    expect(screen.getByText('A detailed description')).toBeInTheDocument();
    expect(screen.getByText('coding')).toBeInTheDocument();
  });

  it('shows AI reasoning section when present', async () => {
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('AI Reasoning')).toBeInTheDocument();
    });
    expect(screen.getByText('AI detected coding activity in VS Code')).toBeInTheDocument();
  });

  it('shows "Not verified" badge when user_verified is false', async () => {
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Not verified')).toBeInTheDocument();
    });
  });

  it('shows "Verified" badge when user_verified is true', async () => {
    mockGetTask.mockResolvedValue(verifiedTask);
    render(<TaskDetail taskId={2} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Verified')).toBeInTheDocument();
    });
  });

  it('back button calls onClose callback', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<TaskDetail taskId={1} onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText('Back to Tasks')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Back to Tasks'));
    expect(onClose).toHaveBeenCalled();
  });

  it('shows edit form when edit button clicked', async () => {
    const user = userEvent.setup();
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Edit'));
    // Should now show input fields
    expect(screen.getByDisplayValue('Test Task')).toBeInTheDocument();
    expect(screen.getByDisplayValue('A detailed description')).toBeInTheDocument();
    expect(screen.getByDisplayValue('coding')).toBeInTheDocument();
    expect(screen.getByText('Save')).toBeInTheDocument();
    expect(screen.getByText('Cancel')).toBeInTheDocument();
  });

  it('calls updateTask on save', async () => {
    const user = userEvent.setup();
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Edit'));

    const titleInput = screen.getByDisplayValue('Test Task');
    await user.clear(titleInput);
    await user.type(titleInput, 'Updated Title');
    await user.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(1, {
        title: 'Updated Title',
        description: 'A detailed description',
        category: 'coding',
      });
    });
  });

  it('toggles verify status when verify button clicked', async () => {
    const user = userEvent.setup();
    render(<TaskDetail taskId={1} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Mark as Verified')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Mark as Verified'));
    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(1, { user_verified: true });
    });
  });

  it('fetches task on mount with provided taskId', async () => {
    render(<TaskDetail taskId={42} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(mockGetTask).toHaveBeenCalledWith(42);
    });
  });
});
