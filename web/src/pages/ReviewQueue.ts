/**
 * ReviewQueue page component.
 * Lists all pending review questions grouped by document.
 */

import { api, ReviewQueueResponse, DocumentReview, Repository, ApiRequestError } from '../api';
import { renderQuestionCard, renderQuestionTypeBadge } from '../components/QuestionCard';
import { setupAnswerFormHandlers, clearFormStates } from '../components/AnswerForm';
import {
  renderBulkActionsBar,
  setupBulkActionsHandlers,
  clearBulkState,
  BulkSelection,
} from '../components/BulkActions';
import { openPreview, cleanupPreview } from '../components/DocumentPreview';
import { renderSkeletonDocumentGroup } from '../components/Loading';
import { renderError, setupRetryHandler } from '../components/Error';
import { toast } from '../components/Toast';

interface ReviewQueueState {
  data: ReviewQueueResponse | null;
  repos: Repository[];
  loading: boolean;
  error: string | null;
  filterRepo: string;
  filterType: string;
  successMessage: string | null;
  bulkMode: boolean;
}

const state: ReviewQueueState = {
  data: null,
  repos: [],
  loading: true,
  error: null,
  filterRepo: '',
  filterType: '',
  successMessage: null,
  bulkMode: false,
};

const QUESTION_TYPES = ['temporal', 'conflict', 'missing', 'ambiguous', 'stale', 'duplicate'];

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
      api.getReviewQueue(params),
      state.repos.length === 0 ? api.getRepositories() : Promise.resolve({ repositories: state.repos }),
    ]);

    state.data = data;
    state.repos = reposResponse.repositories;
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load review queue';
    }
  } finally {
    state.loading = false;
    updateUI();
  }
}

function renderDocumentGroup(doc: DocumentReview): string {
  const unansweredCount = doc.questions.filter(q => !q.answered).length;
  const totalCount = doc.questions.length;

  return `
    <div class="document-group bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-medium text-gray-900 dark:text-white">${escapeHtml(doc.doc_title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">${escapeHtml(doc.file_path)}</p>
          </div>
          <div class="flex items-center space-x-3">
            <button
              class="preview-doc-btn text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 flex items-center space-x-1"
              data-doc-id="${escapeHtml(doc.doc_id)}"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path>
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"></path>
              </svg>
              <span>Preview</span>
            </button>
            <div class="text-sm text-gray-500 dark:text-gray-400">
              <span class="font-medium">${unansweredCount}</span> / ${totalCount} pending
            </div>
          </div>
        </div>
      </div>
      <div class="p-4 space-y-3">
        ${doc.questions.map((q, i) => renderQuestionCard(q, doc.doc_id, i, {
          showAnswerForm: !state.bulkMode,
          showCheckbox: state.bulkMode,
        })).join('')}
      </div>
    </div>
  `;
}

function renderFilters(): string {
  const repoOptions = state.repos.map(r =>
    `<option value="${escapeHtml(r.id)}" ${state.filterRepo === r.id ? 'selected' : ''}>${escapeHtml(r.name)}</option>`
  ).join('');

  const typeOptions = QUESTION_TYPES.map(t =>
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
        <label for="filter-type" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Question Type</label>
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

  const typeCounts: Record<string, number> = {};
  for (const doc of state.data.documents) {
    for (const q of doc.questions) {
      if (!q.answered) {
        typeCounts[q.question_type] = (typeCounts[q.question_type] || 0) + 1;
      }
    }
  }

  const badges = Object.entries(typeCounts)
    .sort((a, b) => b[1] - a[1])
    .map(([type, count]) => `${renderQuestionTypeBadge(type)} <span class="ml-1 text-gray-600 dark:text-gray-400">${count}</span>`)
    .join('<span class="mx-2 text-gray-300 dark:text-gray-600">|</span>');

  return `
    <div class="flex items-center justify-between text-sm">
      <div class="flex items-center space-x-1">${badges || '<span class="text-gray-500 dark:text-gray-400">No pending questions</span>'}</div>
      <div class="text-gray-500 dark:text-gray-400">
        ${state.data.unanswered} pending / ${state.data.total} total
      </div>
    </div>
  `;
}

function updateUI(): void {
  const content = document.getElementById('review-queue-content');
  if (!content) return;

  // Update summary section
  const summaryEl = document.getElementById('review-summary');
  if (summaryEl && state.data) {
    summaryEl.innerHTML = renderSummary();
  }

  // Update bulk actions bar
  const bulkActionsEl = document.getElementById('bulk-actions-container');
  if (bulkActionsEl && state.data && state.bulkMode) {
    bulkActionsEl.innerHTML = renderBulkActionsBar(state.data.unanswered);
    setupBulkActionsHandlers(
      bulkActionsEl,
      state.data.documents,
      {
        onSuccess: handleBulkSuccess,
        onError: handleBulkError,
        onSelectionChange: handleSelectionChange,
      },
      updateUI
    );
  } else if (bulkActionsEl) {
    bulkActionsEl.innerHTML = '';
  }

  // Show success message via toast (remove inline banner)
  const messageEl = document.getElementById('review-message');
  if (messageEl) {
    messageEl.innerHTML = '';
  }

  if (state.loading) {
    content.innerHTML = `
      <div class="space-y-4">
        ${renderSkeletonDocumentGroup()}
        ${renderSkeletonDocumentGroup()}
        ${renderSkeletonDocumentGroup()}
      </div>
    `;
    return;
  }

  if (state.error) {
    content.innerHTML = renderError({
      title: 'Error loading review queue',
      message: state.error,
      onRetry: fetchData,
    });
    setupRetryHandler(fetchData);
    return;
  }

  if (!state.data || state.data.documents.length === 0) {
    content.innerHTML = `
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending review questions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase lint --review</code> to generate questions</p>
      </div>
    `;
    return;
  }

  // Filter to show only documents with unanswered questions first
  const sortedDocs = [...state.data.documents].sort((a, b) => {
    const aUnanswered = a.questions.filter(q => !q.answered).length;
    const bUnanswered = b.questions.filter(q => !q.answered).length;
    return bUnanswered - aUnanswered;
  });

  content.innerHTML = `
    <div class="space-y-4">
      ${sortedDocs.map(doc => renderDocumentGroup(doc)).join('')}
    </div>
  `;

  // Set up answer form handlers after rendering (only in non-bulk mode)
  if (!state.bulkMode) {
    setupAnswerFormHandlers(content, {
      onSuccess: handleAnswerSuccess,
      onError: handleAnswerError,
    });
  }

  // Set up preview button handlers
  setupPreviewHandlers(content);
}

function setupPreviewHandlers(container: HTMLElement): void {
  // Document preview buttons
  container.querySelectorAll('.preview-doc-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const docId = (e.currentTarget as HTMLElement).dataset.docId;
      if (docId) {
        openPreview(docId);
      }
    });
  });

  // Question line preview buttons
  container.querySelectorAll('.preview-line-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const docId = (e.currentTarget as HTMLElement).dataset.docId;
      const lineRef = (e.currentTarget as HTMLElement).dataset.lineRef;
      if (docId) {
        openPreview(docId, lineRef ? parseInt(lineRef, 10) : undefined);
      }
    });
  });
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

function handleAnswerSuccess(docId: string, questionIndex: number, answer: string): void {
  // Update local state to mark question as answered (optimistic update)
  if (state.data) {
    const doc = state.data.documents.find(d => d.doc_id === docId);
    if (doc && doc.questions[questionIndex]) {
      doc.questions[questionIndex].answered = true;
      doc.questions[questionIndex].answer = answer;
      state.data.answered++;
      state.data.unanswered--;
    }
  }

  // Show toast notification
  toast.success(`Answer submitted for question ${questionIndex + 1}`);
  updateUI();
}

function handleAnswerError(error: string): void {
  // Show error toast
  toast.error(`Failed to submit answer: ${error}`);
  updateUI();
}

function handleBulkSuccess(count: number, action: string): void {
  // Show toast and refetch data
  toast.success(`Successfully ${action} ${count} question(s)`);
  fetchData();
}

function handleBulkError(error: string): void {
  toast.error(`Bulk action failed: ${error}`);
  updateUI();
}

function handleSelectionChange(_selections: BulkSelection[]): void {
  // Selection state is managed in BulkActions, just trigger re-render
  // to update the count display
}

function toggleBulkMode(): void {
  state.bulkMode = !state.bulkMode;
  if (!state.bulkMode) {
    clearBulkState();
  }
  updateUI();
}

export function renderReviewQueue(): string {
  const bulkModeLabel = state.bulkMode ? 'Exit bulk mode' : 'Bulk actions';
  const bulkModeClass = state.bulkMode
    ? 'bg-blue-600 text-white hover:bg-blue-700'
    : 'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600';

  return `
    <div class="space-y-4 sm:space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <h2 class="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">Review Queue</h2>
        <button
          id="bulk-mode-toggle"
          class="inline-flex items-center justify-center px-3 py-2 text-sm font-medium rounded-md ${bulkModeClass}"
        >
          ${bulkModeLabel}
        </button>
      </div>
      <div id="review-message"></div>
      <div id="bulk-actions-container"></div>
      <div id="review-filters" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderFilters()}
      </div>
      <div id="review-summary" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        ${renderSummary()}
      </div>
      <div id="review-queue-content">
        <div class="text-center py-8">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading review queue...</p>
        </div>
      </div>
    </div>
  `;
}

export function initReviewQueue(): void {
  setupFilterHandlers();
  setupBulkModeToggle();
  fetchData();
}

function setupBulkModeToggle(): void {
  document.getElementById('bulk-mode-toggle')?.addEventListener('click', toggleBulkMode);
}

export function cleanupReviewQueue(): void {
  // Reset state for next visit
  state.data = null;
  state.loading = true;
  state.error = null;
  state.successMessage = null;
  state.bulkMode = false;
  clearFormStates();
  clearBulkState();
  cleanupPreview();
}
