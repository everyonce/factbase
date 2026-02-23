/**
 * Component unit tests.
 * Tests for Loading, Error, and Toast components.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { screen } from '@testing-library/dom';
import {
  renderSpinner,
  renderSkeletonCard,
  renderSkeletonList,
  renderSkeletonDocumentGroup,
} from './components/Loading';
import {
  renderError,
  renderInlineError,
  renderErrorBanner,
  setupRetryHandler,
} from './components/Error';
import {
  initToasts,
  showToast,
  dismissToast,
  dismissAllToasts,
  toast,
} from './components/Toast';

describe('Loading components', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  describe('renderSpinner', () => {
    it('should render spinner with default message', () => {
      container.innerHTML = renderSpinner();
      expect(container.textContent).toContain('Loading...');
    });

    it('should render spinner with custom message', () => {
      container.innerHTML = renderSpinner('Fetching data...');
      expect(container.textContent).toContain('Fetching data...');
    });

    it('should have accessible role and aria-live', () => {
      container.innerHTML = renderSpinner();
      const status = container.querySelector('[role="status"]');
      expect(status).not.toBeNull();
      expect(status?.getAttribute('aria-live')).toBe('polite');
    });

    it('should have screen reader text', () => {
      container.innerHTML = renderSpinner('Loading items');
      const srOnly = container.querySelector('.sr-only');
      expect(srOnly?.textContent).toBe('Loading items');
    });
  });

  describe('renderSkeletonCard', () => {
    it('should render skeleton card with aria-hidden', () => {
      container.innerHTML = renderSkeletonCard();
      const card = container.querySelector('[aria-hidden="true"]');
      expect(card).not.toBeNull();
    });

    it('should have animate-pulse class', () => {
      container.innerHTML = renderSkeletonCard();
      const card = container.querySelector('.animate-pulse');
      expect(card).not.toBeNull();
    });
  });

  describe('renderSkeletonList', () => {
    it('should render default 3 skeleton cards', () => {
      container.innerHTML = renderSkeletonList();
      const cards = container.querySelectorAll('.animate-pulse');
      // List wrapper + 3 cards
      expect(cards.length).toBeGreaterThanOrEqual(3);
    });

    it('should render specified number of cards', () => {
      container.innerHTML = renderSkeletonList(5);
      const cards = container.querySelectorAll('[aria-hidden="true"]');
      expect(cards.length).toBe(5);
    });

    it('should have accessible label', () => {
      container.innerHTML = renderSkeletonList();
      const list = container.querySelector('[aria-label="Loading content"]');
      expect(list).not.toBeNull();
    });
  });

  describe('renderSkeletonDocumentGroup', () => {
    it('should render document group skeleton', () => {
      container.innerHTML = renderSkeletonDocumentGroup();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).not.toBeNull();
    });
  });
});

describe('Error components', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  describe('renderError', () => {
    it('should render error with default title', () => {
      container.innerHTML = renderError({ message: 'Something went wrong' });
      expect(container.textContent).toContain('Error');
      expect(container.textContent).toContain('Something went wrong');
    });

    it('should render error with custom title', () => {
      container.innerHTML = renderError({ title: 'Network Error', message: 'Failed to connect' });
      expect(container.textContent).toContain('Network Error');
      expect(container.textContent).toContain('Failed to connect');
    });

    it('should render retry button when onRetry provided', () => {
      container.innerHTML = renderError({
        message: 'Failed',
        onRetry: () => {},
      });
      const button = container.querySelector('button');
      expect(button).not.toBeNull();
      expect(button?.textContent).toContain('Retry');
    });

    it('should not render retry button when onRetry not provided', () => {
      container.innerHTML = renderError({ message: 'Failed' });
      const button = container.querySelector('button');
      expect(button).toBeNull();
    });

    it('should render custom retry label', () => {
      container.innerHTML = renderError({
        message: 'Failed',
        onRetry: () => {},
        retryLabel: 'Try Again',
      });
      const button = container.querySelector('button');
      expect(button?.textContent).toContain('Try Again');
    });

    it('should escape HTML in message', () => {
      container.innerHTML = renderError({ message: '<script>alert("xss")</script>' });
      expect(container.innerHTML).not.toContain('<script>');
      expect(container.textContent).toContain('<script>');
    });
  });

  describe('renderInlineError', () => {
    it('should render inline error message', () => {
      container.innerHTML = renderInlineError('Invalid input');
      expect(container.textContent).toContain('Invalid input');
    });

    it('should have error styling', () => {
      container.innerHTML = renderInlineError('Error');
      const errorDiv = container.querySelector('.text-red-600');
      expect(errorDiv).not.toBeNull();
    });
  });

  describe('renderErrorBanner', () => {
    it('should render error banner', () => {
      container.innerHTML = renderErrorBanner('Operation failed');
      expect(container.textContent).toContain('Operation failed');
    });

    it('should render dismiss button when dismissId provided', () => {
      container.innerHTML = renderErrorBanner('Error', 'dismiss-btn');
      const button = container.querySelector('#dismiss-btn');
      expect(button).not.toBeNull();
    });

    it('should not render dismiss button when dismissId not provided', () => {
      container.innerHTML = renderErrorBanner('Error');
      const buttons = container.querySelectorAll('button');
      expect(buttons.length).toBe(0);
    });
  });

  describe('setupRetryHandler', () => {
    it('should attach click handler to retry button', () => {
      const onRetry = vi.fn();
      container.innerHTML = renderError({ message: 'Failed', onRetry });
      setupRetryHandler(onRetry);

      const button = container.querySelector('button');
      button?.click();

      expect(onRetry).toHaveBeenCalledTimes(1);
    });
  });
});

describe('Toast components', () => {
  beforeEach(() => {
    // Clean up any existing toast container
    const existing = document.getElementById('toast-container');
    if (existing) existing.remove();
    vi.useFakeTimers();
  });

  afterEach(() => {
    dismissAllToasts();
    const container = document.getElementById('toast-container');
    if (container) container.remove();
    vi.useRealTimers();
  });

  describe('initToasts', () => {
    it('should create toast container', () => {
      initToasts();
      const container = document.getElementById('toast-container');
      expect(container).not.toBeNull();
    });

    it('should not create duplicate containers', () => {
      initToasts();
      initToasts();
      const containers = document.querySelectorAll('#toast-container');
      expect(containers.length).toBe(1);
    });

    it('should have accessible attributes', () => {
      initToasts();
      const container = document.getElementById('toast-container');
      expect(container?.getAttribute('role')).toBe('region');
      expect(container?.getAttribute('aria-label')).toBe('Notifications');
    });
  });

  describe('showToast', () => {
    it('should show toast with message', () => {
      showToast({ message: 'Test message' });
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Test message');
    });

    it('should return toast ID', () => {
      const id = showToast({ message: 'Test' });
      expect(id).toMatch(/^toast-\d+-[a-z0-9]+$/);
    });

    it('should auto-dismiss after duration', () => {
      showToast({ message: 'Test', duration: 1000 });
      expect(document.getElementById('toast-container')?.textContent).toContain('Test');

      vi.advanceTimersByTime(1000);
      expect(document.getElementById('toast-container')?.textContent).not.toContain('Test');
    });

    it('should not auto-dismiss when duration is 0', () => {
      showToast({ message: 'Persistent', duration: 0 });
      vi.advanceTimersByTime(10000);
      expect(document.getElementById('toast-container')?.textContent).toContain('Persistent');
    });
  });

  describe('dismissToast', () => {
    it('should remove toast by ID', () => {
      const id = showToast({ message: 'To dismiss', duration: 0 });
      expect(document.getElementById('toast-container')?.textContent).toContain('To dismiss');

      dismissToast(id);
      expect(document.getElementById('toast-container')?.textContent).not.toContain('To dismiss');
    });

    it('should handle non-existent ID gracefully', () => {
      expect(() => dismissToast('non-existent')).not.toThrow();
    });
  });

  describe('dismissAllToasts', () => {
    it('should remove all toasts', () => {
      showToast({ message: 'Toast 1', duration: 0 });
      showToast({ message: 'Toast 2', duration: 0 });
      showToast({ message: 'Toast 3', duration: 0 });

      const container = document.getElementById('toast-container');
      expect(container?.children.length).toBe(3);

      dismissAllToasts();
      expect(container?.children.length).toBe(0);
    });
  });

  describe('toast convenience methods', () => {
    it('should show success toast', () => {
      toast.success('Success!');
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Success!');
      expect(container?.innerHTML).toContain('bg-green-50');
    });

    it('should show error toast with persistent duration', () => {
      toast.error('Error!');
      vi.advanceTimersByTime(10000);
      // Error toasts should persist (duration: 0)
      expect(document.getElementById('toast-container')?.textContent).toContain('Error!');
    });

    it('should show info toast', () => {
      toast.info('Info message');
      const container = document.getElementById('toast-container');
      expect(container?.innerHTML).toContain('bg-blue-50');
    });

    it('should show warning toast', () => {
      toast.warning('Warning!');
      const container = document.getElementById('toast-container');
      expect(container?.innerHTML).toContain('bg-amber-50');
    });
  });

  describe('toast with action', () => {
    it('should render action button', () => {
      showToast({
        message: 'With action',
        action: { label: 'Undo', onClick: () => {} },
        duration: 0,
      });
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Undo');
    });

    it('should call action onClick and dismiss', () => {
      const onClick = vi.fn();
      const id = showToast({
        message: 'With action',
        action: { label: 'Undo', onClick },
        duration: 0,
      });

      const actionBtn = document.getElementById(`${id}-action`);
      actionBtn?.click();

      expect(onClick).toHaveBeenCalledTimes(1);
      // Toast should be dismissed after action
      expect(document.getElementById('toast-container')?.textContent).not.toContain('With action');
    });
  });
});
