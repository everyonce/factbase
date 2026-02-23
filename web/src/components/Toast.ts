/**
 * Toast notification system.
 * Provides non-blocking notifications for action feedback.
 */

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastOptions {
  message: string;
  type?: ToastType;
  duration?: number; // ms, 0 for persistent
  action?: {
    label: string;
    onClick: () => void;
  };
}

interface ToastItem {
  id: string;
  options: ToastOptions;
  timeoutId?: number;
}

const toasts: ToastItem[] = [];
let containerId = 'toast-container';

/**
 * Initialize toast container. Call once at app startup.
 */
export function initToasts(): void {
  if (document.getElementById(containerId)) return;

  const container = document.createElement('div');
  container.id = containerId;
  container.className = 'fixed bottom-4 right-4 z-50 flex flex-col space-y-2 max-w-sm';
  container.setAttribute('role', 'region');
  container.setAttribute('aria-label', 'Notifications');
  document.body.appendChild(container);
}

/**
 * Show a toast notification.
 */
export function showToast(options: ToastOptions): string {
  initToasts();

  const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  const duration = options.duration ?? 5000;

  const item: ToastItem = { id, options };

  if (duration > 0) {
    item.timeoutId = window.setTimeout(() => dismissToast(id), duration);
  }

  toasts.push(item);
  renderToasts();

  return id;
}

/**
 * Dismiss a toast by ID.
 */
export function dismissToast(id: string): void {
  const index = toasts.findIndex(t => t.id === id);
  if (index === -1) return;

  const item = toasts[index];
  if (item.timeoutId) {
    clearTimeout(item.timeoutId);
  }

  toasts.splice(index, 1);
  renderToasts();
}

/**
 * Dismiss all toasts.
 */
export function dismissAllToasts(): void {
  toasts.forEach(t => {
    if (t.timeoutId) clearTimeout(t.timeoutId);
  });
  toasts.length = 0;
  renderToasts();
}

/**
 * Convenience methods for common toast types.
 */
export const toast = {
  success: (message: string, options?: Partial<ToastOptions>) =>
    showToast({ message, type: 'success', ...options }),

  error: (message: string, options?: Partial<ToastOptions>) =>
    showToast({ message, type: 'error', duration: 0, ...options }),

  info: (message: string, options?: Partial<ToastOptions>) =>
    showToast({ message, type: 'info', ...options }),

  warning: (message: string, options?: Partial<ToastOptions>) =>
    showToast({ message, type: 'warning', ...options }),
};

function renderToasts(): void {
  const container = document.getElementById(containerId);
  if (!container) return;

  container.innerHTML = toasts.map(item => renderToastItem(item)).join('');

  // Setup dismiss handlers
  toasts.forEach(item => {
    const dismissBtn = document.getElementById(`${item.id}-dismiss`);
    dismissBtn?.addEventListener('click', () => dismissToast(item.id));

    if (item.options.action) {
      const actionBtn = document.getElementById(`${item.id}-action`);
      actionBtn?.addEventListener('click', () => {
        item.options.action?.onClick();
        dismissToast(item.id);
      });
    }
  });
}

function renderToastItem(item: ToastItem): string {
  const { id, options } = item;
  const { message, type = 'info', action } = options;

  const colors = getToastColors(type);
  const icon = getToastIcon(type);

  return `
    <div
      id="${id}"
      class="flex items-start p-4 rounded-lg shadow-lg ${colors.bg} ${colors.border} border animate-slide-in"
      role="alert"
      aria-live="polite"
    >
      <div class="flex-shrink-0 ${colors.icon}">
        ${icon}
      </div>
      <div class="ml-3 flex-1">
        <p class="text-sm font-medium ${colors.text}">${escapeHtml(message)}</p>
        ${action ? `
          <button
            id="${id}-action"
            class="mt-2 text-sm font-medium ${colors.action} hover:underline"
          >
            ${escapeHtml(action.label)}
          </button>
        ` : ''}
      </div>
      <button
        id="${id}-dismiss"
        class="ml-4 flex-shrink-0 ${colors.dismiss} hover:opacity-75"
        aria-label="Dismiss"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
        </svg>
      </button>
    </div>
  `;
}

function getToastColors(type: ToastType): {
  bg: string;
  border: string;
  text: string;
  icon: string;
  action: string;
  dismiss: string;
} {
  switch (type) {
    case 'success':
      return {
        bg: 'bg-green-50 dark:bg-green-900/30',
        border: 'border-green-200 dark:border-green-800',
        text: 'text-green-800 dark:text-green-200',
        icon: 'text-green-500 dark:text-green-400',
        action: 'text-green-700 dark:text-green-300',
        dismiss: 'text-green-500 dark:text-green-400',
      };
    case 'error':
      return {
        bg: 'bg-red-50 dark:bg-red-900/30',
        border: 'border-red-200 dark:border-red-800',
        text: 'text-red-800 dark:text-red-200',
        icon: 'text-red-500 dark:text-red-400',
        action: 'text-red-700 dark:text-red-300',
        dismiss: 'text-red-500 dark:text-red-400',
      };
    case 'warning':
      return {
        bg: 'bg-amber-50 dark:bg-amber-900/30',
        border: 'border-amber-200 dark:border-amber-800',
        text: 'text-amber-800 dark:text-amber-200',
        icon: 'text-amber-500 dark:text-amber-400',
        action: 'text-amber-700 dark:text-amber-300',
        dismiss: 'text-amber-500 dark:text-amber-400',
      };
    case 'info':
    default:
      return {
        bg: 'bg-blue-50 dark:bg-blue-900/30',
        border: 'border-blue-200 dark:border-blue-800',
        text: 'text-blue-800 dark:text-blue-200',
        icon: 'text-blue-500 dark:text-blue-400',
        action: 'text-blue-700 dark:text-blue-300',
        dismiss: 'text-blue-500 dark:text-blue-400',
      };
  }
}

function getToastIcon(type: ToastType): string {
  switch (type) {
    case 'success':
      return `
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
        </svg>
      `;
    case 'error':
      return `
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
        </svg>
      `;
    case 'warning':
      return `
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
      `;
    case 'info':
    default:
      return `
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
      `;
  }
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}
