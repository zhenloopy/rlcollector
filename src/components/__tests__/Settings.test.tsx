import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Settings } from '../Settings';

const mockGetSetting = vi.fn<(key: string) => Promise<string | null>>();
const mockUpdateSetting = vi.fn<(key: string, value: string) => Promise<void>>();
const mockGetLogPath = vi.fn<() => Promise<string>>();

vi.mock('../../lib/tauri', () => ({
  getSetting: (key: string) => mockGetSetting(key),
  updateSetting: (key: string, value: string) => mockUpdateSetting(key, value),
  getLogPath: () => mockGetLogPath(),
}));

const mockOpenPath = vi.fn<(path: string) => Promise<void>>();

vi.mock('@tauri-apps/plugin-opener', () => ({
  openPath: (path: string) => mockOpenPath(path),
}));

describe('Settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSetting.mockResolvedValue(null);
    mockUpdateSetting.mockResolvedValue(undefined);
    mockGetLogPath.mockResolvedValue('C:\\Users\\test\\AppData\\Roaming\\com.rlmarket.rlcollector\\logs');
    mockOpenPath.mockResolvedValue(undefined);
  });

  it('renders API key input', async () => {
    render(<Settings />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText('sk-ant-...')).toBeInTheDocument();
    });
  });

  it('renders Claude API Key label', async () => {
    render(<Settings />);
    await waitFor(() => {
      expect(screen.getByText('Claude API Key:')).toBeInTheDocument();
    });
  });

  it('loads existing API key on mount', async () => {
    mockGetSetting.mockResolvedValue('sk-ant-test-key');
    render(<Settings />);
    await waitFor(() => {
      expect(mockGetSetting).toHaveBeenCalledWith('ai_api_key');
    });
  });

  it('shows saved message after save', async () => {
    const user = userEvent.setup();
    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('sk-ant-...')).toBeInTheDocument();
    });

    const input = screen.getByPlaceholderText('sk-ant-...');
    await user.clear(input);
    await user.type(input, 'sk-ant-new-key');
    await user.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(screen.getByText('Saved')).toBeInTheDocument();
    });
    expect(mockUpdateSetting).toHaveBeenCalledWith('ai_api_key', 'sk-ant-new-key');
  });

  it('renders Open Log Directory button', async () => {
    render(<Settings />);
    await waitFor(() => {
      expect(screen.getByText('Open Log Directory')).toBeInTheDocument();
    });
  });

  it('opens log directory when button is clicked', async () => {
    const user = userEvent.setup();
    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByText('Open Log Directory')).toBeInTheDocument();
    });

    await user.click(screen.getByText('Open Log Directory'));

    await waitFor(() => {
      expect(mockGetLogPath).toHaveBeenCalled();
      expect(mockOpenPath).toHaveBeenCalledWith('C:\\Users\\test\\AppData\\Roaming\\com.rlmarket.rlcollector\\logs');
    });
  });
});
