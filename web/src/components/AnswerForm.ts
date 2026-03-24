/**
 * AnswerForm component.
 * Inline form for answering review questions with type-specific input controls.
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

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

// ============================================================================
// Structured input renderers per question type
// ============================================================================

const INPUT_CLS = 'rounded border border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white text-sm px-2 py-1 focus:border-blue-500 focus:ring-1 focus:ring-blue-500';
const LABEL_CLS = 'text-sm text-gray-600 dark:text-gray-400';
const RADIO_LABEL_CLS = 'flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300 cursor-pointer';

function renderTemporalInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <div class="flex flex-wrap gap-3 items-center">
        <label class="${LABEL_CLS}">
          Start year
          <input type="number" name="t-start" placeholder="YYYY" min="1" max="2200"
            class="${INPUT_CLS} w-24 ml-1" aria-label="Start year">
        </label>
        <label class="${LABEL_CLS}">
          End year
          <input type="number" name="t-end" placeholder="YYYY (optional)" min="1" max="2200"
            class="${INPUT_CLS} w-32 ml-1" aria-label="End year (optional)">
        </label>
        <label class="flex items-center gap-1 text-sm text-gray-600 dark:text-gray-400 cursor-pointer">
          <input type="checkbox" name="t-unknown" class="rounded"> Unknown
        </label>
      </div>
      <div class="text-xs text-gray-400 dark:text-gray-500">
        Preview: <span class="t-preview font-mono">—</span>
      </div>
    </div>`;
}

function renderStaleInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <div class="flex flex-wrap gap-2 items-center">
        <button type="button" data-action="still-accurate"
          class="inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md border border-green-300 dark:border-green-700 text-green-700 dark:text-green-300 bg-green-50 dark:bg-green-900/20 hover:bg-green-100 dark:hover:bg-green-900/40">
          ✓ Still accurate
        </button>
        <input type="url" name="s-source" placeholder="Source URL (optional)"
          class="${INPUT_CLS} flex-1 min-w-0" aria-label="Source URL">
      </div>
      <div class="text-xs text-gray-400 dark:text-gray-500">
        One click to verify with today's date. Add a source URL for attribution.
      </div>
    </div>`;
}

function renderConflictInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <label class="${RADIO_LABEL_CLS}">
        <input type="radio" name="c-resolution" value="not-a-conflict" class="text-blue-600">
        Not a conflict (sequential / overlapping boundary)
      </label>
      <label class="${RADIO_LABEL_CLS}">
        <input type="radio" name="c-resolution" value="correct-date" class="text-blue-600">
        Correct a date:
        <input type="text" name="c-date" placeholder="e.g. ended 2021-06"
          class="${INPUT_CLS} flex-1" aria-label="Date correction">
      </label>
      <label class="${RADIO_LABEL_CLS}">
        <input type="radio" name="c-resolution" value="context" class="text-blue-600">
        Add context:
        <input type="text" name="c-context" placeholder="Explain the resolution"
          class="${INPUT_CLS} flex-1" aria-label="Context">
      </label>
    </div>`;
}

function renderMissingInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <input type="url" name="m-url" placeholder="Source URL"
        class="${INPUT_CLS} w-full" aria-label="Source URL">
      <div class="flex flex-wrap gap-2">
        <input type="text" name="m-title" placeholder="Title (optional)"
          class="${INPUT_CLS} flex-1 min-w-0" aria-label="Source title">
        <input type="text" name="m-date" placeholder="Date (YYYY-MM-DD, optional)"
          class="${INPUT_CLS} w-40" aria-label="Source date">
      </div>
      <div class="text-xs text-gray-400 dark:text-gray-500">
        Leave URL blank to dismiss without a source.
      </div>
    </div>`;
}

function renderAmbiguousInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <input type="text" name="a-definition" placeholder="What does this term mean in context?"
        class="${INPUT_CLS} w-full" aria-label="Term definition">
      <div class="text-xs text-gray-400 dark:text-gray-500">
        Clarify the ambiguous term. Leave blank to dismiss.
      </div>
    </div>`;
}

function renderPrecisionInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <label class="${RADIO_LABEL_CLS}">
        <input type="radio" name="p-path" value="specific" class="text-blue-600">
        Specific value:
        <input type="text" name="p-value" placeholder="e.g. 500 casualties"
          class="${INPUT_CLS} flex-1" aria-label="Specific value">
      </label>
      <label class="${RADIO_LABEL_CLS}">
        <input type="radio" name="p-path" value="editorial" class="text-blue-600">
        Editorial / not quantifiable:
        <input type="text" name="p-reason" placeholder="Reason (optional)"
          class="${INPUT_CLS} flex-1" aria-label="Reason">
      </label>
    </div>`;
}

function renderWeakSourceInputs(_id: string): string {
  return `
    <div class="space-y-2">
      <input type="url" name="w-url" placeholder="Better source URL"
        class="${INPUT_CLS} w-full" aria-label="Improved source URL">
      <label class="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400 cursor-pointer">
        <input type="checkbox" name="w-cannot" class="rounded">
        Cannot improve this citation (mark as reviewed)
      </label>
    </div>`;
}

function renderStructuredInputs(type: string, id: string): string {
  switch (type) {
    case 'temporal':   return renderTemporalInputs(id);
    case 'stale':      return renderStaleInputs(id);
    case 'conflict':   return renderConflictInputs(id);
    case 'missing':    return renderMissingInputs(id);
    case 'ambiguous':  return renderAmbiguousInputs(id);
    case 'precision':  return renderPrecisionInputs(id);
    case 'weak-source': return renderWeakSourceInputs(id);
    default:           return '';
  }
}

// ============================================================================
// Answer collection from structured inputs
// ============================================================================

function collectTemporal(form: HTMLFormElement): string {
  const unknown = (form.querySelector('input[name="t-unknown"]') as HTMLInputElement | null)?.checked;
  if (unknown) return '?';
  const start = (form.querySelector('input[name="t-start"]') as HTMLInputElement | null)?.value.trim();
  const end = (form.querySelector('input[name="t-end"]') as HTMLInputElement | null)?.value.trim();
  if (!start && !end) return '';
  if (start && end) return `started ${start}, ended ${end}`;
  if (start) return `started ${start}`;
  return `ended ${end}`;
}

function collectStale(form: HTMLFormElement, source?: string): string {
  const src = source ?? (form.querySelector('input[name="s-source"]') as HTMLInputElement | null)?.value.trim() ?? '';
  const today = todayIso();
  if (src) return `still accurate per ${src}; verified ${today}`;
  return `still accurate; verified ${today}`;
}

function collectConflict(form: HTMLFormElement): string {
  const resolution = (form.querySelector('input[name="c-resolution"]:checked') as HTMLInputElement | null)?.value;
  if (!resolution) return '';
  if (resolution === 'not-a-conflict') return 'not a conflict';
  if (resolution === 'correct-date') {
    const date = (form.querySelector('input[name="c-date"]') as HTMLInputElement | null)?.value.trim();
    return date ? `correct: ${date}` : 'not a conflict';
  }
  if (resolution === 'context') {
    const ctx = (form.querySelector('input[name="c-context"]') as HTMLInputElement | null)?.value.trim();
    return ctx ? ctx : 'not a conflict';
  }
  return '';
}

function collectMissing(form: HTMLFormElement): string {
  const url = (form.querySelector('input[name="m-url"]') as HTMLInputElement | null)?.value.trim();
  if (!url) return 'dismiss';
  const title = (form.querySelector('input[name="m-title"]') as HTMLInputElement | null)?.value.trim();
  const date = (form.querySelector('input[name="m-date"]') as HTMLInputElement | null)?.value.trim();
  const label = title || url;
  if (date) return `Source: ${label} (${url}), ${date}`;
  return `Source: ${label} (${url})`;
}

function collectAmbiguous(form: HTMLFormElement): string {
  const def = (form.querySelector('input[name="a-definition"]') as HTMLInputElement | null)?.value.trim();
  return def || 'dismiss';
}

function collectPrecision(form: HTMLFormElement): string {
  const path = (form.querySelector('input[name="p-path"]:checked') as HTMLInputElement | null)?.value;
  if (path === 'specific') {
    const val = (form.querySelector('input[name="p-value"]') as HTMLInputElement | null)?.value.trim();
    return val ? `correct: ${val}` : '';
  }
  if (path === 'editorial') {
    const reason = (form.querySelector('input[name="p-reason"]') as HTMLInputElement | null)?.value.trim();
    return reason ? `dismiss: ${reason}` : 'dismiss';
  }
  return '';
}

function collectWeakSource(form: HTMLFormElement): string {
  const cannot = (form.querySelector('input[name="w-cannot"]') as HTMLInputElement | null)?.checked;
  if (cannot) return 'dismiss';
  const url = (form.querySelector('input[name="w-url"]') as HTMLInputElement | null)?.value.trim();
  return url ? `per ${url}` : '';
}

function collectAnswerFromForm(form: HTMLFormElement): string {
  // If freeform fallback is visible, use it
  const freeformDiv = form.querySelector('.freeform-fallback');
  if (freeformDiv && !freeformDiv.classList.contains('hidden')) {
    const textarea = freeformDiv.querySelector('textarea') as HTMLTextAreaElement | null;
    return textarea?.value.trim() || '';
  }

  const type = form.dataset.questionType || '';
  switch (type) {
    case 'temporal':    return collectTemporal(form);
    case 'stale':       return collectStale(form);
    case 'conflict':    return collectConflict(form);
    case 'missing':     return collectMissing(form);
    case 'ambiguous':   return collectAmbiguous(form);
    case 'precision':   return collectPrecision(form);
    case 'weak-source': return collectWeakSource(form);
    default: {
      const textarea = form.querySelector('textarea') as HTMLTextAreaElement | null;
      return textarea?.value.trim() || '';
    }
  }
}

// ============================================================================
// Render
// ============================================================================

export function renderAnswerForm(
  docId: string,
  questionIndex: number,
  questionType: string
): string {
  const state = getFormState(docId, questionIndex);
  const formId = `answer-form-${escapeHtml(docId)}-${questionIndex}`;
  const id = `${escapeHtml(docId)}-${questionIndex}`;
  const disabledClass = state.submitting ? 'opacity-50 pointer-events-none' : '';

  const structuredHtml = renderStructuredInputs(questionType, id);
  const hasStructured = structuredHtml !== '';

  const inputSection = hasStructured
    ? `
      <div class="structured-inputs space-y-2">
        ${structuredHtml}
      </div>
      <div class="freeform-fallback hidden">
        <textarea
          name="answer"
          rows="2"
          class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
          placeholder="Enter your answer..."
          ${state.submitting ? 'disabled' : ''}
        ></textarea>
      </div>`
    : `
      <textarea
        name="answer"
        rows="2"
        class="block w-full rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500 text-sm resize-none"
        placeholder="${escapeHtml(getFreeformPlaceholder(questionType))}"
        ${state.submitting ? 'disabled' : ''}
      ></textarea>`;

  const toggleBtn = hasStructured
    ? `<button type="button" class="toggle-freeform text-xs text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 underline">
        Freeform
      </button>`
    : '';

  return `
    <form id="${formId}" class="answer-form mt-3 ${disabledClass}"
          data-doc-id="${escapeHtml(docId)}"
          data-question-index="${questionIndex}"
          data-question-type="${escapeHtml(questionType)}">
      <div class="space-y-2">
        ${inputSection}
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
            ${toggleBtn}
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

function getFreeformPlaceholder(questionType: string): string {
  switch (questionType) {
    case 'temporal':    return 'e.g., "Started March 2022, left December 2024"';
    case 'conflict':    return 'e.g., "Not a conflict — sequential roles" or date correction';
    case 'missing':     return 'e.g., "Source: Title (https://...), 2024-01-15"';
    case 'ambiguous':   return 'e.g., "Home address" or "split: home in Austin, work in SF"';
    case 'stale':       return 'e.g., "still accurate; verified 2026-03-23"';
    case 'duplicate':   return 'e.g., "Keep this one" or "Merge into [other_id]"';
    case 'precision':   return 'e.g., "Heavy means >500 casualties" or specific number';
    default:            return 'Enter your answer...';
  }
}

// ============================================================================
// Submit / state management
// ============================================================================

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

// ============================================================================
// Event handlers
// ============================================================================

export function setupAnswerFormHandlers(
  container: HTMLElement,
  callbacks: AnswerFormCallbacks
): void {
  // Form submit
  container.addEventListener('submit', async (e) => {
    const form = (e.target as HTMLElement).closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;
    e.preventDefault();

    const docId = form.dataset.docId;
    const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
    if (!docId) return;

    const answer = collectAnswerFromForm(form);
    if (!answer) return;

    await submitAnswer(docId, questionIndex, answer, callbacks);
  });

  // Quick actions (dismiss, delete, still-accurate)
  container.addEventListener('click', async (e) => {
    const button = (e.target as HTMLElement).closest('[data-action]') as HTMLButtonElement | null;
    if (!button) return;

    const form = button.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;

    const docId = form.dataset.docId;
    const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
    const action = button.dataset.action;
    if (!docId || !action) return;

    if (action === 'dismiss') {
      await submitAnswer(docId, questionIndex, 'dismiss', callbacks);
    } else if (action === 'delete') {
      await submitAnswer(docId, questionIndex, 'delete', callbacks);
    } else if (action === 'still-accurate') {
      const answer = collectStale(form);
      await submitAnswer(docId, questionIndex, answer, callbacks);
    }
  });

  // Toggle freeform / structured
  container.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.toggle-freeform') as HTMLButtonElement | null;
    if (!btn) return;
    const form = btn.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;

    const structured = form.querySelector('.structured-inputs') as HTMLElement | null;
    const freeform = form.querySelector('.freeform-fallback') as HTMLElement | null;
    if (!structured || !freeform) return;

    const showingFreeform = !freeform.classList.contains('hidden');
    if (showingFreeform) {
      freeform.classList.add('hidden');
      structured.classList.remove('hidden');
      btn.textContent = 'Freeform';
    } else {
      structured.classList.add('hidden');
      freeform.classList.remove('hidden');
      btn.textContent = 'Structured';
    }
  });

  // Live temporal preview
  container.addEventListener('input', (e) => {
    const input = e.target as HTMLInputElement;
    if (!input.name?.startsWith('t-')) return;
    const form = input.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;
    const preview = form.querySelector('.t-preview');
    if (!preview) return;

    const unknown = (form.querySelector('input[name="t-unknown"]') as HTMLInputElement | null)?.checked;
    if (unknown) { preview.textContent = '@t[?]'; return; }
    const start = (form.querySelector('input[name="t-start"]') as HTMLInputElement | null)?.value.trim();
    const end = (form.querySelector('input[name="t-end"]') as HTMLInputElement | null)?.value.trim();
    if (!start && !end) { preview.textContent = '—'; return; }
    if (start && end) { preview.textContent = `@t[${start}..${end}]`; return; }
    if (start) { preview.textContent = `@t[=${start}]`; return; }
    preview.textContent = `@t[..${end}]`;
  });

  // Ctrl+Enter on any input/textarea inside a form
  container.addEventListener('keydown', async (e) => {
    if (!(e.key === 'Enter' && (e.ctrlKey || e.metaKey))) return;
    const el = e.target as HTMLElement;
    if (el.tagName !== 'TEXTAREA' && el.tagName !== 'INPUT') return;

    const form = el.closest('.answer-form') as HTMLFormElement | null;
    if (!form) return;
    e.preventDefault();

    const docId = form.dataset.docId;
    const questionIndex = parseInt(form.dataset.questionIndex || '0', 10);
    if (!docId) return;

    const answer = collectAnswerFromForm(form);
    if (!answer) return;

    await submitAnswer(docId, questionIndex, answer, callbacks);
  });
}

export function clearFormStates(): void {
  formStates.clear();
}
