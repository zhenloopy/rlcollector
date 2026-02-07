import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Popup } from '../Popup';

describe('Popup', () => {
  const defaultProps = {
    title: 'Confirm Action',
    message: 'Are you sure you want to proceed?',
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders title text', () => {
    render(<Popup {...defaultProps} />);
    expect(screen.getByText('Confirm Action')).toBeInTheDocument();
  });

  it('renders message text', () => {
    render(<Popup {...defaultProps} />);
    expect(screen.getByText('Are you sure you want to proceed?')).toBeInTheDocument();
  });

  it('renders default action buttons', () => {
    render(<Popup {...defaultProps} />);
    expect(screen.getByText('Confirm')).toBeInTheDocument();
    expect(screen.getByText('Cancel')).toBeInTheDocument();
  });

  it('renders custom button labels', () => {
    render(<Popup {...defaultProps} confirmLabel="Yes, delete" cancelLabel="No, keep" />);
    expect(screen.getByText('Yes, delete')).toBeInTheDocument();
    expect(screen.getByText('No, keep')).toBeInTheDocument();
  });

  it('calls onConfirm when confirm clicked', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(<Popup {...defaultProps} onConfirm={onConfirm} />);
    await user.click(screen.getByText('Confirm'));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when cancel clicked', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(<Popup {...defaultProps} onCancel={onCancel} />);
    await user.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when Escape key is pressed', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(<Popup {...defaultProps} onCancel={onCancel} />);
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onConfirm when Enter key is pressed', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(<Popup {...defaultProps} onConfirm={onConfirm} />);
    await user.keyboard('{Enter}');
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('renders overlay backdrop', () => {
    const { container } = render(<Popup {...defaultProps} />);
    const overlay = container.querySelector('.popup-overlay');
    expect(overlay).toBeInTheDocument();
  });

  it('renders popup card', () => {
    const { container } = render(<Popup {...defaultProps} />);
    const card = container.querySelector('.popup-card');
    expect(card).toBeInTheDocument();
  });
});
