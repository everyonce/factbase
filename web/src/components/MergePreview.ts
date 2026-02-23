/**
 * MergePreview component.
 * Side-by-side document comparison for merge candidates.
 */

import { api, Document, ApiRequestError } from '../api';

interface MergePreviewState {
  loading: boolean;
  error: string | null;
  doc1: Document | null;
  doc2: Document | null;
}

const state: MergePreviewState = {
  loading: false,
  error: null,
  doc1: null,
  doc2: null,
};

let isOpen = false;
let panelElement: HTMLElement | null = null;
let currentDoc1Id: string | null = null;
let currentDoc2Id: string | null = null;

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function countFacts(content: string): number {
  // Count list items and non-empty paragraphs as facts
  const lines = content.split('\n');
  let count = 0;
  for (const line of lines) {
    const trimmed = line.trim();
    // List items
    if (trimmed.startsWith('- ') || trimmed.startsWith('* ') || /^\d+\.\s/.test(trimmed)) {
      count++;
    }
  }
  return count;
}

function renderDocumentColumn(doc: Document | null, label: string): string {
  if (!doc) {
    return `
      <div class="flex-1 min-w-0 p-4 bg-gray-50 dark:bg-gray-800/50 rounded-lg">
        <div class="text-center text-gray-500 dark:text-gray-400">
          Loading...
        </div>
      </div>
    `;
  }

  const factCount = doc.content ? countFacts(doc.content) : 0;
  const lines = doc.content?.split('\n') || [];

  return `
    <div class="flex-1 min-w-0 flex flex-col bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <!-- Header -->
      <div class="flex-shrink-0 p-3 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">${label}</span>
          <span class="text-xs text-gray-500 dark:text-gray-400">${factCount} fact${factCount !== 1 ? 's' : ''}</span>
        </div>
        <h4 class="mt-1 text-sm font-semibold text-gray-900 dark:text-white truncate" title="${escapeHtml(doc.title)}">
          ${escapeHtml(doc.title)}
        </h4>
        <div class="mt-1 flex items-center space-x-2">
          <span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
            ${escapeHtml(doc.doc_type)}
          </span>
          <span class="text-xs text-gray-400 dark:text-gray-500">[${escapeHtml(doc.id)}]</span>
        </div>
        <p class="mt-1 text-xs text-gray-500 dark:text-gray-400 truncate" title="${escapeHtml(doc.file_path)}">
          ${escapeHtml(doc.file_path)}
        </p>
      </div>
      <!-- Content -->
      <div class="flex-1 overflow-auto p-3 font-mono text-xs">
        ${lines.map((line, i) => `
          <div class="flex hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <span class="select-none w-8 flex-shrink-0 text-right pr-2 text-gray-400 dark:text-gray-600">${i + 1}</span>
            <pre class="flex-1 whitespace-pre-wrap break-words text-gray-700 dark:text-gray-300">${escapeHtml(line) || ' '}</pre>
          </div>
        `).join('')}
      </div>
    </div>
  `;
}

function renderPanel(): string {
  if (state.loading) {
    return `
      <div class="flex items-center justify-center h-64">
        <div class="text-center">
          <div class="inline-block animate-spin rounded-full h-8 w-8 border-4 border-gray-300 border-t-blue-600"></div>
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading documents...</p>
        </div>
      </div>
    `;
  }

  if (state.error) {
    return `
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${escapeHtml(state.error)}</p>
        <button id="merge-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;
  }

  const doc1Facts = state.doc1?.content ? countFacts(state.doc1.content) : 0;
  const doc2Facts = state.doc2?.content ? countFacts(state.doc2.content) : 0;
  const totalFacts = doc1Facts + doc2Facts;

  return `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">Merge Preview</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">Compare documents before merging</p>
          </div>
          <button id="merge-preview-close-btn" class="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Summary -->
      <div class="flex-shrink-0 p-4 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between text-sm">
          <div class="flex items-center space-x-4">
            <span class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${totalFacts}</span> total facts
            </span>
            <span class="text-gray-400 dark:text-gray-500">|</span>
            <span class="text-gray-600 dark:text-gray-300">
              Doc 1: <span class="font-medium">${doc1Facts}</span>
            </span>
            <span class="text-gray-600 dark:text-gray-300">
              Doc 2: <span class="font-medium">${doc2Facts}</span>
            </span>
          </div>
          <div class="text-xs text-gray-500 dark:text-gray-400">
            Merged document will contain facts from both
          </div>
        </div>
      </div>

      <!-- Side-by-side comparison -->
      <div class="flex-1 overflow-hidden p-4">
        <div class="flex gap-4 h-full">
          ${renderDocumentColumn(state.doc1, 'Document 1')}
          ${renderDocumentColumn(state.doc2, 'Document 2')}
        </div>
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Merge requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize merge ${currentDoc1Id || 'doc1'} ${currentDoc2Id || 'doc2'}</code>
          </p>
          <div class="flex items-center space-x-2">
            <button
              id="merge-preview-dismiss-btn"
              class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            >
              Dismiss
            </button>
            <button
              id="merge-preview-approve-btn"
              class="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            >
              Copy Command
            </button>
          </div>
        </div>
      </div>
    </div>
  `;
}

function updatePanel(): void {
  if (!panelElement) return;
  const contentEl = panelElement.querySelector('#merge-preview-content');
  if (contentEl) {
    contentEl.innerHTML = renderPanel();
    setupPanelHandlers();
  }
}

function setupPanelHandlers(): void {
  document.getElementById('merge-preview-close-btn')?.addEventListener('click', closeMergePreview);
  document.getElementById('merge-preview-close-error')?.addEventListener('click', closeMergePreview);

  document.getElementById('merge-preview-dismiss-btn')?.addEventListener('click', () => {
    closeMergePreview();
  });

  document.getElementById('merge-preview-approve-btn')?.addEventListener('click', () => {
    const command = `factbase organize merge ${currentDoc1Id} ${currentDoc2Id}`;
    navigator.clipboard.writeText(command).then(() => {
      const btn = document.getElementById('merge-preview-approve-btn');
      if (btn) {
        btn.textContent = 'Copied!';
        setTimeout(() => {
          btn.textContent = 'Copy Command';
        }, 2000);
      }
    }).catch(() => {
      alert(`Run: ${command}`);
    });
  });
}

async function fetchDocuments(doc1Id: string, doc2Id: string): Promise<void> {
  state.loading = true;
  state.error = null;
  state.doc1 = null;
  state.doc2 = null;
  updatePanel();

  try {
    const [doc1, doc2] = await Promise.all([
      api.getDocument(doc1Id),
      api.getDocument(doc2Id),
    ]);
    state.doc1 = doc1;
    state.doc2 = doc2;
  } catch (e) {
    if (e instanceof ApiRequestError) {
      state.error = e.message;
    } else {
      state.error = 'Failed to load documents';
    }
  } finally {
    state.loading = false;
    updatePanel();
  }
}

export function openMergePreview(doc1Id: string, doc2Id: string): void {
  currentDoc1Id = doc1Id;
  currentDoc2Id = doc2Id;

  if (!isOpen) {
    createPanel();
    isOpen = true;
  }

  fetchDocuments(doc1Id, doc2Id);
}

export function closeMergePreview(): void {
  if (panelElement) {
    panelElement.classList.add('translate-x-full');
    setTimeout(() => {
      panelElement?.remove();
      panelElement = null;
    }, 300);
  }
  isOpen = false;
  state.doc1 = null;
  state.doc2 = null;
  state.error = null;
  currentDoc1Id = null;
  currentDoc2Id = null;
}

function createPanel(): void {
  panelElement?.remove();

  panelElement = document.createElement('div');
  panelElement.id = 'merge-preview-panel';
  panelElement.className = `
    fixed top-0 right-0 h-full w-full lg:w-[900px] xl:w-[1100px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g, ' ');

  panelElement.innerHTML = `
    <div id="merge-preview-content" class="h-full flex flex-col">
      ${renderPanel()}
    </div>
  `;

  document.body.appendChild(panelElement);

  requestAnimationFrame(() => {
    panelElement?.classList.remove('translate-x-full');
  });

  setupPanelHandlers();

  const handleEscape = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && isOpen) {
      closeMergePreview();
    }
  };
  document.addEventListener('keydown', handleEscape);

  (panelElement as any)._cleanup = () => {
    document.removeEventListener('keydown', handleEscape);
  };
}

export function isMergePreviewOpen(): boolean {
  return isOpen;
}

export function cleanupMergePreview(): void {
  if (panelElement) {
    const cleanup = (panelElement as any)._cleanup;
    if (cleanup) cleanup();
    panelElement.remove();
    panelElement = null;
  }
  isOpen = false;
  state.doc1 = null;
  state.doc2 = null;
  state.error = null;
  currentDoc1Id = null;
  currentDoc2Id = null;
}
