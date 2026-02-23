/**
 * OrphanCard component.
 * Renders an orphaned fact with assignment controls.
 */

import { OrphanEntry } from '../api';

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export interface OrphanCardOptions {
  showCheckbox?: boolean;
}

/**
 * Render an orphan entry card.
 */
export function renderOrphanCard(
  orphan: OrphanEntry,
  repoId: string,
  options: OrphanCardOptions = {}
): string {
  const { showCheckbox = false } = options;
  const checkboxId = `orphan-${repoId}-${orphan.line_number}`;

  const sourceInfo = orphan.source_doc
    ? `<span class="text-xs text-gray-500 dark:text-gray-400">from <button class="preview-source-btn text-blue-600 dark:text-blue-400 hover:underline" data-doc-id="${escapeHtml(orphan.source_doc)}" data-line="${orphan.source_line || ''}">${escapeHtml(orphan.source_doc)}</button>${orphan.source_line ? ` line ${orphan.source_line}` : ''}</span>`
    : '';

  const statusBadge = orphan.answered
    ? `<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300">
        Assigned: ${escapeHtml(orphan.answer || '')}
      </span>`
    : `<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-300">
        Pending
      </span>`;

  const checkbox = showCheckbox
    ? `<input type="checkbox" id="${checkboxId}" class="orphan-checkbox h-4 w-4 text-blue-600 rounded border-gray-300 dark:border-gray-600 dark:bg-gray-700 focus:ring-blue-500" data-repo="${escapeHtml(repoId)}" data-line="${orphan.line_number}">`
    : '';

  const assignForm = !orphan.answered
    ? `<div class="orphan-assign-form mt-3 pt-3 border-t border-gray-200 dark:border-gray-700">
        <div class="flex items-center space-x-2">
          <input
            type="text"
            class="orphan-target-input flex-1 text-sm rounded-md border-gray-300 dark:border-gray-600 dark:bg-gray-700 dark:text-white shadow-sm focus:border-blue-500 focus:ring-blue-500"
            placeholder="Document ID (6 chars) or 'dismiss'"
            data-repo="${escapeHtml(repoId)}"
            data-line="${orphan.line_number}"
          >
          <button
            class="orphan-assign-btn px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
            data-repo="${escapeHtml(repoId)}"
            data-line="${orphan.line_number}"
          >
            Assign
          </button>
          <button
            class="orphan-dismiss-btn px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 rounded-md hover:bg-gray-200 dark:hover:bg-gray-600"
            data-repo="${escapeHtml(repoId)}"
            data-line="${orphan.line_number}"
          >
            Dismiss
          </button>
        </div>
        <p class="orphan-error mt-1 text-sm text-red-600 dark:text-red-400 hidden"></p>
      </div>`
    : '';

  return `
    <div class="orphan-card p-4 bg-gray-50 dark:bg-gray-700/50 rounded-lg ${orphan.answered ? 'opacity-60' : ''}" data-line="${orphan.line_number}">
      <div class="flex items-start space-x-3">
        ${checkbox ? `<div class="pt-0.5">${checkbox}</div>` : ''}
        <div class="flex-1 min-w-0">
          <div class="flex items-center justify-between mb-2">
            ${statusBadge}
            ${sourceInfo}
          </div>
          <p class="text-sm text-gray-900 dark:text-white whitespace-pre-wrap">${escapeHtml(orphan.content)}</p>
          ${assignForm}
        </div>
      </div>
    </div>
  `;
}
