/**
 * AnswerForm component.
 * Inline form for answering review questions.
 */

import { api, ApiRequestError } from '../api';

export interface AnswerFormCallbacks {
  onSuccess: (docId: string, questionIndex: number, answer: string) => void;
  onError: (error: string) => void;
}

interface FormState {
  submitting: boolean;
  error: string | null;
}

const formStates = new Map<string, FormState>();

function getFormKey(docId: string, questionIndex: number): string {
  return `${docId}:${questionIndex}`;
}

function getFormState(docId: string, questionIndex: number): FormState {
  const key = getFormKey(docId, questionIndex);
  if (!formStates.has(key)) {
    formStates.set(key, { submitting: false, error: null });
  }
  return formStates.get(key)!;
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export function renderAnswerForm(
  docId: string,
  questionIndex: number,
  questionType: string
): string {
  const state = getFormState(docId, questionIndex);
  const formId = `answer-form-${escapeHtml(docId)}-${questionIndex}`;
  const inputId = `answer-input-${escapeHtml(docId)}-${questionIndex}`;
  const labelId = `answer-label-${escapeHtml(docId)}-${questionIndex}`;
  const hintId = `answer-hint-${escapeHtml(docId)}-${questionIndex}`;

  const placeholder = getPlaceholder(questionType);
  const disabledClass = state.submitting ? 'opacity-50 pointer-events-none' : '';

  return `
    <form id="${formId}" class="answer-form mt-3 ${disabledClass}" data-doc-id="${escapeHtml(docId)}" data-question-index="${questionIndex}">
      <div class="space-y-2">
        <label id="${labelId}" for="${inputId}" class="sr-only">Answer for ${escapeHtml(questionType)} question</label>
        <textarea
          id="${inputId}"
          name="answer"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="${escapeHtml(placeholder)}"
          aria-labelledby="${labelId}"
          aria-describedby="${hintId}"
          ${state.submitting ? 'disabled aria-busy="true"' : ''}
        ></textarea>
        <div id="${hintId}" class="sr-only">Press Ctrl+Enter to submit. Use Dismiss to skip or Delete fact to remove.</div>
        <div id="answer-hint-live-${escapeHtml(docId)}-${questionIndex}" class="text-xs text-gray-400 dark:text-gray-500 h-4" aria-live="polite"></div>
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <button
              type="submit"
              class="inline-flex items-center px-3 py-1.5 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50"
              ${state.submitting ? 'disabled aria-busy="true"' : ''}
            >
              ${state.submitting ? `
                <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24" aria-hidden="true">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                Submitting...
              ` : 'Submit'}
            </button>
            <span class="text-xs text-gray-400 dark:text-gray-500" aria-hidden="true">Ctrl+Enter</span>
          </div>
          <div class="flex items-center space-x-2">
            <button
              type="button"
              class="quick-action inline-flex items-center px-2 py-1 text-xs font-medium text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
              data-action="dismiss"
              aria-label="Dismiss this question"
              ${state.submitting ? 'disabled' : ''}
            >
              Dismiss
            </button>
            <button
              type="button"
              class="quick-action inline-flex items-center px-2 py-1 text-xs font-medium text-red-600 dark:text-red-400 hover:text-red-900 dark:hover:text-red-200 hover:bg-red-50 dark:hover:bg-red-900/20 rounded"
              data-action="delete"
              aria-label="Delete the referenced fact"
              ${state.submitting ? 'disabled' : ''}
            >
              Delete fact
            </button>
          </div>
        </div>
        ${state.error ? `
          <div class="text-sm text-red-600 dark:text-red-400" role="alert">${escapeHtml(state.error)}</div>
        ` : ''}
      </div>
    </form>
  `;
}

function getPlaceholder(questionType: string): string {
  switch (questionType) {
    case 'temporal':
      return 'e.g., "Started March 2022, left December 2024"';
    case 'conflict':
      return 'e.g., "Both were part-time, no conflict" or explain resolution';
    case 'missing':
      return 'e.g., "LinkedIn profile, checked 2024-01-15"';
    case 'ambiguous':
      return 'e.g., "Home address" or "split: home in Austin, work in SF"';
    case 'stale':
      return 'e.g., "Still accurate as of today" or provide update';
    case 'duplicate':
      return 'e.g., "Keep this one" or "Merge into [other_id]"';
    default:
      return 'Enter your answer...';
  }
}

async function submitAnswer(
  docId: string,
  questionIndex: number,
  answer: string,
  callbacks: AnswerFormCallbacks
): Promise<void> {
  const state = getFormState(docId, questionIndex);
  state.submitting = true;
  state.error = null;

  try {
    await api.answerQuestion(docId, questionIndex, answer);
    callbacks.onSuccess(docId, questionIndex, answer);
    // Clear state on success
    formStates.delete(getFormKey(docId, questionIndex));
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to submit answer';
    }
    callbacks.onError(state.error);
  } finally {
    state.submitting = false;
  }
}

export function setupAnswerFormHandlers(
  container: HTMLElement,
  callbacks: AnswerFormCallbacks
): void {
  // Handle form submissions
  container.addEventListener('submit', async (e) => {
    const form = (e.target as HTMLElement).closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;

    e.preventDefault();

    const docId = form.dataset.docId;
    const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
    const textarea = form.querySelector('textarea') as HTMLTextAreaElement | null;
    const answer = textarea?.value.trim() || '';

    if (!docId || !answer) return;

    await submitAnswer(docId, questionIndex, answer, callbacks);
  });

  // Handle quick action buttons
  container.addEventListener('click', async (e) => {
    const button = (e.target as HTMLElement).closest('.quick-action') as HTMLButtonElement | null;
    if (!button) return;

    const form = button.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;

    const docId = form.dataset.docId;
    const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
    const action = button.dataset.action;

    if (!docId || !action) return;

    const answer = action === 'dismiss' ? 'dismiss' : 'delete';
    await submitAnswer(docId, questionIndex, answer, callbacks);
  });

  // Handle live answer type hints
  container.addEventListener('input', (e) => {
    const textarea = e.target as HTMLTextAreaElement;
    if (textarea.tagName !== 'TEXTAREA') return;
    const form = textarea.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;
    const docId = form.dataset.docId;
    const qIdx = form.dataset.questionIndex;
    if (!docId || !qIdx) return;
    const hintEl = document.getElementById(`answer-hint-live-${docId}-${qIdx}`);
    if (hintEl) {
      hintEl.textContent = classifyAnswerHint(textarea.value.trim());
    }
  });

  // Handle Ctrl+Enter keyboard shortcut
  container.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      const textarea = e.target as HTMLTextAreaElement;
      if (textarea.tagName !== 'TEXTAREA') return;

      const form = textarea.closest('.answer-form') as HTMLFormElement | null;
      if (!form) return;

      e.preventDefault();

      const docId = form.dataset.docId;
      const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
      const answer = textarea.value.trim();

      if (!docId || !answer) return;

      await submitAnswer(docId, questionIndex, answer, callbacks);
    }
  });
}

function classifyAnswerHint(text: string): string {
  if (!text) return '';
  const lower = text.toLowerCase();
  if (lower === 'dismiss' || lower === 'ignore') return '→ Will dismiss this question';
  if (lower === 'delete' || lower === 'remove') return '→ Will delete the referenced fact';
  if (lower === 'defer' || lower.startsWith('defer ') || lower.startsWith('needs ')) return '→ Will defer for later review';
  if (/^(yes|confirmed|still accurate|correct)$/i.test(lower) || (lower.startsWith('yes') && text.length < 30))
    return '→ Will refresh last-seen date (@t[~])';
  if (/\d{4}[-/]\d{2}/.test(text) || /^(per |via |from |according to )/i.test(lower))
    return '→ Looks like a source citation — will add footnote';
  if (lower.startsWith('correct:') || lower.startsWith('correction:'))
    return '→ Will rewrite the fact with LLM assistance';
  return '';
}

export function clearFormStates(): void {
  formStates.clear();
}
