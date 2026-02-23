/**
 * DocumentPreview component.
 * Side panel showing document content with line highlighting and links.
 */

import { api, Document, DocumentLink, ApiRequestError } from '../api';

interface PreviewState {
  loading: boolean;
  error: string | null;
  document: Document | null;
  highlightLine: number | null;
}

const state: PreviewState = {
  loading: false,
  error: null,
  document: null,
  highlightLine: null,
};

let isOpen = false;
let panelElement: HTMLElement | null = null;

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function renderLinks(links: DocumentLink[]): string {
  if (!links || links.length === 0) {
    return `<p class="text-sm text-gray-500 dark:text-gray-400">None</p>`;
  }
  return `
    <ul class="space-y-1">
      ${links.map(link => `
        <li>
          <button
            class="preview-link-btn text-sm text-blue-600 dark:text-blue-400 hover:underline text-left"
            data-doc-id="${escapeHtml(link.id)}"
          >
            ${escapeHtml(link.title)}
          </button>
        </li>
      `).join('')}
    </ul>
  `;
}

function renderContent(content: string, highlightLine: number | null): string {
  const lines = content.split('\n');
  return lines.map((line, index) => {
    const lineNum = index + 1;
    const isHighlighted = highlightLine !== null && lineNum === highlightLine;
    const highlightClass = isHighlighted
      ? 'bg-yellow-100 dark:bg-yellow-900/50 border-l-4 border-yellow-400'
      : '';
    const lineNumClass = isHighlighted
      ? 'text-yellow-600 dark:text-yellow-400 font-bold'
      : 'text-gray-400 dark:text-gray-600';
    return `
      <div class="flex ${highlightClass}" ${isHighlighted ? 'id="highlighted-line"' : ''}>
        <span class="select-none w-10 flex-shrink-0 text-right pr-3 ${lineNumClass} text-xs leading-6">${lineNum}</span>
        <pre class="flex-1 text-sm leading-6 whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">${escapeHtml(line) || ' '}</pre>
      </div>
    `;
  }).join('');
}

function renderPanel(): string {
  if (state.loading) {
    return `
      <div class="flex items-center justify-center h-64" role="status" aria-live="polite">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600" aria-hidden="true"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;
  }

  if (state.error) {
    return `
      <div class="p-4 text-center" role="alert">
        <p class="text-red-600 dark:text-red-400">${escapeHtml(state.error)}</p>
        <button id="preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;
  }

  if (!state.document) {
    return `<div class="p-4 text-gray-500 dark:text-gray-400">No document selected</div>`;
  }

  const doc = state.document;
  return `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-start justify-between">
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white truncate" id="preview-title">${escapeHtml(doc.title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 truncate">${escapeHtml(doc.file_path)}</p>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300">
                ${escapeHtml(doc.doc_type)}
              </span>
              <span class="text-xs text-gray-500 dark:text-gray-400">${escapeHtml(doc.id)}</span>
            </div>
          </div>
          <button id="preview-close-btn" class="ml-2 p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200" aria-label="Close preview">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Links section -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="grid grid-cols-2 gap-4">
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="links-to-heading">Links to</h4>
            <nav aria-labelledby="links-to-heading">
              ${renderLinks(doc.links_to || [])}
            </nav>
          </div>
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="linked-from-heading">Linked from</h4>
            <nav aria-labelledby="linked-from-heading">
              ${renderLinks(doc.linked_from || [])}
            </nav>
          </div>
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-auto p-4 font-mono bg-white dark:bg-gray-900" role="region" aria-label="Document content">
        ${doc.content ? renderContent(doc.content, state.highlightLine) : '<p class="text-gray-500 dark:text-gray-400">No content available</p>'}
      </div>
    </div>
  `;
}

function updatePanel(): void {
  if (!panelElement) return;
  const contentEl = panelElement.querySelector('#preview-panel-content');
  if (contentEl) {
    contentEl.innerHTML = renderPanel();
    setupPanelHandlers();

    // Scroll to highlighted line after render
    if (state.highlightLine !== null && !state.loading) {
      setTimeout(() => {
        const highlightedEl = document.getElementById('highlighted-line');
        highlightedEl?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }, 100);
    }
  }
}

function setupPanelHandlers(): void {
  // Close button
  document.getElementById('preview-close-btn')?.addEventListener('click', closePreview);
  document.getElementById('preview-close-error')?.addEventListener('click', closePreview);

  // Link buttons - navigate to linked document
  document.querySelectorAll('.preview-link-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const docId = (e.currentTarget as HTMLElement).dataset.docId;
      if (docId) {
        openPreview(docId);
      }
    });
  });
}

async function fetchDocument(docId: string): Promise<void> {
  state.loading = true;
  state.error = null;
  state.document = null;
  updatePanel();

  try {
    const doc = await api.getDocument(docId);
    const links = await api.getDocumentLinks(docId);
    state.document = {
      ...doc,
      links_to: links.links_to,
      linked_from: links.linked_from,
    };
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load document';
    }
  } finally {
    state.loading = false;
    updatePanel();
  }
}

export function openPreview(docId: string, highlightLine?: number): void {
  state.highlightLine = highlightLine ?? null;

  if (!isOpen) {
    // Create panel if not exists
    createPanel();
    isOpen = true;
  }

  fetchDocument(docId);
}

export function closePreview(): void {
  if (panelElement) {
    panelElement.classList.add('translate-x-full');
    setTimeout(() => {
      panelElement?.remove();
      panelElement = null;
    }, 300);
  }
  isOpen = false;
  state.document = null;
  state.error = null;
  state.highlightLine = null;
}

function createPanel(): void {
  // Remove existing panel if any
  panelElement?.remove();

  // Create panel element
  panelElement = document.createElement('div');
  panelElement.id = 'document-preview-panel';
  panelElement.className = `
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g, ' ');
  panelElement.setAttribute('role', 'dialog');
  panelElement.setAttribute('aria-modal', 'true');
  panelElement.setAttribute('aria-labelledby', 'preview-title');

  panelElement.innerHTML = `
    <div id="preview-panel-content" class="h-full flex flex-col">
      ${renderPanel()}
    </div>
  `;

  document.body.appendChild(panelElement);

  // Trigger animation
  requestAnimationFrame(() => {
    panelElement?.classList.remove('translate-x-full');
  });

  // Setup handlers
  setupPanelHandlers();

  // Close on escape key
  const handleEscape = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && isOpen) {
      closePreview();
    }
  };
  document.addEventListener('keydown', handleEscape);

  // Store cleanup function
  (panelElement as any)._cleanup = () => {
    document.removeEventListener('keydown', handleEscape);
  };
}

export function isPreviewOpen(): boolean {
  return isOpen;
}

export function cleanupPreview(): void {
  if (panelElement) {
    const cleanup = (panelElement as any)._cleanup;
    if (cleanup) cleanup();
    panelElement.remove();
    panelElement = null;
  }
  isOpen = false;
  state.document = null;
  state.error = null;
  state.highlightLine = null;
}
