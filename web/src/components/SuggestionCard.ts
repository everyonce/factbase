/**
 * SuggestionCard component.
 * Renders merge and misplaced suggestion cards with type badges and confidence scores.
 */

import { MergeCandidate, MisplacedCandidate } from '../api';

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Render a badge for suggestion type.
 */
export function renderSuggestionTypeBadge(type: 'merge' | 'misplaced' | 'duplicate'): string {
  const colors: Record<string, string> = {
    merge: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
    misplaced: 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200',
    duplicate: 'bg-rose-100 text-rose-800 dark:bg-rose-900 dark:text-rose-200',
  };

  const icons: Record<string, string> = {
    merge: '🔗',
    misplaced: '📁',
    duplicate: '👥',
  };

  return `<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${colors[type]}">
    <span class="mr-1">${icons[type]}</span>${type}
  </span>`;
}

/**
 * Format similarity score as percentage.
 */
function formatSimilarity(similarity: number): string {
  return `${Math.round(similarity * 100)}%`;
}

/**
 * Get color class for similarity score.
 */
function getSimilarityColor(similarity: number): string {
  if (similarity >= 0.95) return 'text-red-600 dark:text-red-400';
  if (similarity >= 0.90) return 'text-amber-600 dark:text-amber-400';
  return 'text-gray-600 dark:text-gray-400';
}

export interface SuggestionCardOptions {
  showDismiss?: boolean;
  showApprove?: boolean;
  showCompare?: boolean;
  showSections?: boolean;
}

/**
 * Render a merge suggestion card.
 */
export function renderMergeCard(
  suggestion: MergeCandidate,
  index: number,
  options: SuggestionCardOptions = {}
): string {
  const { showDismiss = true, showApprove = true, showCompare = true } = options;
  const similarityColor = getSimilarityColor(suggestion.similarity);

  return `
    <div class="suggestion-card merge-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="merge" data-index="${index}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${renderSuggestionTypeBadge('merge')}
            <span class="text-sm font-medium ${similarityColor}">
              ${formatSimilarity(suggestion.similarity)} similar
            </span>
          </div>
          <div class="space-y-2">
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 1:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${escapeHtml(suggestion.doc1_id)}"
              >
                ${escapeHtml(suggestion.doc1_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${escapeHtml(suggestion.doc1_id)}]</span>
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-gray-500 dark:text-gray-400 text-sm">Doc 2:</span>
              <button
                class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
                data-doc-id="${escapeHtml(suggestion.doc2_id)}"
              >
                ${escapeHtml(suggestion.doc2_title)}
              </button>
              <span class="text-gray-400 dark:text-gray-500 text-xs">[${escapeHtml(suggestion.doc2_id)}]</span>
            </div>
          </div>
          <p class="mt-2 text-sm text-gray-600 dark:text-gray-300">
            These documents have high content similarity and may be candidates for merging.
          </p>
        </div>
        ${(showDismiss || showApprove || showCompare) ? `
        <div class="flex items-center space-x-2 ml-4">
          ${showCompare ? `
          <button
            class="compare-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-doc1="${escapeHtml(suggestion.doc1_id)}"
            data-doc2="${escapeHtml(suggestion.doc2_id)}"
            title="Compare documents side-by-side"
          >
            Compare
          </button>
          ` : ''}
          ${showApprove ? `
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="merge"
            data-doc1="${escapeHtml(suggestion.doc1_id)}"
            data-doc2="${escapeHtml(suggestion.doc2_id)}"
            title="Approve merge (requires CLI)"
          >
            Approve
          </button>
          ` : ''}
          ${showDismiss ? `
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="merge"
            data-doc-id="${escapeHtml(suggestion.doc1_id)}"
            data-target-id="${escapeHtml(suggestion.doc2_id)}"
          >
            Dismiss
          </button>
          ` : ''}
        </div>
        ` : ''}
      </div>
    </div>
  `;
}

/**
 * Render a misplaced suggestion card.
 */
export function renderMisplacedCard(
  suggestion: MisplacedCandidate,
  index: number,
  options: SuggestionCardOptions = {}
): string {
  const { showDismiss = true, showApprove = true, showSections = true } = options;

  return `
    <div class="suggestion-card misplaced-card bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-4" data-type="misplaced" data-index="${index}">
      <div class="flex items-start justify-between">
        <div class="flex-1">
          <div class="flex items-center space-x-2 mb-2">
            ${renderSuggestionTypeBadge('misplaced')}
          </div>
          <div class="flex items-center space-x-2 mb-2">
            <button
              class="preview-doc-btn text-blue-600 dark:text-blue-400 hover:underline text-sm font-medium"
              data-doc-id="${escapeHtml(suggestion.doc_id)}"
            >
              ${escapeHtml(suggestion.doc_title)}
            </button>
            <span class="text-gray-400 dark:text-gray-500 text-xs">[${escapeHtml(suggestion.doc_id)}]</span>
          </div>
          <div class="flex items-center space-x-2 text-sm">
            <span class="text-gray-500 dark:text-gray-400">Current type:</span>
            <span class="px-2 py-0.5 bg-gray-100 dark:bg-gray-700 rounded text-gray-700 dark:text-gray-300">${escapeHtml(suggestion.current_type)}</span>
            <span class="text-gray-400 dark:text-gray-500">→</span>
            <span class="text-gray-500 dark:text-gray-400">Suggested:</span>
            <span class="px-2 py-0.5 bg-green-100 dark:bg-green-900 rounded text-green-700 dark:text-green-300">${escapeHtml(suggestion.suggested_type)}</span>
          </div>
          <p class="mt-2 text-sm text-gray-600 dark:text-gray-300">
            ${escapeHtml(suggestion.reason)}
          </p>
        </div>
        ${(showDismiss || showApprove || showSections) ? `
        <div class="flex items-center space-x-2 ml-4">
          ${showSections ? `
          <button
            class="sections-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-doc-id="${escapeHtml(suggestion.doc_id)}"
            title="View document sections"
          >
            Sections
          </button>
          ` : ''}
          ${showApprove ? `
          <button
            class="approve-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            data-type="misplaced"
            data-doc-id="${escapeHtml(suggestion.doc_id)}"
            data-suggested-type="${escapeHtml(suggestion.suggested_type)}"
            title="Approve retype (requires CLI)"
          >
            Approve
          </button>
          ` : ''}
          ${showDismiss ? `
          <button
            class="dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            data-type="misplaced"
            data-doc-id="${escapeHtml(suggestion.doc_id)}"
          >
            Dismiss
          </button>
          ` : ''}
        </div>
        ` : ''}
      </div>
    </div>
  `;
}
