/**
 * TriageView component.
 * Groups review questions by action type rather than by document.
 *
 * Buckets:
 *  1. Quick approvals  — agent-answered, high confidence (space=approve, backspace=reject)
 *  2. Quick answers    — structured input, low/medium confidence or agent suggestion
 *  3. Research needed  — agent-deferred or genuinely ambiguous
 */

import { ReviewQuestion, DocumentReview } from '../api';
import { renderQuestionTypeBadge, renderConfidenceBadge } from './QuestionCard';
import { renderAnswerForm } from './AnswerForm';

export interface TriageItem {
  question: ReviewQuestion;
  docId: string;
  docTitle: string;
  questionIndex: number;
}

export interface TriageBuckets {
  quickApprovals: TriageItem[];
  quickAnswers: TriageItem[];
  researchNeeded: TriageItem[];
}

export interface SessionStats {
  approved: number;
  answered: number;
  skipped: number;
  startCount: number;
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/** Classify all unanswered questions into triage buckets. */
export function classifyQuestions(documents: DocumentReview[]): TriageBuckets {
  const quickApprovals: TriageItem[] = [];
  const quickAnswers: TriageItem[] = [];
  const researchNeeded: TriageItem[] = [];

  for (const doc of documents) {
    doc.questions.forEach((q, idx) => {
      if (q.answered) return;
      const item: TriageItem = {
        question: q,
        docId: doc.doc_id,
        docTitle: doc.doc_title,
        questionIndex: idx,
      };
      if (q.confidence === 'deferred') {
        researchNeeded.push(item);
      } else if (q.confidence === 'high' && q.answer) {
        quickApprovals.push(item);
      } else {
        quickAnswers.push(item);
      }
    });
  }

  return { quickApprovals, quickAnswers, researchNeeded };
}

// ============================================================================
// Quick approvals card stack
// ============================================================================

function renderApprovalCard(item: TriageItem, isCurrent: boolean): string {
  const { question, docId, docTitle, questionIndex } = item;
  const suggestion = question.answer ? escapeHtml(question.answer) : '';
  const confidenceReason = question.confidence_reason ? escapeHtml(question.confidence_reason) : '';

  return `
    <div class="triage-approval-card ${isCurrent ? 'block' : 'hidden'} bg-white dark:bg-gray-800 rounded-xl shadow-lg border-2 border-blue-200 dark:border-blue-700 p-5"
         data-doc-id="${escapeHtml(docId)}"
         data-question-index="${questionIndex}"
         data-triage-bucket="approval">
      <div class="flex items-center justify-between mb-3">
        <div class="flex items-center space-x-2">
          ${renderQuestionTypeBadge(question.question_type)}
          <span class="text-xs text-gray-500 dark:text-gray-400">${escapeHtml(docTitle)}</span>
        </div>
        <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-200">
          ✓ High confidence
        </span>
      </div>
      <p class="text-gray-800 dark:text-gray-200 mb-3">${escapeHtml(question.description)}</p>
      ${suggestion ? `
        <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-700 rounded-lg p-3 mb-3">
          <p class="text-xs font-medium text-blue-700 dark:text-blue-300 mb-1">Agent suggestion</p>
          <p class="text-sm text-blue-900 dark:text-blue-100">${suggestion}</p>
          ${confidenceReason ? `<p class="text-xs text-blue-600 dark:text-blue-400 mt-1">${confidenceReason}</p>` : ''}
        </div>
      ` : ''}
      <div class="flex items-center justify-between">
        <div class="flex items-center space-x-2">
          <button class="triage-approve-btn inline-flex items-center px-4 py-2 text-sm font-medium rounded-lg bg-green-600 text-white hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500"
                  data-doc-id="${escapeHtml(docId)}" data-question-index="${questionIndex}">
            ✓ Approve <kbd class="ml-2 text-xs opacity-75">Space</kbd>
          </button>
          <button class="triage-reject-btn inline-flex items-center px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400"
                  data-doc-id="${escapeHtml(docId)}" data-question-index="${questionIndex}">
            ✗ Reject <kbd class="ml-2 text-xs opacity-75">⌫</kbd>
          </button>
        </div>
        <button class="triage-skip-btn text-xs text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 underline"
                data-doc-id="${escapeHtml(docId)}" data-question-index="${questionIndex}">
          Skip for later
        </button>
      </div>
    </div>
  `;
}

function renderQuickApprovalsSection(items: TriageItem[], currentIndex: number): string {
  if (items.length === 0) return '';

  const remaining = items.length - currentIndex;
  if (remaining <= 0) {
    return `
      <section class="triage-section" aria-labelledby="triage-approvals-heading">
        <div class="flex items-center space-x-2 mb-3">
          <h3 id="triage-approvals-heading" class="text-base font-semibold text-gray-900 dark:text-white">⚡ Quick approvals</h3>
          <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-200">All done</span>
        </div>
      </section>
    `;
  }

  return `
    <section class="triage-section" aria-labelledby="triage-approvals-heading">
      <div class="flex items-center justify-between mb-3">
        <div class="flex items-center space-x-2">
          <h3 id="triage-approvals-heading" class="text-base font-semibold text-gray-900 dark:text-white">⚡ Quick approvals</h3>
          <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200">${remaining} remaining</span>
        </div>
        <p class="text-xs text-gray-500 dark:text-gray-400">Target: ~2 sec/card</p>
      </div>
      <div id="triage-approvals-stack" class="relative">
        ${items.map((item, i) => renderApprovalCard(item, i === currentIndex)).join('')}
        <div class="mt-2 flex items-center space-x-1">
          ${items.map((_, i) => `<span class="w-2 h-2 rounded-full ${i < currentIndex ? 'bg-green-400' : i === currentIndex ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'}"></span>`).join('')}
        </div>
      </div>
    </section>
  `;
}

// ============================================================================
// Quick answers section
// ============================================================================

function renderQuickAnswerCard(item: TriageItem): string {
  const { question, docId, docTitle, questionIndex } = item;
  const suggestion = question.answer ? escapeHtml(question.answer) : '';

  return `
    <div class="triage-answer-card bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-4"
         data-doc-id="${escapeHtml(docId)}"
         data-question-index="${questionIndex}"
         data-triage-bucket="answer">
      <div class="flex items-center space-x-2 mb-2">
        ${renderQuestionTypeBadge(question.question_type)}
        <span class="text-xs text-gray-500 dark:text-gray-400">${escapeHtml(docTitle)}</span>
        ${renderConfidenceBadge(question.confidence)}
        ${question.line_ref ? `<button class="preview-line-btn text-xs text-blue-600 dark:text-blue-400 hover:underline" data-doc-id="${escapeHtml(docId)}" data-line-ref="${question.line_ref}">Line ${question.line_ref}</button>` : ''}
      </div>
      <p class="text-gray-700 dark:text-gray-300 mb-3">${escapeHtml(question.description)}</p>
      ${suggestion ? `
        <div class="bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded p-2 mb-3 text-xs text-amber-800 dark:text-amber-200">
          <span class="font-medium">Suggestion:</span> ${suggestion}
        </div>
      ` : ''}
      ${renderAnswerForm(docId, questionIndex, question.question_type)}
    </div>
  `;
}

function renderQuickAnswersSection(items: TriageItem[]): string {
  if (items.length === 0) return '';

  return `
    <section class="triage-section" aria-labelledby="triage-answers-heading">
      <div class="flex items-center justify-between mb-3">
        <div class="flex items-center space-x-2">
          <h3 id="triage-answers-heading" class="text-base font-semibold text-gray-900 dark:text-white">✏️ Quick answers</h3>
          <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-200">${items.length}</span>
        </div>
        <p class="text-xs text-gray-500 dark:text-gray-400">Target: 10–30 sec/question</p>
      </div>
      <div class="space-y-3">
        ${items.map(item => renderQuickAnswerCard(item)).join('')}
      </div>
    </section>
  `;
}

// ============================================================================
// Research needed section
// ============================================================================

function renderResearchCard(item: TriageItem): string {
  const { question, docId, docTitle, questionIndex } = item;
  const agentNotes = question.confidence_reason ? escapeHtml(question.confidence_reason) : '';

  return `
    <div class="triage-research-card bg-white dark:bg-gray-800 rounded-lg shadow border border-amber-200 dark:border-amber-700 p-4"
         data-doc-id="${escapeHtml(docId)}"
         data-question-index="${questionIndex}"
         data-triage-bucket="research">
      <div class="flex items-center space-x-2 mb-2">
        ${renderQuestionTypeBadge(question.question_type)}
        <span class="text-xs text-gray-500 dark:text-gray-400">${escapeHtml(docTitle)}</span>
        ${question.line_ref ? `<button class="preview-line-btn text-xs text-blue-600 dark:text-blue-400 hover:underline" data-doc-id="${escapeHtml(docId)}" data-line-ref="${question.line_ref}">Line ${question.line_ref}</button>` : ''}
        <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-200">Needs research</span>
      </div>
      <p class="text-gray-700 dark:text-gray-300 mb-3">${escapeHtml(question.description)}</p>
      ${agentNotes ? `
        <div class="bg-gray-50 dark:bg-gray-700 rounded p-2 mb-3 text-xs text-gray-600 dark:text-gray-400">
          <span class="font-medium">Agent notes:</span> ${agentNotes}
        </div>
      ` : ''}
      ${renderAnswerForm(docId, questionIndex, question.question_type)}
    </div>
  `;
}

function renderResearchSection(items: TriageItem[]): string {
  if (items.length === 0) return '';

  return `
    <section class="triage-section" aria-labelledby="triage-research-heading">
      <div class="flex items-center justify-between mb-3">
        <div class="flex items-center space-x-2">
          <h3 id="triage-research-heading" class="text-base font-semibold text-gray-900 dark:text-white">🔍 Research needed</h3>
          <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-red-100 dark:bg-red-900 text-red-700 dark:text-red-200">${items.length}</span>
        </div>
        <p class="text-xs text-gray-500 dark:text-gray-400">Target: 1–5 min/question</p>
      </div>
      <div class="space-y-4">
        ${items.map(item => renderResearchCard(item)).join('')}
      </div>
    </section>
  `;
}

// ============================================================================
// Session summary
// ============================================================================

export function renderSessionSummary(stats: SessionStats): string {
  const total = stats.approved + stats.answered + stats.skipped;
  return `
    <div class="bg-green-50 dark:bg-green-900/30 border border-green-200 dark:border-green-800 rounded-xl p-6 text-center">
      <div class="text-4xl mb-3">🎉</div>
      <h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-2">Session complete!</h3>
      <p class="text-gray-600 dark:text-gray-300 mb-4">
        You reviewed ${total} question${total !== 1 ? 's' : ''}.
        ${stats.approved > 0 ? `<strong>${stats.approved}</strong> approved, ` : ''}
        ${stats.answered > 0 ? `<strong>${stats.answered}</strong> answered, ` : ''}
        ${stats.skipped > 0 ? `<strong>${stats.skipped}</strong> skipped for later.` : ''}
      </p>
      <p class="text-sm text-gray-500 dark:text-gray-400">Skipped items remain in the queue.</p>
    </div>
  `;
}

// ============================================================================
// Main render
// ============================================================================

function renderProgressBar(remaining: number, startCount: number): string {
  if (startCount === 0) return '';
  const answered = startCount - remaining;
  const pct = Math.round((answered / startCount) * 100);
  return `
    <div class="sticky top-0 z-10 bg-white dark:bg-gray-900 border-b border-gray-200 dark:border-gray-700 px-1 py-2 mb-2">
      <div class="flex items-center justify-between text-sm mb-1">
        <span class="font-medium text-gray-700 dark:text-gray-300">${answered} of ${startCount} questions answered</span>
        <span class="text-gray-500 dark:text-gray-400">${pct}%</span>
      </div>
      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
        <div class="bg-blue-500 h-2 rounded-full transition-all duration-300" style="width: ${pct}%"></div>
      </div>
    </div>
  `;
}

export function renderTriageView(
  buckets: TriageBuckets,
  approvalIndex: number,
  sessionStats: SessionStats | null,
  sessionStartCount: number = 0
): string {
  const total = buckets.quickApprovals.length + buckets.quickAnswers.length + buckets.researchNeeded.length;

  if (total === 0 && sessionStats) {
    return renderSessionSummary(sessionStats);
  }

  if (total === 0) {
    return `
      <div class="text-center py-8">
        <span class="text-4xl">✅</span>
        <p class="mt-2 text-gray-600 dark:text-gray-300 font-medium">No pending review questions</p>
        <p class="text-sm text-gray-500 dark:text-gray-400">Run <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase check</code> to generate questions</p>
      </div>
    `;
  }

  return `
    <div class="space-y-6" id="triage-view">
      ${renderProgressBar(total, sessionStartCount)}
      ${renderQuickApprovalsSection(buckets.quickApprovals, approvalIndex)}
      ${renderQuickAnswersSection(buckets.quickAnswers)}
      ${renderResearchSection(buckets.researchNeeded)}
    </div>
  `;
}
