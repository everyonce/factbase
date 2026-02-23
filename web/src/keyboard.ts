/**
 * Keyboard navigation module.
 * Provides j/k navigation, Enter to expand, Escape to cancel, ? for help.
 */

// Focusable item selector - items that can be navigated with j/k
const FOCUSABLE_SELECTOR = '.document-group, .suggestion-card, .orphan-card, .question-card';

// Current focused index per page
let focusedIndex = -1;
let keydownHandler: ((e: KeyboardEvent) => void) | null = null;
let helpModalOpen = false;

/**
 * Get all focusable items on the current page.
 */
function getFocusableItems(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}

/**
 * Focus an item by index, scrolling it into view.
 */
function focusItem(index: number): void {
  const items = getFocusableItems();
  if (items.length === 0) return;

  // Clamp index to valid range
  const newIndex = Math.max(0, Math.min(index, items.length - 1));
  
  // Remove focus from previous item
  if (focusedIndex >= 0 && focusedIndex < items.length) {
    items[focusedIndex].classList.remove('keyboard-focused');
  }

  // Focus new item
  focusedIndex = newIndex;
  const item = items[focusedIndex];
  item.classList.add('keyboard-focused');
  item.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  
  // Set tabindex for accessibility
  item.setAttribute('tabindex', '0');
  item.focus();
}

/**
 * Move focus to next item (j key).
 */
function focusNext(): void {
  const items = getFocusableItems();
  if (items.length === 0) return;
  
  if (focusedIndex < 0) {
    focusItem(0);
  } else {
    focusItem(focusedIndex + 1);
  }
}

/**
 * Move focus to previous item (k key).
 */
function focusPrevious(): void {
  const items = getFocusableItems();
  if (items.length === 0) return;
  
  if (focusedIndex < 0) {
    focusItem(items.length - 1);
  } else {
    focusItem(focusedIndex - 1);
  }
}

/**
 * Expand/activate the currently focused item (Enter key).
 * Clicks the first actionable button in the item.
 */
function expandFocused(): void {
  const items = getFocusableItems();
  if (focusedIndex < 0 || focusedIndex >= items.length) return;

  const item = items[focusedIndex];
  
  // Try to find and click an expand/preview button
  const previewBtn = item.querySelector<HTMLButtonElement>('.preview-doc-btn, .preview-line-btn, .compare-btn, .sections-btn');
  if (previewBtn) {
    previewBtn.click();
    return;
  }

  // Try to focus the answer input if present
  const answerInput = item.querySelector<HTMLTextAreaElement>('textarea[name="answer"]');
  if (answerInput) {
    answerInput.focus();
    return;
  }

  // Try to click the first button
  const firstBtn = item.querySelector<HTMLButtonElement>('button');
  if (firstBtn) {
    firstBtn.click();
  }
}

/**
 * Cancel current action (Escape key).
 * Closes modals/panels or clears focus.
 */
function cancelAction(): void {
  // Close help modal if open
  if (helpModalOpen) {
    closeHelpModal();
    return;
  }

  // Check if any preview panel is open and close it
  const previewPanel = document.getElementById('document-preview-panel');
  const mergePanel = document.getElementById('merge-preview-panel');
  const splitPanel = document.getElementById('split-preview-panel');
  
  if (previewPanel && !previewPanel.classList.contains('hidden')) {
    const closeBtn = previewPanel.querySelector<HTMLButtonElement>('.close-preview-btn');
    closeBtn?.click();
    return;
  }
  
  if (mergePanel && !mergePanel.classList.contains('hidden')) {
    const closeBtn = mergePanel.querySelector<HTMLButtonElement>('.close-preview-btn');
    closeBtn?.click();
    return;
  }
  
  if (splitPanel && !splitPanel.classList.contains('hidden')) {
    const closeBtn = splitPanel.querySelector<HTMLButtonElement>('.close-preview-btn');
    closeBtn?.click();
    return;
  }

  // Clear focus
  clearFocus();
}

/**
 * Clear keyboard focus from all items.
 */
function clearFocus(): void {
  const items = getFocusableItems();
  items.forEach(item => {
    item.classList.remove('keyboard-focused');
    item.removeAttribute('tabindex');
  });
  focusedIndex = -1;
}

/**
 * Show keyboard shortcuts help modal.
 */
function showHelpModal(): void {
  if (helpModalOpen) return;
  helpModalOpen = true;

  const modal = document.createElement('div');
  modal.id = 'keyboard-help-modal';
  modal.className = 'fixed inset-0 z-50 flex items-center justify-center bg-black/50';
  modal.setAttribute('role', 'dialog');
  modal.setAttribute('aria-modal', 'true');
  modal.setAttribute('aria-labelledby', 'keyboard-help-title');
  modal.innerHTML = `
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4 p-6" role="document">
      <div class="flex items-center justify-between mb-4">
        <h3 id="keyboard-help-title" class="text-lg font-semibold text-gray-900 dark:text-white">Keyboard Shortcuts</h3>
        <button id="close-help-modal" class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300" aria-label="Close keyboard shortcuts">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
          </svg>
        </button>
      </div>
      <div class="space-y-3" role="list" aria-label="Keyboard shortcuts list">
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Next item</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="j key">j</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Previous item</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="k key">k</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Expand / Preview</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Enter</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Close / Cancel</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Esc</kbd>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Show this help</span>
          <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="question mark key">?</kbd>
        </div>
        <hr class="border-gray-200 dark:border-gray-700" aria-hidden="true" />
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Submit answer</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Ctrl</kbd>
            <span class="text-gray-400" aria-hidden="true">+</span>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono">Enter</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Dashboard</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="d key">d</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Review</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="r key">r</kbd>
          </div>
        </div>
        <div class="flex items-center justify-between" role="listitem">
          <span class="text-gray-600 dark:text-gray-300">Go to Organize</span>
          <div class="flex items-center space-x-1">
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="g key">g</kbd>
            <kbd class="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono" aria-label="o key">o</kbd>
          </div>
        </div>
      </div>
      <p class="mt-4 text-xs text-gray-500 dark:text-gray-400">Press <kbd class="px-1 bg-gray-100 dark:bg-gray-700 rounded text-xs">Esc</kbd> to close</p>
    </div>
  `;

  document.body.appendChild(modal);

  // Close on backdrop click
  modal.addEventListener('click', (e) => {
    if (e.target === modal) {
      closeHelpModal();
    }
  });

  // Close button
  document.getElementById('close-help-modal')?.addEventListener('click', closeHelpModal);

  // Focus the close button for keyboard users
  document.getElementById('close-help-modal')?.focus();
}

/**
 * Close keyboard shortcuts help modal.
 */
function closeHelpModal(): void {
  const modal = document.getElementById('keyboard-help-modal');
  if (modal) {
    modal.remove();
  }
  helpModalOpen = false;
}

// Track 'g' key for navigation sequences (g+d, g+r, g+o)
let gKeyPressed = false;
let gKeyTimeout: number | null = null;

/**
 * Handle keydown events for keyboard navigation.
 */
function handleKeydown(e: KeyboardEvent): void {
  // Ignore if typing in an input/textarea
  const target = e.target as HTMLElement;
  if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT') {
    // Allow Escape to blur inputs
    if (e.key === 'Escape') {
      target.blur();
      e.preventDefault();
    }
    return;
  }

  // Handle 'g' key sequences for navigation
  if (gKeyPressed) {
    gKeyPressed = false;
    if (gKeyTimeout) {
      clearTimeout(gKeyTimeout);
      gKeyTimeout = null;
    }
    
    switch (e.key.toLowerCase()) {
      case 'd':
        window.location.hash = '#/';
        e.preventDefault();
        return;
      case 'r':
        window.location.hash = '#/review';
        e.preventDefault();
        return;
      case 'o':
        window.location.hash = '#/organize';
        e.preventDefault();
        return;
    }
  }

  switch (e.key.toLowerCase()) {
    case 'j':
      focusNext();
      e.preventDefault();
      break;
    case 'k':
      focusPrevious();
      e.preventDefault();
      break;
    case 'enter':
      if (focusedIndex >= 0) {
        expandFocused();
        e.preventDefault();
      }
      break;
    case 'escape':
      cancelAction();
      e.preventDefault();
      break;
    case '?':
      showHelpModal();
      e.preventDefault();
      break;
    case 'g':
      // Start 'g' key sequence
      gKeyPressed = true;
      gKeyTimeout = window.setTimeout(() => {
        gKeyPressed = false;
        gKeyTimeout = null;
      }, 500);
      e.preventDefault();
      break;
  }
}

/**
 * Initialize keyboard navigation.
 * Call this once when the app starts.
 */
export function initKeyboardNavigation(): void {
  if (keydownHandler) return; // Already initialized
  
  keydownHandler = handleKeydown;
  document.addEventListener('keydown', keydownHandler);
}

/**
 * Cleanup keyboard navigation.
 * Call this when navigating away or unmounting.
 */
export function cleanupKeyboardNavigation(): void {
  if (keydownHandler) {
    document.removeEventListener('keydown', keydownHandler);
    keydownHandler = null;
  }
  clearFocus();
  closeHelpModal();
  gKeyPressed = false;
  if (gKeyTimeout) {
    clearTimeout(gKeyTimeout);
    gKeyTimeout = null;
  }
}

/**
 * Reset focus state when page content changes.
 * Call this after re-rendering page content.
 */
export function resetFocus(): void {
  focusedIndex = -1;
  // Don't clear classes - let the new render handle it
}
