/**
 * ReviewQueue page component.
 * Lists all pending review questions grouped by document.
 */

import { api, ReviewQueueResponse, DocumentReview, Repository, ApiRequestError, ApplyResult } from '../api';
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
  const isArchived = doc.file_path.includes('/archive/');
  const archiveBadge = isArchived
    ? '<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 ml-2" title="Archived documents are excluded from checks">📦 archived</span>'
    : '';

  return `
    <div class="document-group bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <div class="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-b border-gray-200 dark:border-gray-600">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-medium text-gray-900 dark:text-white">${escapeHtml(doc.doc_title)}${archiveBadge}</h3>
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

  // Update workflow stepper
  const stepperEl = document.getElementById('workflow-stepper');
  if (stepperEl) stepperEl.innerHTML = renderWorkflowStepper();

  // Update deferred banner
  const deferredEl = document.getElementById('deferred-banner');
  if (deferredEl) deferredEl.innerHTML = renderDeferredBanner();

  // Update apply bar
  const applyEl = document.getElementById('apply-bar');
  if (applyEl) {
    applyEl.innerHTML = renderApplyBar();
    setupApplyHandlers();
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

function renderWorkflowStepper(): string {
  if (!state.data) return '';
  const unanswered = state.data.unanswered;
  const answered = state.data.answered;
  // Determine current step: 1=review, 2=answer, 3=apply, 4=verify
  let currentStep = 1;
  if (unanswered === 0 && answered > 0) currentStep = 3;
  else if (answered > 0) currentStep = 2;
  else if (unanswered === 0 && answered === 0) currentStep = 4;

  const steps = [
    { num: 1, label: 'Review questions' },
    { num: 2, label: 'Answer/defer' },
    { num: 3, label: 'Apply answers' },
    { num: 4, label: 'Verify' },
  ];

  return `
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
      <div class="flex items-center justify-between text-sm">
        ${steps.map(s => {
          const active = s.num === currentStep;
          const done = s.num < currentStep;
          const cls = active
            ? 'text-blue-600 dark:text-blue-400 font-semibold'
            : done
              ? 'text-green-600 dark:text-green-400'
              : 'text-gray-400 dark:text-gray-500';
          const icon = done ? '✓' : s.num.toString();
          return `<div class="flex items-center space-x-1 ${cls}"><span class="w-5 h-5 flex items-center justify-center rounded-full ${active ? 'bg-blue-100 dark:bg-blue-900' : done ? 'bg-green-100 dark:bg-green-900' : 'bg-gray-100 dark:bg-gray-700'} text-xs">${icon}</span><span class="hidden sm:inline">${s.label}</span></div>`;
        }).join('<div class="flex-1 h-px bg-gray-200 dark:bg-gray-700 mx-2"></div>')}
      </div>
    </div>
  `;
}

function renderDeferredBanner(): string {
  if (!state.data) return '';
  // Count deferred questions (answered=false but has answer text)
  let deferredCount = 0;
  for (const doc of state.data.documents) {
    for (const q of doc.questions) {
      if (!q.answered && q.answer && (q.answer.toLowerCase().startsWith('defer') || q.answer.toLowerCase().startsWith('needs '))) {
        deferredCount++;
      }
    }
  }
  // Also check the deferred field from stats
  if (deferredCount === 0) return '';

  return `
    <div class="bg-amber-50 dark:bg-amber-900/30 border border-amber-200 dark:border-amber-800 rounded-lg p-4">
      <div class="flex items-center justify-between">
        <div class="flex items-center space-x-2">
          <span class="text-amber-600 dark:text-amber-400">⚠</span>
          <span class="text-sm font-medium text-amber-800 dark:text-amber-200">${deferredCount} item${deferredCount !== 1 ? 's' : ''} need${deferredCount === 1 ? 's' : ''} human attention</span>
        </div>
        <button id="filter-deferred-btn" class="text-sm text-amber-700 dark:text-amber-300 hover:underline">Show deferred</button>
      </div>
    </div>
  `;
}

function renderApplyBar(): string {
  if (!state.data || state.data.answered === 0) return '';
  return `
    <div class="bg-green-50 dark:bg-green-900/30 border border-green-200 dark:border-green-800 rounded-lg p-4">
      <div class="flex items-center justify-between">
        <span class="text-sm text-green-800 dark:text-green-200">${state.data.answered} answered question${state.data.answered !== 1 ? 's' : ''} ready to apply</span>
        <div class="flex items-center space-x-2">
          <button id="apply-preview-btn" class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-green-100 dark:bg-green-800 text-green-700 dark:text-green-200 hover:bg-green-200 dark:hover:bg-green-700">Preview</button>
          <button id="apply-btn" class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-green-600 text-white hover:bg-green-700 disabled:opacity-50">Apply Answers</button>
        </div>
      </div>
      <div id="apply-result" class="mt-2"></div>
    </div>
  `;
}

async function handleApply(dryRun: boolean): Promise<void> {
  const btn = document.getElementById(dryRun ? 'apply-preview-btn' : 'apply-btn') as HTMLButtonElement | null;
  if (btn) { btn.disabled = true; btn.textContent = '⏳ ...'; }

  try {
    const result: ApplyResult = await api.applyAnswers({ dry_run: dryRun });
    const resultEl = document.getElementById('apply-result');
    if (resultEl) {
      if (result.total_applied === 0) {
        resultEl.innerHTML = `<p class="text-sm text-gray-600 dark:text-gray-400">${result.message}</p>`;
      } else {
        const docs = result.documents.map(d =>
          `<li class="text-sm"><span class="font-medium">${escapeHtml(d.doc_title)}</span>: ${d.questions_applied ?? 0} question${(d.questions_applied ?? 0) !== 1 ? 's' : ''} ${d.status}</li>`
        ).join('');
        resultEl.innerHTML = `<p class="text-sm font-medium mb-1">${result.message}</p><ul class="list-disc list-inside space-y-1">${docs}</ul>`;
      }
    }
    if (!dryRun && result.total_applied > 0) {
      toast.success(`Applied ${result.total_applied} answer(s)`);
      fetchData();
    }
  } catch (e) {
    if (e instanceof ApiRequestError && e.status === 503) {
      toast.info(e.message);
    } else {
      toast.error(e instanceof Error ? e.message : 'Apply failed');
    }
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = dryRun ? 'Preview' : 'Apply Answers';
    }
  }
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
      <div id="workflow-stepper">${renderWorkflowStepper()}</div>
      <div id="deferred-banner">${renderDeferredBanner()}</div>
      <div id="apply-bar">${renderApplyBar()}</div>
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
  setupApplyHandlers();
  fetchData();
}

function setupApplyHandlers(): void {
  document.getElementById('apply-preview-btn')?.addEventListener('click', () => handleApply(true));
  document.getElementById('apply-btn')?.addEventListener('click', () => handleApply(false));
  document.getElementById('filter-deferred-btn')?.addEventListener('click', () => {
    // Set filter to show deferred items
    state.filterType = '';
    // Refetch with deferred status - for now just show all and let user see deferred markers
    toast.info('Deferred items are shown with ⚠ markers in the queue');
  });
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
