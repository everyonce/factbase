/**
 * DocumentPreview component.
 * Side panel showing document content with line highlighting and links.
 */

import { api, Document, DocumentLink, DocumentPreviewResponse, PreviewLine, ApiRequestError } from '../api';

interface PreviewState {
  loading: boolean;
  error: string | null;
  document: Document | null;
  preview: DocumentPreviewResponse | null;
  highlightLine: number | null;
  agentReasoning: string | null;
}

const state: PreviewState = {
  loading: false,
  error: null,
  document: null,
  preview: null,
  highlightLine: null,
  agentReasoning: null,
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

function renderPreviewLines(lines: PreviewLine[], totalLines: number, startLine: number, endLine: number): string {
  const isWindowed = endLine < totalLines || startLine > 1;
  const header = isWindowed
    ? `<div class="px-4 py-1 text-xs text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">Lines ${startLine}–${endLine} of ${totalLines}</div>`
    : '';
  const rows = lines.map(({ line, content, highlighted }) => {
    const highlightClass = highlighted
      ? 'bg-yellow-100 dark:bg-yellow-900/50 border-l-4 border-yellow-400'
      : '';
    const lineNumClass = highlighted
      ? 'text-yellow-600 dark:text-yellow-400 font-bold'
      : 'text-gray-400 dark:text-gray-600';
    return `
      <div class="flex ${highlightClass}" ${highlighted ? 'id="highlighted-line"' : ''}>
        <span class="select-none w-10 flex-shrink-0 text-right pr-3 ${lineNumClass} text-xs leading-6">${line}</span>
        <pre class="flex-1 text-sm leading-6 whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">${escapeHtml(content) || ' '}</pre>
      </div>
    `;
  }).join('');
  return header + rows;
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

  if (!state.document && !state.preview) {
    return `<div class="p-4 text-gray-500 dark:text-gray-400">No document selected</div>`;
  }

  // Use preview data if available (windowed), otherwise fall back to full document
  const title = state.preview?.doc_title ?? state.document?.title ?? '';
  const filePath = state.preview?.file_path ?? state.document?.file_path ?? '';
  const docId = state.preview?.doc_id ?? state.document?.id ?? '';
  const docType = state.document?.doc_type ?? '';

  const reasoningBanner = state.agentReasoning
    ? `<div class="flex-shrink-0 px-4 py-2 bg-amber-50 dark:bg-amber-900/20 border-b border-amber-200 dark:border-amber-800 text-xs text-amber-700 dark:text-amber-300">
        <span class="font-medium">Agent flagged this because:</span> ${escapeHtml(state.agentReasoning)}
      </div>`
    : '';

  const contentHtml = state.preview
    ? renderPreviewLines(state.preview.lines, state.preview.total_lines, state.preview.start_line, state.preview.end_line)
    : state.document?.content
      ? renderContent(state.document.content, state.highlightLine)
      : '<p class="text-gray-500 dark:text-gray-400">No content available</p>';

  return `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-start justify-between">
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white truncate" id="preview-title">${escapeHtml(title)}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 truncate">${escapeHtml(filePath)}</p>
            ${docType ? `<div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300">
                ${escapeHtml(docType)}
              </span>
              <span class="text-xs text-gray-500 dark:text-gray-400">${escapeHtml(docId)}</span>
            </div>` : ''}
          </div>
          <button id="preview-close-btn" class="ml-2 p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200" aria-label="Close preview">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      ${reasoningBanner}

      ${state.document ? `<!-- Links section -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="grid grid-cols-2 gap-4">
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="links-to-heading">Links to</h4>
            <nav aria-labelledby="links-to-heading">
              ${renderLinks(state.document.links_to || [])}
            </nav>
          </div>
          <div>
            <h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2" id="linked-from-heading">Linked from</h4>
            <nav aria-labelledby="linked-from-heading">
              ${renderLinks(state.document.linked_from || [])}
            </nav>
          </div>
        </div>
      </div>` : ''}

      <!-- Content -->
      <div class="flex-1 overflow-auto font-mono bg-white dark:bg-gray-900" role="region" aria-label="Document content">
        ${contentHtml}
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

async function fetchDocument(docId: string, highlightLine?: number): Promise<void> {
  state.loading = true;
  state.error = null;
  state.document = null;
  state.preview = null;
  updatePanel();

  try {
    if (highlightLine !== undefined) {
      // Use focused preview endpoint for line-specific context
      state.preview = await api.getDocumentPreview(docId, highlightLine);
    } else {
      // Full document view with links
      const doc = await api.getDocument(docId);
      const links = await api.getDocumentLinks(docId);
      state.document = {
        ...doc,
        links_to: links.links_to,
        linked_from: links.linked_from,
      };
    }
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

export function openPreview(docId: string, highlightLine?: number, agentReasoning?: string): void {
  state.highlightLine = highlightLine ?? null;
  state.agentReasoning = agentReasoning ?? null;

  if (!isOpen) {
    // Create panel if not exists
    createPanel();
    isOpen = true;
  }

  fetchDocument(docId, highlightLine);
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
  state.preview = null;
  state.error = null;
  state.highlightLine = null;
  state.agentReasoning = null;
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
  state.preview = null;
  state.error = null;
  state.highlightLine = null;
  state.agentReasoning = null;
}
