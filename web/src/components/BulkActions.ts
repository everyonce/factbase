/**
 * BulkActions component.
 * Provides checkbox selection and bulk operations for review questions.
 */

import { api, ApiRequestError, DocumentReview, ReviewQuestion } from '../api';

export interface BulkSelection {
  docId: string;
  questionIndex: number;
}

export interface BulkActionsCallbacks {
  onSuccess: (count: number, action: string) => void;
  onError: (error: string) => void;
  onSelectionChange: (selections: BulkSelection[]) => void;
}

interface BulkState {
  selections: Set<string>;
  submitting: boolean;
  showBulkAnswer: boolean;
}

const state: BulkState = {
  selections: new Set(),
  submitting: false,
  showBulkAnswer: false,
};

function getSelectionKey(docId: string, questionIndex: number): string {
  return `${docId}:${questionIndex}`;
}

function parseSelectionKey(key: string): BulkSelection {
  const [docId, indexStr] = key.split(':');
  return { docId, questionIndex: parseInt(indexStr, 10) };
}

export function getSelections(): BulkSelection[] {
  return Array.from(state.selections).map(parseSelectionKey);
}

export function isSelected(docId: string, questionIndex: number): boolean {
  return state.selections.has(getSelectionKey(docId, questionIndex));
}

export function toggleSelection(docId: string, questionIndex: number): void {
  const key = getSelectionKey(docId, questionIndex);
  if (state.selections.has(key)) {
    state.selections.delete(key);
  } else {
    state.selections.add(key);
  }
}

export function selectAll(documents: DocumentReview[]): void {
  state.selections.clear();
  for (const doc of documents) {
    for (let i = 0; i < doc.questions.length; i++) {
      if (!doc.questions[i].answered) {
        state.selections.add(getSelectionKey(doc.doc_id, i));
      }
    }
  }
}

export function selectNone(): void {
  state.selections.clear();
}

export function clearBulkState(): void {
  state.selections.clear();
  state.submitting = false;
  state.showBulkAnswer = false;
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export function renderBulkActionsBar(totalUnanswered: number): string {
  const selectedCount = state.selections.size;
  const hasSelection = selectedCount > 0;

  return `
    <div id="bulk-actions-bar" class="bg-white dark:bg-gray-800 rounded-lg shadow p-4" role="toolbar" aria-label="Bulk actions">
      <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
        <div class="flex items-center space-x-4">
          <span class="text-sm text-gray-600 dark:text-gray-400" aria-live="polite">
            <span id="bulk-selected-count" class="font-medium">${selectedCount}</span> of ${totalUnanswered} selected
          </span>
          <div class="flex items-center space-x-2" role="group" aria-label="Selection controls">
            <button
              id="bulk-select-all"
              class="text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-200"
              aria-label="Select all unanswered questions"
            >
              Select all
            </button>
            <span class="text-gray-300 dark:text-gray-600" aria-hidden="true">|</span>
            <button
              id="bulk-select-none"
              class="text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-200"
              aria-label="Clear selection"
            >
              Select none
            </button>
          </div>
        </div>
        <div class="flex items-center space-x-2" role="group" aria-label="Bulk actions">
          <button
            id="bulk-dismiss-btn"
            class="inline-flex items-center px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md disabled:opacity-50 disabled:cursor-not-allowed"
            ${!hasSelection || state.submitting ? 'disabled aria-disabled="true"' : ''}
            aria-label="Dismiss ${selectedCount} selected questions"
          >
            Dismiss selected
          </button>
          <button
            id="bulk-answer-btn"
            class="inline-flex items-center px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md disabled:opacity-50 disabled:cursor-not-allowed"
            ${!hasSelection || state.submitting ? 'disabled aria-disabled="true"' : ''}
            ${state.submitting ? 'aria-busy="true"' : ''}
            aria-label="Answer ${selectedCount} selected questions"
          >
            ${state.submitting ? `
              <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24" aria-hidden="true">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Processing...
            ` : 'Answer selected...'}
          </button>
        </div>
      </div>
      ${state.showBulkAnswer ? renderBulkAnswerForm() : ''}
    </div>
  `;
}

function renderBulkAnswerForm(): string {
  const inputId = 'bulk-answer-input';
  const labelId = 'bulk-answer-label';
  return `
    <div id="bulk-answer-form" class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
      <div class="space-y-3">
        <label id="${labelId}" for="${inputId}" class="block text-sm font-medium text-gray-700 dark:text-gray-300">
          Apply same answer to ${state.selections.size} selected question(s)
        </label>
        <textarea
          id="${inputId}"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="Enter answer to apply to all selected questions..."
          aria-labelledby="${labelId}"
          ${state.submitting ? 'disabled aria-busy="true"' : ''}
        ></textarea>
        <div class="flex items-center justify-end space-x-2">
          <button
            id="bulk-answer-cancel"
            class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md"
            ${state.submitting ? 'disabled' : ''}
          >
            Cancel
          </button>
          <button
            id="bulk-answer-submit"
            class="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md disabled:opacity-50"
            ${state.submitting ? 'disabled aria-busy="true"' : ''}
          >
            Apply to all
          </button>
        </div>
      </div>
    </div>
  `;
}

export function renderSelectionCheckbox(
  docId: string,
  questionIndex: number,
  question: ReviewQuestion
): string {
  if (question.answered) return '';

  const key = getSelectionKey(docId, questionIndex);
  const checked = state.selections.has(key);
  const checkboxId = `bulk-checkbox-${escapeHtml(docId)}-${questionIndex}`;

  return `
    <input
      type="checkbox"
      id="${checkboxId}"
      class="bulk-checkbox h-4 w-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
      data-doc-id="${escapeHtml(docId)}"
      data-question-index="${questionIndex}"
      aria-label="Select question ${questionIndex + 1} for bulk action"
      ${checked ? 'checked' : ''}
    />
  `;
}

async function submitBulkAction(
  action: 'dismiss' | 'answer',
  answer: string,
  callbacks: BulkActionsCallbacks
): Promise<void> {
  if (state.selections.size === 0) return;

  state.submitting = true;

  try {
    const answers = Array.from(state.selections).map(key => {
      const { docId, questionIndex } = parseSelectionKey(key);
      return { doc_id: docId, question_index: questionIndex, answer };
    });

    const result = await api.bulkAnswerQuestions(answers);

    if (result.errors && result.errors.length > 0) {
      callbacks.onError(`Some answers failed: ${result.errors.join(', ')}`);
    }

    const successCount = result.results.filter(r => r.success).length;
    if (successCount > 0) {
      callbacks.onSuccess(successCount, action === 'dismiss' ? 'dismissed' : 'answered');
      state.selections.clear();
      state.showBulkAnswer = false;
    }
  } catch (e) {
    if (e instanceof ApiRequestError) {
      callbacks.onError(e.message);
    } else {
      callbacks.onError('Failed to process bulk action');
    }
  } finally {
    state.submitting = false;
  }
}

export function setupBulkActionsHandlers(
  container: HTMLElement,
  documents: DocumentReview[],
  callbacks: BulkActionsCallbacks,
  updateUI: () => void
): void {
  // Select all button
  container.querySelector('#bulk-select-all')?.addEventListener('click', () => {
    selectAll(documents);
    callbacks.onSelectionChange(getSelections());
    updateUI();
  });

  // Select none button
  container.querySelector('#bulk-select-none')?.addEventListener('click', () => {
    selectNone();
    callbacks.onSelectionChange(getSelections());
    updateUI();
  });

  // Bulk dismiss button
  container.querySelector('#bulk-dismiss-btn')?.addEventListener('click', async () => {
    if (state.selections.size === 0) return;

    const count = state.selections.size;
    if (!confirm(`Dismiss ${count} selected question(s)?`)) return;

    await submitBulkAction('dismiss', 'dismiss', callbacks);
    updateUI();
  });

  // Bulk answer button - toggle form
  container.querySelector('#bulk-answer-btn')?.addEventListener('click', () => {
    if (state.submitting) return;
    state.showBulkAnswer = !state.showBulkAnswer;
    updateUI();
  });

  // Bulk answer cancel
  container.querySelector('#bulk-answer-cancel')?.addEventListener('click', () => {
    state.showBulkAnswer = false;
    updateUI();
  });

  // Bulk answer submit
  container.querySelector('#bulk-answer-submit')?.addEventListener('click', async () => {
    const input = container.querySelector('#bulk-answer-input') as HTMLTextAreaElement | null;
    const answer = input?.value.trim();
    if (!answer) return;

    await submitBulkAction('answer', answer, callbacks);
    updateUI();
  });

  // Checkbox changes
  container.addEventListener('change', (e) => {
    const checkbox = e.target as HTMLInputElement;
    if (!checkbox.classList.contains('bulk-checkbox')) return;

    const docId = checkbox.dataset.docId;
    const questionIndex = parseInt(checkbox.dataset.questionIndex || '0', 10);
    if (!docId) return;

    toggleSelection(docId, questionIndex);
    callbacks.onSelectionChange(getSelections());
    updateUI();
  });
}
