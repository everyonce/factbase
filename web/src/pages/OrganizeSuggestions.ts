/**
 * OrganizeSuggestions page component.
 * Lists all pending organize suggestions grouped by type.
 */

import { api, SuggestionsResponse, Repository, ApiRequestError } from '../api';
import { renderSuggestionTypeBadge, renderMergeCard, renderMisplacedCard } from '../components/SuggestionCard';
import { openPreview, cleanupPreview } from '../components/DocumentPreview';
import { openMergePreview, cleanupMergePreview } from '../components/MergePreview';
import { openSplitPreview, cleanupSplitPreview } from '../components/SplitPreview';
import { renderSkeletonList } from '../components/Loading';
import { renderError, setupRetryHandler } from '../components/Error';
import { toast } from '../components/Toast';

interface OrganizeSuggestionsState {
  data: SuggestionsResponse | null;
  repos: Repository[];
  loading: boolean;
  error: string | null;
  filterRepo: string;
  filterType: string;
  successMessage: string | null;
}

const state: OrganizeSuggestionsState = {
  data: null,
  repos: [],
  loading: true,
  error: null,
  filterRepo: '',
  filterType: '',
  successMessage: null,
};

const SUGGESTION_TYPES = ['merge', 'misplaced'];

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

async function fetchData(): Promise<void> {
  state.loading = true;
  state.error = null;
  updateUI();

  try {
    const params: { repo?: string; type?: string } = {};
    if (state.filterRepo) params.repo = state.filterRepo;
    if (state.filterType) params.type = state.filterType;

    const [data, reposResponse] = await Promise.all([
      api.getSuggestions(params),
      state.repos.length === 0 ? api.getRepositories() : Promise.resolve({ repositories: state.repos }),
    ]);

    state.data = data;
    state.repos = reposResponse.repositories;
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load suggestions';
    }
  } finally {
    state.loading = false;
    updateUI();
  }
}

function renderFilters(): string {
  const repoOptions = state.repos.map(r =>
    `<option value="${escapeHtml(r.id)}" ${state.filterRepo === r.id ? 'selected' : ''}>${escapeHtml(r.name)}</option>`
  ).join('');

  const typeOptions = SUGGESTION_TYPES.map(t =>
    `<option value="${t}" ${state.filterType === t ? 'selected' : ''}>${t}</option>`
  ).join('');

  return `
    <div class="flex flex-col sm:flex-row gap-4">
      <div class="flex-1 sm:flex-none">
        <label for="filter-repo" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Repository</label>
        <select id="filter-repo" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All repositories</option>
          ${repoOptions}
        </select>
      </div>
      <div class="flex-1 sm:flex-none">
        <label for="filter-type" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Suggestion Type</label>
        <select id="filter-type" class="block w-full sm:w-48 rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm">
          <option value="">All types</option>
          ${typeOptions}
        </select>
      </div>
    </div>
  `;
}

function renderSummary(): string {
  if (!state.data) return '';

  const mergeCount = state.data.merge.length;
  const misplacedCount = state.data.misplaced.length;

  const badges: string[] = [];
  if (mergeCount > 0) {
    badges.push(`${renderSuggestionTypeBadge('merge')} <span class="ml-1 text-gray-600 dark:text-gray-400">${mergeCount}</span>`);
  }
  if (misplacedCount > 0) {
    badges.push(`${renderSuggestionTypeBadge('misplaced')} <span class="ml-1 text-gray-600 dark:text-gray-400">${misplacedCount}</span>`);
  }

  return `
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-4">
        ${badges.length > 0 ? badges.join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>') : '<span class="text-gray-500 dark:text-gray-400">No pending suggestions</span>'}
      </div>
      <div class="text-gray-500 dark:text-gray-400">
        ${state.data.total} total suggestion${state.data.total !== 1 ? 's' : ''}
      </div>
    </div>
  `;
}

function renderMergeSection(): string {
  if (!state.data || state.data.merge.length === 0) return '';
  if (state.filterType && state.filterType !== 'merge') return '';

  return `
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${renderSuggestionTypeBadge('merge')}
        <span>Merge Candidates</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${state.data.merge.length})</span>
      </h3>
      <div class="space-y-3">
        ${state.data.merge.map((s, i) => renderMergeCard(s, i)).join('')}
      </div>
    </div>
  `;
}

function renderMisplacedSection(): string {
  if (!state.data || state.data.misplaced.length === 0) return '';
  if (state.filterType && state.filterType !== 'misplaced') return '';

  return `
    <div class="space-y-4">
      <h3 class="text-lg font-medium text-gray-900 dark:text-white flex items-center space-x-2">
        ${renderSuggestionTypeBadge('misplaced')}
        <span>Misplaced Documents</span>
        <span class="text-sm font-normal text-gray-500 dark:text-gray-400">(${state.data.misplaced.length})</span>
      </h3>
      <div class="space-y-3">
        ${state.data.misplaced.map((s, i) => renderMisplacedCard(s, i)).join('')}
      </div>
    </div>
  `;
}

function updateUI(): void {
  const content = document.getElementById('organize-content');
  if (!content) return;

  // Update summary section
  const summaryEl = document.getElementById('organize-summary');
  if (summaryEl && state.data) {
    summaryEl.innerHTML = renderSummary();
  }

  // Clear inline message area (using toasts now)
  const messageEl = document.getElementById('organize-message');
  if (messageEl) {
    messageEl.innerHTML = '';
  }

  if (state.loading) {
    content.innerHTML = renderSkeletonList(4);
    return;
  }

  if (state.error) {
    content.innerHTML = renderError({
      title: 'Error loading suggestions',
      message: state.error,
      onRetry: fetchData,
    });
    setupRetryHandler(fetchData);
    return;
  }

  if (!state.data || state.data.total === 0) {
    content.innerHTML = `
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending organize suggestions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize analyze</code> to detect suggestions</p>
      </div>
    `;
    return;
  }

  const mergeSection = renderMergeSection();
  const misplacedSection = renderMisplacedSection();

  if (!mergeSection && !misplacedSection) {
    content.innerHTML = `
      <div class="text-center py-8">
        <span class="text-4xl">🔍</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No suggestions match current filters</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Try adjusting the filters above</p>
      </div>
    `;
    return;
  }

  content.innerHTML = `
    <div class="space-y-8">
      ${mergeSection}
      ${misplacedSection}
    </div>
  `;

  setupActionHandlers(content);
  setupPreviewHandlers(content);
}

function setupFilterHandlers(): void {
  const repoSelect = document.getElementById('filter-repo') as HTMLSelectElement | null;
  const typeSelect = document.getElementById('filter-type') as HTMLSelectElement | null;

  repoSelect?.addEventListener('change', (e) => {
    state.filterRepo = (e.target as HTMLSelectElement).value;
    fetchData();
  });

  typeSelect?.addEventListener('change', (e) => {
    state.filterType = (e.target as HTMLSelectElement).value;
    fetchData();
  });
}

function setupActionHandlers(container: HTMLElement): void {
  // Compare buttons (merge preview)
  container.querySelectorAll('.compare-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const button = e.currentTarget as HTMLElement;
      const doc1 = button.dataset.doc1;
      const doc2 = button.dataset.doc2;
      if (doc1 && doc2) {
        openMergePreview(doc1, doc2);
      }
    });
  });

  // Sections buttons (split preview)
  container.querySelectorAll('.sections-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const button = e.currentTarget as HTMLElement;
      const docId = button.dataset.docId;
      if (docId) {
        openSplitPreview(docId);
      }
    });
  });

  // Approve buttons
  container.querySelectorAll('.approve-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      const button = e.currentTarget as HTMLElement;
      const type = button.dataset.type as 'merge' | 'misplaced';

      // Show CLI instruction since approve requires LLM
      const message = type === 'merge'
        ? `To merge these documents, run: factbase organize merge ${button.dataset.doc1} ${button.dataset.doc2}`
        : `To retype this document, run: factbase organize retype ${button.dataset.docId} --type ${button.dataset.suggestedType}`;

      alert(message);
    });
  });

  // Dismiss buttons
  container.querySelectorAll('.dismiss-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      const button = e.currentTarget as HTMLElement;
      const type = button.dataset.type as 'merge' | 'misplaced';
      const docId = button.dataset.docId!;
      const targetId = button.dataset.targetId;

      button.textContent = 'Dismissing...';
      (button as HTMLButtonElement).disabled = true;

      try {
        await api.dismissSuggestion(type, docId, targetId);
        toast.success('Suggestion dismissed');
        // Note: Since suggestions are computed dynamically, dismissing is just acknowledgment
        // The suggestion may reappear on next analyze. For persistent dismissal, use CLI.
        fetchData();
      } catch (e) {
        if (e instanceof ApiRequestError) {
          toast.error(`Failed to dismiss: ${e.message}`);
        } else {
          toast.error('Failed to dismiss suggestion');
        }
        updateUI();
      }
    });
  });
}

function setupPreviewHandlers(container: HTMLElement): void {
  container.querySelectorAll('.preview-doc-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const docId = (e.currentTarget as HTMLElement).dataset.docId;
      if (docId) {
        openPreview(docId);
      }
    });
  });
}

export function renderOrganizeSuggestions(): string {
  return `
    <div class="space-y-4 sm:space-y-6">
      <div class="flex items-center justify-between">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Organize Suggestions</h2>
      </div>
      <div id="organize-message"></div>
      <div id="organize-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderFilters()}
      </div>
      <div id="organize-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderSummary()}
      </div>
      <div id="organize-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading suggestions...</p>
        </div>
      </div>
    </div>
  `;
}

export function initOrganizeSuggestions(): void {
  setupFilterHandlers();
  fetchData();
}

export function cleanupOrganizeSuggestions(): void {
  state.data = null;
  state.loading = true;
  state.error = null;
  state.successMessage = null;
  cleanupPreview();
  cleanupMergePreview();
  cleanupSplitPreview();
}
