/**
 * Error state components.
 * Provides consistent error display with retry functionality.
 */

export interface ErrorDisplayOptions {
  title?: string;
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
}

/**
 * Render a centered error display with optional retry button.
 */
export function renderError(options: ErrorDisplayOptions): string {
  const { title = 'Error', message, onRetry, retryLabel = 'Retry' } = options;
  const retryId = onRetry ? `retry-${Date.now()}` : null;

  return `
    <div class="text-center py-8">
      <div class="inline-flex items-center justify-center w-12 h-12 rounded-full bg-red-100 dark:bg-red-900/30 mb-4">
        <svg class="w-6 h-6 text-red-600 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
        </svg>
      </div>
      <p class="font-medium text-red-600 dark:text-red-400">${escapeHtml(title)}</p>
      <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">${escapeHtml(message)}</p>
      ${retryId ? `
        <button id="${retryId}" class="mt-4 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 text-sm transition-colors">
          ${escapeHtml(retryLabel)}
        </button>
      ` : ''}
    </div>
  `;
}

/**
 * Render an inline error message (for forms, etc).
 */
export function renderInlineError(message: string): string {
  return `
    <div class="flex items-center space-x-2 text-sm text-red-600 dark:text-red-400 mt-2">
      <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
      </svg>
      <span>${escapeHtml(message)}</span>
    </div>
  `;
}

/**
 * Render an error banner (for page-level errors that don't block content).
 */
export function renderErrorBanner(message: string, dismissId?: string): string {
  return `
    <div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-3 mb-4">
      <div class="flex items-start">
        <svg class="w-5 h-5 text-red-600 dark:text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
        </svg>
        <div class="ml-3 flex-1">
          <p class="text-sm text-red-700 dark:text-red-300">${escapeHtml(message)}</p>
        </div>
        ${dismissId ? `
          <button id="${dismissId}" class="ml-3 text-red-600 dark:text-red-400 hover:text-red-800 dark:hover:text-red-300">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        ` : ''}
      </div>
    </div>
  `;
}

/**
 * Setup retry button handler after rendering.
 * Call this after inserting renderError() HTML into DOM.
 */
export function setupRetryHandler(onRetry: () => void): void {
  // Find the most recently added retry button
  const buttons = document.querySelectorAll('[id^="retry-"]');
  const lastButton = buttons[buttons.length - 1];
  lastButton?.addEventListener('click', onRetry);
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}
