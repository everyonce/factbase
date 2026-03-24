/**
 * QuestionCard component.
 * Displays a single review question with type badge and context.
 */

import { ReviewQuestion } from '../api';
import { renderAnswerForm } from './AnswerForm';
import { renderSelectionCheckbox } from './BulkActions';

const QUESTION_TYPE_COLORS: Record<string, { bg: string; text: string }> = {
  temporal: { bg: 'bg-blue-100 dark:bg-blue-900', text: 'text-blue-700 dark:text-blue-200' },
  conflict: { bg: 'bg-red-100 dark:bg-red-900', text: 'text-red-700 dark:text-red-200' },
  missing: { bg: 'bg-amber-100 dark:bg-amber-900', text: 'text-amber-700 dark:text-amber-200' },
  ambiguous: { bg: 'bg-purple-100 dark:bg-purple-900', text: 'text-purple-700 dark:text-purple-200' },
  stale: { bg: 'bg-gray-100 dark:bg-gray-700', text: 'text-gray-700 dark:text-gray-200' },
  duplicate: { bg: 'bg-green-100 dark:bg-green-900', text: 'text-green-700 dark:text-green-200' },
  corruption: { bg: 'bg-orange-100 dark:bg-orange-900', text: 'text-orange-700 dark:text-orange-200' },
  precision: { bg: 'bg-teal-100 dark:bg-teal-900', text: 'text-teal-700 dark:text-teal-200' },
  'weak-source': { bg: 'bg-yellow-100 dark:bg-yellow-900', text: 'text-yellow-700 dark:text-yellow-200' },
};

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export function renderQuestionTypeBadge(type: string): string {
  const colors = QUESTION_TYPE_COLORS[type] || { bg: 'bg-gray-100 dark:bg-gray-700', text: 'text-gray-700 dark:text-gray-200' };
  return `<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${colors.bg} ${colors.text}">@q[${escapeHtml(type)}]</span>`;
}

const CONFIDENCE_STYLES: Record<string, { bg: string; text: string; dot: string }> = {
  high:   { bg: 'bg-green-100 dark:bg-green-900', text: 'text-green-700 dark:text-green-300', dot: 'bg-green-500' },
  medium: { bg: 'bg-amber-100 dark:bg-amber-900', text: 'text-amber-700 dark:text-amber-300', dot: 'bg-amber-500' },
  low:    { bg: 'bg-red-100 dark:bg-red-900',     text: 'text-red-700 dark:text-red-300',     dot: 'bg-red-500' },
};

/** Renders a subtle confidence badge. Returns '' for deferred/unknown. */
export function renderConfidenceBadge(confidence?: string): string {
  if (!confidence || confidence === 'deferred') return '';
  const style = CONFIDENCE_STYLES[confidence];
  if (!style) return '';
  return `<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium ${style.bg} ${style.text}" title="Agent confidence: ${confidence}">
    <span class="w-1.5 h-1.5 rounded-full ${style.dot}"></span>🤖 ${confidence}
  </span>`;
}

export interface QuestionCardOptions {
  showAnswerForm?: boolean;
  showCheckbox?: boolean;
}

export function renderQuestionCard(
  question: ReviewQuestion,
  docId: string,
  questionIndex: number,
  options: QuestionCardOptions = {}
): string {
  const { showAnswerForm = true, showCheckbox = false } = options;

  const answeredClass = question.answered ? 'opacity-60' : '';
  const answeredBadge = question.answered
    ? '<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-200 ml-2">Answered</span>'
    : '';

  const checkbox = showCheckbox ? renderSelectionCheckbox(docId, questionIndex, question) : '';

  // Line reference as clickable button to open preview
  const lineRef = question.line_ref
    ? `<button
        class="preview-line-btn text-xs text-blue-600 dark:text-blue-400 hover:underline"
        data-doc-id="${escapeHtml(docId)}"
        data-line-ref="${question.line_ref}"
      >Line ${question.line_ref}</button>`
    : '';

  const answerSection = question.answered && question.answer
    ? `<div class="mt-2 p-2 bg-gray-50 dark:bg-gray-800 rounded text-sm text-gray-600 dark:text-gray-400">
        <span class="font-medium">Answer:</span> ${escapeHtml(question.answer)}
      </div>`
    : showAnswerForm && !question.answered
      ? renderAnswerForm(docId, questionIndex, question.question_type)
      : '';

  return `
    <div class="question-card border border-gray-200 dark:border-gray-700 rounded-lg p-4 ${answeredClass}" data-doc-id="${escapeHtml(docId)}" data-question-index="${questionIndex}">
      <div class="flex items-start justify-between">
        <div class="flex items-center space-x-2">
          ${checkbox}
          ${renderQuestionTypeBadge(question.question_type)}
          ${answeredBadge}
          ${renderConfidenceBadge(question.confidence)}
          ${lineRef}
        </div>
      </div>
      <p class="mt-2 text-gray-700 dark:text-gray-300">${escapeHtml(question.description)}</p>
      ${answerSection}
    </div>
  `;
}
