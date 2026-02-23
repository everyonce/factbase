/**
 * Orphans page component.
 * Lists orphaned facts with assignment interface.
 */

import { api, Repository, ApiRequestError, OrphansResponse } from '../api';
import { renderOrphanCard } from '../components/OrphanCard';
import { openPreview, cleanupPreview } from '../components/DocumentPreview';
import { renderSkeletonList } from '../components/Loading';
import { renderError, setupRetryHandler } from '../components/Error';
import { toast } from '../components/Toast';

interface OrphansState {
  data: OrphansResponse | null;
  repos: Repository[];
  loading: boolean;
  error: string | null;
  selectedRepo: string;
  successMessage: string | null;
  bulkMode: boolean;
  selectedLines: Set<number>;
}

const state: OrphansState = {
  data: null,
  repos: [],
  loading: true,
  error: null,
  selectedRepo: '',
  successMessage: null,
  bulkMode: false,
  selectedLines: new Set(),
};

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

async function fetchRepos(): Promise<void> {
  try {
    const response = await api.getRepositories();
    state.repos = response.repositories;
    // Auto-select first repo if none selected
    if (!state.selectedRepo && state.repos.length > 0) {
      state.selectedRepo = state.repos[0].id;
    }
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load repositories';
    }
  }
}

async function fetchOrphans(): Promise<void> {
  if (!state.selectedRepo) {
    state.data = null;
    state.loading = false;
    updateUI();
    return;
  }

  state.loading = true;
  state.error = null;
  updateUI();

  try {
    const data = await api.getOrphans(state.selectedRepo);
    state.data = data as OrphansResponse;
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load orphans';
    }
    state.data = null;
  } finally {
    state.loading = false;
    updateUI();
  }
}

async function assignOrphan(lineNumber: number, target: string): Promise<void> {
  if (!state.selectedRepo) return;

  try {
    await api.assignOrphan(state.selectedRepo, lineNumber, target);
    toast.success(target === 'dismiss' ? 'Orphan dismissed' : `Orphan assigned to ${target}`);
    await fetchOrphans();
  } catch (e) {
    if (e instanceof ApiRequestError) {
      throw e;
    }
    throw new Error('Failed to assign orphan');
  }
}

async function bulkAssign(target: string): Promise<void> {
  if (!state.selectedRepo || state.selectedLines.size === 0) return;

  const lines = Array.from(state.selectedLines);
  let successCount = 0;
  const errors: string[] = [];

  for (const lineNumber of lines) {
    try {
      await api.assignOrphan(state.selectedRepo, lineNumber, target);
      successCount++;
    } catch (e) {
      errors.push(`Line ${lineNumber}: ${e instanceof Error ? e.message : 'Unknown error'}`);
    }
  }

  if (successCount > 0) {
    toast.success(target === 'dismiss'
      ? `Dismissed ${successCount} orphan(s)`
      : `Assigned ${successCount} orphan(s) to ${target}`);
  }
  if (errors.length > 0) {
    toast.error(`Some assignments failed: ${errors.join('; ')}`);
  }

  state.selectedLines.clear();
  state.bulkMode = false;
  await fetchOrphans();
}

function renderRepoSelector(): string {
  const options = state.repos.map(r =>
    `<option value="${escapeHtml(r.id)}" ${state.selectedRepo === r.id ? 'selected' : ''}>${escapeHtml(r.name)}</option>`
  ).join('');

  return `
    <div>
      <label for="repo-select" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
      <select id="repo-select" class="block w-full sm:w-64 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
        ${state.repos.length === 0 ? '<option value="">No repositories</option>' : ''}
        ${options}
      </select>
    </div>
  `;
}

function renderSummary(): string {
  if (!state.data) return '';

  return `
    <div class="flex items-center justify-between text-sm">
      <div class="text-gray-600 dark:text-gray-400">
        ${state.data.unanswered} pending / ${state.data.total} total orphans
      </div>
      ${state.data.answered > 0 ? `<div class="text-green-600 dark:text-green-400">${state.data.answered} assigned</div>` : ''}
    </div>
  `;
}

function renderBulkActions(): string {
  if (!state.bulkMode || state.selectedLines.size === 0) {
    return '';
  }

  return `
    <div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <span class="text-sm text-blue-700 dark:text-blue-300">
          ${state.selectedLines.size} orphan(s) selected
        </span>
        <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
          <input
            type="text"
            id="bulk-target-input"
            class="text-sm rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500"
            placeholder="Document ID"
          >
          <div class="flex gap-2">
            <button
              id="bulk-assign-btn"
              class="flex-1 sm:flex-none px-3 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700"
            >
              Assign All
            </button>
            <button
              id="bulk-dismiss-btn"
              class="flex-1 sm:flex-none px-3 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 rounded-md hover:bg-gray-200 dark:hover:bg-gray-600"
            >
              Dismiss All
            </button>
          </div>
        </div>
      </div>
    </div>
  `;
}

function updateUI(): void {
  const content = document.getElementById('orphans-content');
  if (!content) return;

  // Update summary
  const summaryEl = document.getElementById('orphans-summary');
  if (summaryEl && state.data) {
    summaryEl.innerHTML = renderSummary();
  }

  // Update bulk actions
  const bulkActionsEl = document.getElementById('bulk-actions-container');
  if (bulkActionsEl) {
    bulkActionsEl.innerHTML = renderBulkActions();
    setupBulkActionHandlers();
  }

  // Clear inline message area (using toasts now)
  const messageEl = document.getElementById('orphans-message');
  if (messageEl) {
    messageEl.innerHTML = '';
  }

  if (state.loading) {
    content.innerHTML = renderSkeletonList(3);
    return;
  }

  if (state.error) {
    content.innerHTML = renderError({
      title: 'Error loading orphans',
      message: state.error,
      onRetry: fetchOrphans,
    });
    setupRetryHandler(fetchOrphans);
    return;
  }

  if (!state.selectedRepo) {
    content.innerHTML = `
      <div class="text-center py-8">
        <p class="text-gray-600 dark:text-gray-300">Select a repository to view orphans</p>
      </div>
    `;
    return;
  }

  if (!state.data || state.data.orphans.length === 0) {
    content.innerHTML = `
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No orphaned facts</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Orphans are created during merge/split operations</p>
      </div>
    `;
    return;
  }

  // Filter to show unanswered first
  const sortedOrphans = [...state.data.orphans].sort((a, b) => {
    if (a.answered === b.answered) return a.line_number - b.line_number;
    return a.answered ? 1 : -1;
  });

  content.innerHTML = `
    <div class="space-y-3">
      ${sortedOrphans.map(orphan => renderOrphanCard(orphan, state.selectedRepo, { showCheckbox: state.bulkMode })).join('')}
    </div>
  `;

  setupOrphanHandlers(content);
}

function setupOrphanHandlers(container: HTMLElement): void {
  // Assign button handlers
  container.querySelectorAll('.orphan-assign-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      const button = e.currentTarget as HTMLButtonElement;
      const lineNumber = parseInt(button.dataset.line || '0', 10);
      const card = button.closest('.orphan-card');
      const input = card?.querySelector('.orphan-target-input') as HTMLInputElement;
      const errorEl = card?.querySelector('.orphan-error') as HTMLElement;

      if (!input?.value.trim()) {
        if (errorEl) {
          errorEl.textContent = 'Please enter a document ID or "dismiss"';
          errorEl.classList.remove('hidden');
        }
        return;
      }

      button.disabled = true;
      button.textContent = 'Assigning...';

      try {
        await assignOrphan(lineNumber, input.value.trim());
      } catch (err) {
        if (errorEl) {
          errorEl.textContent = err instanceof Error ? err.message : 'Failed to assign';
          errorEl.classList.remove('hidden');
        }
        button.disabled = false;
        button.textContent = 'Assign';
      }
    });
  });

  // Dismiss button handlers
  container.querySelectorAll('.orphan-dismiss-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      const button = e.currentTarget as HTMLButtonElement;
      const lineNumber = parseInt(button.dataset.line || '0', 10);
      const card = button.closest('.orphan-card');
      const errorEl = card?.querySelector('.orphan-error') as HTMLElement;

      button.disabled = true;
      button.textContent = 'Dismissing...';

      try {
        await assignOrphan(lineNumber, 'dismiss');
      } catch (err) {
        if (errorEl) {
          errorEl.textContent = err instanceof Error ? err.message : 'Failed to dismiss';
          errorEl.classList.remove('hidden');
        }
        button.disabled = false;
        button.textContent = 'Dismiss';
      }
    });
  });

  // Checkbox handlers for bulk mode
  container.querySelectorAll('.orphan-checkbox').forEach(checkbox => {
    checkbox.addEventListener('change', (e) => {
      const input = e.target as HTMLInputElement;
      const lineNumber = parseInt(input.dataset.line || '0', 10);
      if (input.checked) {
        state.selectedLines.add(lineNumber);
      } else {
        state.selectedLines.delete(lineNumber);
      }
      updateUI();
    });
  });

  // Source preview handlers
  container.querySelectorAll('.preview-source-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      const docId = (e.currentTarget as HTMLElement).dataset.docId;
      const line = (e.currentTarget as HTMLElement).dataset.line;
      if (docId) {
        openPreview(docId, line ? parseInt(line, 10) : undefined);
      }
    });
  });
}

function setupBulkActionHandlers(): void {
  const assignBtn = document.getElementById('bulk-assign-btn');
  const dismissBtn = document.getElementById('bulk-dismiss-btn');
  const targetInput = document.getElementById('bulk-target-input') as HTMLInputElement;

  assignBtn?.addEventListener('click', async () => {
    const target = targetInput?.value.trim();
    if (!target) {
      alert('Please enter a document ID');
      return;
    }
    if (confirm(`Assign ${state.selectedLines.size} orphan(s) to ${target}?`)) {
      await bulkAssign(target);
    }
  });

  dismissBtn?.addEventListener('click', async () => {
    if (confirm(`Dismiss ${state.selectedLines.size} orphan(s)?`)) {
      await bulkAssign('dismiss');
    }
  });
}

function setupRepoSelector(): void {
  const select = document.getElementById('repo-select') as HTMLSelectElement;
  select?.addEventListener('change', (e) => {
    state.selectedRepo = (e.target as HTMLSelectElement).value;
    state.selectedLines.clear();
    fetchOrphans();
  });
}

function toggleBulkMode(): void {
  state.bulkMode = !state.bulkMode;
  if (!state.bulkMode) {
    state.selectedLines.clear();
  }
  updateUI();
}

export function renderOrphans(): string {
  const bulkModeLabel = state.bulkMode ? 'Exit bulk mode' : 'Bulk actions';
  const bulkModeClass = state.bulkMode
    ? 'bg-blue-600 text-white hover:bg-blue-700'
    : 'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600';

  return `
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Orphaned Facts</h2>
        <button
          id="bulk-mode-toggle"
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md ${bulkModeClass}"
        >
          ${bulkModeLabel}
        </button>
      </div>
      <div id="orphans-message"></div>
      <div id="bulk-actions-container"></div>
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderRepoSelector()}
      </div>
      <div id="orphans-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderSummary()}
      </div>
      <div id="orphans-content" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading...</p>
        </div>
      </div>
    </div>
  `;
}

export async function initOrphans(): Promise<void> {
  await fetchRepos();
  setupRepoSelector();
  document.getElementById('bulk-mode-toggle')?.addEventListener('click', toggleBulkMode);
  if (state.selectedRepo) {
    await fetchOrphans();
  } else {
    state.loading = false;
    updateUI();
  }
}

export function cleanupOrphans(): void {
  state.data = null;
  state.loading = true;
  state.error = null;
  state.successMessage = null;
  state.bulkMode = false;
  state.selectedLines.clear();
  cleanupPreview();
}
