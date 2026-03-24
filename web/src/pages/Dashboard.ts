/**
 * Dashboard page component.
 * Shows HITL summary with counts and quick links.
 */

import { api, AggregateStats, ReviewStats, OrganizeStats, ApiRequestError } from '../api';
import { renderSkeletonStats } from '../components/Loading';
import { renderError, setupRetryHandler } from '../components/Error';
import { toast } from '../components/Toast';

interface DashboardState {
  stats: AggregateStats | null;
  review: ReviewStats | null;
  organize: OrganizeStats | null;
  loading: boolean;
  error: string | null;
  autoRefresh: boolean;
  refreshInterval: number | null;
}

const state: DashboardState = {
  stats: null,
  review: null,
  organize: null,
  loading: true,
  error: null,
  autoRefresh: false,
  refreshInterval: null,
};

const AUTO_REFRESH_MS = 30000; // 30 seconds

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(iso: string | undefined): string {
  if (!iso) return 'Never';
  const date = new Date(iso);
  return date.toLocaleString();
}

async function fetchData(): Promise<void> {
  state.loading = true;
  state.error = null;
  updateUI();

  try {
    const [stats, review, organize] = await Promise.all([
      api.getStats(),
      api.getReviewStats(),
      api.getOrganizeStats(),
    ]);
    state.stats = stats;
    state.review = review;
    state.organize = organize;
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load dashboard data';
    }
  } finally {
    state.loading = false;
    updateUI();
  }
}

function updateUI(): void {
  // Update review count — show triage breakdown
  const reviewCount = document.getElementById('review-count');
  if (reviewCount) {
    if (state.loading) {
      reviewCount.innerHTML = '...';
    } else if (state.review) {
      const r = state.review;
      const total = r.unanswered + r.deferred;
      reviewCount.innerHTML = `
        <div class="space-y-1 text-base">
          <div class="flex items-center gap-2">
            <span class="text-2xl sm:text-3xl font-bold text-red-600 dark:text-red-400">${r.needs_input}</span>
            <span class="text-sm text-gray-500 dark:text-gray-400">needs your input</span>
          </div>
          <div class="flex items-center gap-2 text-sm">
            <span class="font-semibold text-amber-600 dark:text-amber-400">🟡 ${r.needs_approval}</span>
            <span class="text-gray-500 dark:text-gray-400">needs approval</span>
          </div>
          <div class="flex items-center gap-2 text-sm">
            <span class="font-semibold text-green-600 dark:text-green-400">🟢 ${r.auto_resolved}</span>
            <span class="text-gray-500 dark:text-gray-400">auto-resolved</span>
          </div>
          <div class="text-xs text-gray-400 dark:text-gray-500 mt-1">${total} total pending</div>
        </div>
      `;
    } else {
      reviewCount.innerHTML = '-';
    }
  }

  // Update deferred count
  const deferredCount = document.getElementById('deferred-count');
  if (deferredCount) {
    const deferred = state.review?.deferred ?? 0;
    deferredCount.textContent = deferred > 0 ? `⚠ ${deferred} deferred` : '';
  }

  // Update organize count
  const organizeCount = document.getElementById('organize-count');
  if (organizeCount) {
    const total = state.organize
      ? state.organize.merge_candidates + state.organize.misplaced_candidates + state.organize.duplicate_entry_count
      : 0;
    organizeCount.textContent = state.loading ? '...' : total.toString();
  }

  // Update orphan count
  const orphanCount = document.getElementById('orphan-count');
  if (orphanCount) {
    orphanCount.textContent = state.loading ? '...' : (state.organize?.orphan_count.toString() ?? '-');
  }

  // Update stats content
  const statsContent = document.getElementById('stats-content');
  if (statsContent) {
    if (state.loading) {
      statsContent.innerHTML = renderSkeletonStats();
    } else if (state.error) {
      statsContent.innerHTML = renderError({
        title: 'Error loading stats',
        message: state.error,
        onRetry: fetchData,
      });
      setupRetryHandler(fetchData);
    } else if (state.stats) {
      statsContent.innerHTML = `
        <dl class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Repositories</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${state.stats.repos_count}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Documents</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${state.stats.docs_count}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Database Size</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${formatBytes(state.stats.db_size_bytes)}</dd>
          </div>
          <div>
            <dt class="text-sm text-gray-500 dark:text-gray-400">Last Scan</dt>
            <dd class="text-lg font-semibold text-gray-900 dark:text-white">${formatDate(state.stats.last_scan)}</dd>
          </div>
        </dl>
      `;
    }
  }

  // Update auto-refresh toggle
  const autoRefreshToggle = document.getElementById('auto-refresh-toggle') as HTMLInputElement | null;
  if (autoRefreshToggle) {
    autoRefreshToggle.checked = state.autoRefresh;
  }
}

function toggleAutoRefresh(enabled: boolean): void {
  state.autoRefresh = enabled;
  if (enabled && !state.refreshInterval) {
    state.refreshInterval = window.setInterval(() => fetchData(), AUTO_REFRESH_MS);
  } else if (!enabled && state.refreshInterval) {
    clearInterval(state.refreshInterval);
    state.refreshInterval = null;
  }
}

export function renderDashboard(): string {
  return `
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Dashboard</h2>
        <div class="flex items-center space-x-3">
          <button id="scan-btn" class="inline-flex items-center px-3 py-2 text-sm font-medium rounded-md bg-green-600 text-white hover:bg-green-700 disabled:opacity-50">🔄 Scan</button>
          <button id="check-btn" class="inline-flex items-center px-3 py-2 text-sm font-medium rounded-md bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50">🔍 Check</button>
          <label class="flex items-center space-x-2 text-sm text-gray-600 dark:text-gray-300">
            <input type="checkbox" id="auto-refresh-toggle" class="rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500" ${state.autoRefresh ? 'checked' : ''}>
            <span>Auto-refresh</span>
          </label>
        </div>
      </div>
      <div id="action-result"></div>
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 sm:gap-6">
        <a href="#/review" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">❓</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Review Queue</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Pending questions</p>
            </div>
          </div>
          <div id="review-count" class="mt-3 sm:mt-4">-</div>
          <div id="deferred-count" class="mt-1 text-sm text-amber-600 dark:text-amber-400"></div>
        </a>
        <a href="#/organize" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">📁</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Organize</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Suggestions</p>
            </div>
          </div>
          <div id="organize-count" class="mt-3 sm:mt-4 text-2xl sm:text-3xl font-bold text-green-600 dark:text-green-400">-</div>
        </a>
        <a href="#/orphans" class="block bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6 hover:shadow-lg transition-shadow sm:col-span-2 lg:col-span-1">
          <div class="flex items-center space-x-3">
            <span class="text-2xl sm:text-3xl">📝</span>
            <div>
              <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white">Orphans</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">Unassigned facts</p>
            </div>
          </div>
          <div id="orphan-count" class="mt-3 sm:mt-4 text-2xl sm:text-3xl font-bold text-amber-600 dark:text-amber-400">-</div>
        </a>
      </div>
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
        <h3 class="text-base sm:text-lg font-semibold text-gray-900 dark:text-white mb-4">Quick Stats</h3>
        <div id="stats-content" class="text-gray-600 dark:text-gray-300">Loading...</div>
      </div>
    </div>
  `;
}

export function initDashboard(): void {
  // Set up auto-refresh toggle handler
  const toggle = document.getElementById('auto-refresh-toggle');
  toggle?.addEventListener('change', (e) => {
    toggleAutoRefresh((e.target as HTMLInputElement).checked);
  });

  // Set up scan button
  document.getElementById('scan-btn')?.addEventListener('click', async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = '⏳ Scanning...';
    try {
      const result = await api.triggerScan();
      if (result.status === 'cli_required') {
        toast.info(result.message);
        showActionResult(result.command);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Scan failed');
    } finally {
      btn.disabled = false;
      btn.textContent = '🔄 Scan';
    }
  });

  // Set up check button
  document.getElementById('check-btn')?.addEventListener('click', async (e) => {
    const btn = e.currentTarget as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = '⏳ Checking...';
    try {
      const result = await api.triggerCheck();
      if (result.status === 'cli_required') {
        toast.info(result.message);
        showActionResult(result.command);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Check failed');
    } finally {
      btn.disabled = false;
      btn.textContent = '🔍 Check';
    }
  });

  // Fetch initial data
  fetchData();
}

function showActionResult(command: string): void {
  const el = document.getElementById('action-result');
  if (el) {
    el.innerHTML = `
      <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
        <p class="text-sm text-blue-800 dark:text-blue-200">Run in terminal:</p>
        <code class="block mt-1 text-sm bg-blue-100 dark:bg-blue-900 px-2 py-1 rounded font-mono">${command}</code>
      </div>
    `;
  }
}

export function cleanupDashboard(): void {
  if (state.refreshInterval) {
    clearInterval(state.refreshInterval);
    state.refreshInterval = null;
  }
}
