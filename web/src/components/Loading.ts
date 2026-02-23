/**
 * Loading state components.
 * Provides spinner and skeleton loaders for consistent loading UX.
 */

/**
 * Render a centered spinner with optional message.
 */
export function renderSpinner(message = 'Loading...'): string {
  return `
    <div class="text-center py-8" role="status" aria-live="polite">
      <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 dark:border-gray-600 border-t-blue-600 dark:border-t-blue-400" aria-hidden="true"></div>
      <p class="mt-2 text-gray-500 dark:text-gray-400">${escapeHtml(message)}</p>
      <span class="sr-only">${escapeHtml(message)}</span>
    </div>
  `;
}

/**
 * Render a skeleton line for text content.
 */
export function renderSkeletonLine(width = 'w-full'): string {
  return `<div class="h-4 bg-gray-200 dark:bg-gray-700 rounded animate-pulse ${width}" aria-hidden="true"></div>`;
}

/**
 * Render a skeleton card matching document/question card layout.
 */
export function renderSkeletonCard(): string {
  return `
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 animate-pulse" aria-hidden="true">
      <div class="flex items-center justify-between mb-3">
        <div class="h-5 bg-gray-200 dark:bg-gray-700 rounded w-1/3"></div>
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-16"></div>
      </div>
      <div class="space-y-2">
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
        <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-2/3"></div>
      </div>
    </div>
  `;
}

/**
 * Render skeleton cards for a list view.
 */
export function renderSkeletonList(count = 3): string {
  return `
    <div class="space-y-4" role="status" aria-label="Loading content">
      <span class="sr-only">Loading content...</span>
      ${Array(count).fill(0).map(() => renderSkeletonCard()).join('')}
    </div>
  `;
}

/**
 * Render a skeleton for document group (header + questions).
 */
export function renderSkeletonDocumentGroup(): string {
  return `
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden animate-pulse" aria-hidden="true">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div class="space-y-2">
            <div class="h-5 bg-gray-300 dark:bg-gray-600 rounded w-48"></div>
            <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-32"></div>
          </div>
          <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-20"></div>
        </div>
      </div>
      <div class="p-4 space-y-3">
        <div class="flex items-start space-x-3">
          <div class="h-6 w-16 bg-gray-200 dark:bg-gray-700 rounded"></div>
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-3/4"></div>
          </div>
        </div>
        <div class="flex items-start space-x-3">
          <div class="h-6 w-16 bg-gray-200 dark:bg-gray-700 rounded"></div>
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-full"></div>
            <div class="h-4 bg-gray-200 dark:bg-gray-700 rounded w-1/2"></div>
          </div>
        </div>
      </div>
    </div>
  `;
}

/**
 * Render skeleton for stats cards on dashboard.
 */
export function renderSkeletonStats(): string {
  return `
    <dl class="grid grid-cols-2 md:grid-cols-4 gap-4 animate-pulse" aria-hidden="true">
      ${Array(4).fill(0).map(() => `
        <div>
          <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-20 mb-2"></div>
          <div class="h-6 bg-gray-300 dark:bg-gray-600 rounded w-12"></div>
        </div>
      `).join('')}
    </dl>
  `;
}

/**
 * Render skeleton for dashboard quick link cards.
 */
export function renderSkeletonDashboardCards(): string {
  return `
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 sm:gap-6 animate-pulse" role="status" aria-label="Loading dashboard">
      <span class="sr-only">Loading dashboard...</span>
      ${Array(3).fill(0).map(() => `
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6" aria-hidden="true">
          <div class="flex items-center space-x-3">
            <div class="h-8 w-8 bg-gray-200 dark:bg-gray-700 rounded"></div>
            <div class="space-y-2">
              <div class="h-4 bg-gray-300 dark:bg-gray-600 rounded w-24"></div>
              <div class="h-3 bg-gray-200 dark:bg-gray-700 rounded w-16"></div>
            </div>
          </div>
          <div class="mt-4 h-8 bg-gray-200 dark:bg-gray-700 rounded w-12"></div>
        </div>
      `).join('')}
    </div>
  `;
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}
