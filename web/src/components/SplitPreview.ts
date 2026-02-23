/**
 * SplitPreview component.
 * Shows document sections and fact distribution for split candidates.
 */

import { api, Document, ApiRequestError } from '../api';

interface SplitSection {
  title: string;
  level: number;
  startLine: number;
  endLine: number;
  content: string;
  factCount: number;
}

interface SplitPreviewState {
  loading: boolean;
  error: string | null;
  doc: Document | null;
  sections: SplitSection[];
}

const state: SplitPreviewState = {
  loading: false,
  error: null,
  doc: null,
  sections: [],
};

let isOpen = false;
let panelElement: HTMLElement | null = null;
let currentDocId: string | null = null;

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Count facts in content (list items).
 */
function countFacts(content: string): number {
  const lines = content.split('\n');
  let count = 0;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('- ') || trimmed.startsWith('* ') || /^\d+\.\s/.test(trimmed)) {
      count++;
    }
  }
  return count;
}

/**
 * Extract sections from markdown content based on headers.
 * Client-side implementation matching the Rust extract_sections function.
 */
function extractSections(content: string): SplitSection[] {
  const lines = content.split('\n');
  const sections: SplitSection[] = [];
  
  let currentTitle = 'Introduction';
  let currentLevel = 0;
  let currentStart = 1;
  let currentContent: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const lineNum = i + 1;
    const line = lines[i];

    // Skip factbase header
    if (line.startsWith('<!-- factbase:')) {
      continue;
    }

    // Check for header
    const header = parseHeader(line);
    if (header) {
      // Save previous section if it has content
      if (currentContent.length > 0) {
        const contentStr = currentContent.join('\n').trim();
        if (contentStr) {
          sections.push({
            title: currentTitle,
            level: currentLevel,
            startLine: currentStart,
            endLine: lineNum - 1,
            content: contentStr,
            factCount: countFacts(contentStr),
          });
        }
      }

      // Start new section
      currentTitle = header.title;
      currentLevel = header.level;
      currentStart = lineNum;
      currentContent = [];
    } else {
      currentContent.push(line);
    }
  }

  // Save final section
  if (currentContent.length > 0) {
    const contentStr = currentContent.join('\n').trim();
    if (contentStr) {
      sections.push({
        title: currentTitle,
        level: currentLevel,
        startLine: currentStart,
        endLine: lines.length,
        content: contentStr,
        factCount: countFacts(contentStr),
      });
    }
  }

  return sections;
}

/**
 * Parse a markdown header line.
 */
function parseHeader(line: string): { level: number; title: string } | null {
  const trimmed = line.trimStart();
  if (!trimmed.startsWith('#')) {
    return null;
  }

  let level = 0;
  for (const char of trimmed) {
    if (char === '#') {
      level++;
    } else {
      break;
    }
  }

  if (level === 0 || level > 6) {
    return null;
  }

  const title = trimmed.slice(level).trim();
  if (!title) {
    return null;
  }

  return { level, title };
}

function renderSectionCard(section: SplitSection, index: number): string {
  const levelBadge = section.level > 0 
    ? `<span class="text-xs text-gray-400 dark:text-gray-500">H${section.level}</span>`
    : `<span class="text-xs text-gray-400 dark:text-gray-500">Intro</span>`;

  const lines = section.content.split('\n').slice(0, 5);
  const hasMore = section.content.split('\n').length > 5;

  return `
    <div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <!-- Section Header -->
      <div class="p-3 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <span class="text-xs font-medium text-gray-500 dark:text-gray-400">Section ${index + 1}</span>
            ${levelBadge}
          </div>
          <div class="flex items-center space-x-2">
            <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
              ${section.factCount} fact${section.factCount !== 1 ? 's' : ''}
            </span>
            <span class="text-xs text-gray-400 dark:text-gray-500">
              Lines ${section.startLine}-${section.endLine}
            </span>
          </div>
        </div>
        <h4 class="mt-1 text-sm font-semibold text-gray-900 dark:text-white">
          ${escapeHtml(section.title)}
        </h4>
      </div>
      <!-- Section Preview -->
      <div class="p-3 font-mono text-xs text-gray-600 dark:text-gray-400 max-h-32 overflow-hidden">
        ${lines.map(line => `<div class="truncate">${escapeHtml(line) || '&nbsp;'}</div>`).join('')}
        ${hasMore ? '<div class="text-gray-400 dark:text-gray-500 mt-1">...</div>' : ''}
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
          <p class="mt-2 text-gray-500 dark:text-gray-400">Loading document...</p>
        </div>
      </div>
    `;
  }

  if (state.error) {
    return `
      <div class="p-4 text-center">
        <p class="text-red-600 dark:text-red-400">${escapeHtml(state.error)}</p>
        <button id="split-preview-close-error" class="mt-2 text-sm text-blue-600 dark:text-blue-400 hover:underline">
          Close
        </button>
      </div>
    `;
  }

  if (!state.doc) {
    return `
      <div class="p-4 text-center text-gray-500 dark:text-gray-400">
        No document loaded
      </div>
    `;
  }

  const totalFacts = state.sections.reduce((sum, s) => sum + s.factCount, 0);
  const validSections = state.sections.filter(s => s.content.length >= 50);

  return `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex-shrink-0 p-4 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">Split Preview</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400">Review sections before splitting</p>
          </div>
          <button id="split-preview-close-btn" class="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
            </svg>
          </button>
        </div>
      </div>

      <!-- Document Info -->
      <div class="flex-shrink-0 p-4 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700">
        <div class="flex items-center justify-between">
          <div>
            <h4 class="text-sm font-semibold text-gray-900 dark:text-white">${escapeHtml(state.doc.title)}</h4>
            <div class="mt-1 flex items-center space-x-2">
              <span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                ${escapeHtml(state.doc.doc_type)}
              </span>
              <span class="text-xs text-gray-400 dark:text-gray-500">[${escapeHtml(state.doc.id)}]</span>
            </div>
          </div>
          <div class="text-right text-sm">
            <div class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${state.sections.length}</span> sections
            </div>
            <div class="text-gray-600 dark:text-gray-300">
              <span class="font-medium">${totalFacts}</span> total facts
            </div>
          </div>
        </div>
        ${validSections.length < 2 ? `
          <div class="mt-3 p-2 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded text-xs text-amber-700 dark:text-amber-300">
            <strong>Note:</strong> This document has fewer than 2 sections with sufficient content (50+ chars). 
            Split may not be recommended.
          </div>
        ` : ''}
      </div>

      <!-- Sections List -->
      <div class="flex-1 overflow-auto p-4">
        <div class="space-y-3">
          ${state.sections.map((section, i) => renderSectionCard(section, i)).join('')}
        </div>
        ${state.sections.length === 0 ? `
          <div class="text-center text-gray-500 dark:text-gray-400 py-8">
            No sections found in document
          </div>
        ` : ''}
      </div>

      <!-- Actions -->
      <div class="flex-shrink-0 p-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Split requires CLI: <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">factbase organize split ${currentDocId || 'doc_id'}</code>
          </p>
          <div class="flex items-center space-x-2">
            <button
              id="split-preview-dismiss-btn"
              class="px-3 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-md"
            >
              Close
            </button>
            <button
              id="split-preview-copy-btn"
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
  const contentEl = panelElement.querySelector('#split-preview-content');
  if (contentEl) {
    contentEl.innerHTML = renderPanel();
    setupPanelHandlers();
  }
}

function setupPanelHandlers(): void {
  document.getElementById('split-preview-close-btn')?.addEventListener('click', closeSplitPreview);
  document.getElementById('split-preview-close-error')?.addEventListener('click', closeSplitPreview);
  document.getElementById('split-preview-dismiss-btn')?.addEventListener('click', closeSplitPreview);

  document.getElementById('split-preview-copy-btn')?.addEventListener('click', () => {
    const command = `factbase organize split ${currentDocId}`;
    navigator.clipboard.writeText(command).then(() => {
      const btn = document.getElementById('split-preview-copy-btn');
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

async function fetchDocument(docId: string): Promise<void> {
  state.loading = true;
  state.error = null;
  state.doc = null;
  state.sections = [];
  updatePanel();

  try {
    const doc = await api.getDocument(docId);
    state.doc = doc;
    
    // Extract sections from content
    if (doc.content) {
      state.sections = extractSections(doc.content);
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

export function openSplitPreview(docId: string): void {
  currentDocId = docId;

  if (!isOpen) {
    createPanel();
    isOpen = true;
  }

  fetchDocument(docId);
}

export function closeSplitPreview(): void {
  if (panelElement) {
    panelElement.classList.add('translate-x-full');
    setTimeout(() => {
      panelElement?.remove();
      panelElement = null;
    }, 300);
  }
  isOpen = false;
  state.doc = null;
  state.sections = [];
  state.error = null;
  currentDocId = null;
}

function createPanel(): void {
  panelElement?.remove();

  panelElement = document.createElement('div');
  panelElement.id = 'split-preview-panel';
  panelElement.className = `
    fixed top-0 right-0 h-full w-full sm:w-[480px] lg:w-[560px]
    bg-white dark:bg-gray-800 shadow-xl
    transform translate-x-full transition-transform duration-300 ease-in-out
    z-50 flex flex-col
  `.trim().replace(/\s+/g, ' ');

  panelElement.innerHTML = `
    <div id="split-preview-content" class="h-full flex flex-col">
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
      closeSplitPreview();
    }
  };
  document.addEventListener('keydown', handleEscape);

  (panelElement as any)._cleanup = () => {
    document.removeEventListener('keydown', handleEscape);
  };
}

export function isSplitPreviewOpen(): boolean {
  return isOpen;
}

export function cleanupSplitPreview(): void {
  if (panelElement) {
    const cleanup = (panelElement as any)._cleanup;
    if (cleanup) cleanup();
    panelElement.remove();
    panelElement = null;
  }
  isOpen = false;
  state.doc = null;
  state.sections = [];
  state.error = null;
  currentDocId = null;
}
