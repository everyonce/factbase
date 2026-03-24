/**
 * Component unit tests.
 * Tests for Loading, Error, and Toast components.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { screen } from '@testing-library/dom';
import {
  renderSpinner,
  renderSkeletonCard,
  renderSkeletonList,
  renderSkeletonDocumentGroup,
} from './components/Loading';
import {
  renderError,
  renderInlineError,
  renderErrorBanner,
  setupRetryHandler,
} from './components/Error';
import {
  initToasts,
  showToast,
  dismissToast,
  dismissAllToasts,
  toast,
} from './components/Toast';

describe('Loading components', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  describe('renderSpinner', () => {
    it('should render spinner with default message', () => {
      container.innerHTML = renderSpinner();
      expect(container.textContent).toContain('Loading...');
    });

    it('should render spinner with custom message', () => {
      container.innerHTML = renderSpinner('Fetching data...');
      expect(container.textContent).toContain('Fetching data...');
    });

    it('should have accessible role and aria-live', () => {
      container.innerHTML = renderSpinner();
      const status = container.querySelector('[role="status"]');
      expect(status).not.toBeNull();
      expect(status?.getAttribute('aria-live')).toBe('polite');
    });

    it('should have screen reader text', () => {
      container.innerHTML = renderSpinner('Loading items');
      const srOnly = container.querySelector('.sr-only');
      expect(srOnly?.textContent).toBe('Loading items');
    });
  });

  describe('renderSkeletonCard', () => {
    it('should render skeleton card with aria-hidden', () => {
      container.innerHTML = renderSkeletonCard();
      const card = container.querySelector('[aria-hidden="true"]');
      expect(card).not.toBeNull();
    });

    it('should have animate-pulse class', () => {
      container.innerHTML = renderSkeletonCard();
      const card = container.querySelector('.animate-pulse');
      expect(card).not.toBeNull();
    });
  });

  describe('renderSkeletonList', () => {
    it('should render default 3 skeleton cards', () => {
      container.innerHTML = renderSkeletonList();
      const cards = container.querySelectorAll('.animate-pulse');
      // List wrapper + 3 cards
      expect(cards.length).toBeGreaterThanOrEqual(3);
    });

    it('should render specified number of cards', () => {
      container.innerHTML = renderSkeletonList(5);
      const cards = container.querySelectorAll('[aria-hidden="true"]');
      expect(cards.length).toBe(5);
    });

    it('should have accessible label', () => {
      container.innerHTML = renderSkeletonList();
      const list = container.querySelector('[aria-label="Loading content"]');
      expect(list).not.toBeNull();
    });
  });

  describe('renderSkeletonDocumentGroup', () => {
    it('should render document group skeleton', () => {
      container.innerHTML = renderSkeletonDocumentGroup();
      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).not.toBeNull();
    });
  });
});

describe('Error components', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  describe('renderError', () => {
    it('should render error with default title', () => {
      container.innerHTML = renderError({ message: 'Something went wrong' });
      expect(container.textContent).toContain('Error');
      expect(container.textContent).toContain('Something went wrong');
    });

    it('should render error with custom title', () => {
      container.innerHTML = renderError({ title: 'Network Error', message: 'Failed to connect' });
      expect(container.textContent).toContain('Network Error');
      expect(container.textContent).toContain('Failed to connect');
    });

    it('should render retry button when onRetry provided', () => {
      container.innerHTML = renderError({
        message: 'Failed',
        onRetry: () => {},
      });
      const button = container.querySelector('button');
      expect(button).not.toBeNull();
      expect(button?.textContent).toContain('Retry');
    });

    it('should not render retry button when onRetry not provided', () => {
      container.innerHTML = renderError({ message: 'Failed' });
      const button = container.querySelector('button');
      expect(button).toBeNull();
    });

    it('should render custom retry label', () => {
      container.innerHTML = renderError({
        message: 'Failed',
        onRetry: () => {},
        retryLabel: 'Try Again',
      });
      const button = container.querySelector('button');
      expect(button?.textContent).toContain('Try Again');
    });

    it('should escape HTML in message', () => {
      container.innerHTML = renderError({ message: '<script>alert("xss")</script>' });
      expect(container.innerHTML).not.toContain('<script>');
      expect(container.textContent).toContain('<script>');
    });
  });

  describe('renderInlineError', () => {
    it('should render inline error message', () => {
      container.innerHTML = renderInlineError('Invalid input');
      expect(container.textContent).toContain('Invalid input');
    });

    it('should have error styling', () => {
      container.innerHTML = renderInlineError('Error');
      const errorDiv = container.querySelector('.text-red-600');
      expect(errorDiv).not.toBeNull();
    });
  });

  describe('renderErrorBanner', () => {
    it('should render error banner', () => {
      container.innerHTML = renderErrorBanner('Operation failed');
      expect(container.textContent).toContain('Operation failed');
    });

    it('should render dismiss button when dismissId provided', () => {
      container.innerHTML = renderErrorBanner('Error', 'dismiss-btn');
      const button = container.querySelector('#dismiss-btn');
      expect(button).not.toBeNull();
    });

    it('should not render dismiss button when dismissId not provided', () => {
      container.innerHTML = renderErrorBanner('Error');
      const buttons = container.querySelectorAll('button');
      expect(buttons.length).toBe(0);
    });
  });

  describe('setupRetryHandler', () => {
    it('should attach click handler to retry button', () => {
      const onRetry = vi.fn();
      container.innerHTML = renderError({ message: 'Failed', onRetry });
      setupRetryHandler(onRetry);

      const button = container.querySelector('button');
      button?.click();

      expect(onRetry).toHaveBeenCalledTimes(1);
    });
  });
});

describe('Toast components', () => {
  beforeEach(() => {
    // Clean up any existing toast container
    const existing = document.getElementById('toast-container');
    if (existing) existing.remove();
    vi.useFakeTimers();
  });

  afterEach(() => {
    dismissAllToasts();
    const container = document.getElementById('toast-container');
    if (container) container.remove();
    vi.useRealTimers();
  });

  describe('initToasts', () => {
    it('should create toast container', () => {
      initToasts();
      const container = document.getElementById('toast-container');
      expect(container).not.toBeNull();
    });

    it('should not create duplicate containers', () => {
      initToasts();
      initToasts();
      const containers = document.querySelectorAll('#toast-container');
      expect(containers.length).toBe(1);
    });

    it('should have accessible attributes', () => {
      initToasts();
      const container = document.getElementById('toast-container');
      expect(container?.getAttribute('role')).toBe('region');
      expect(container?.getAttribute('aria-label')).toBe('Notifications');
    });
  });

  describe('showToast', () => {
    it('should show toast with message', () => {
      showToast({ message: 'Test message' });
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Test message');
    });

    it('should return toast ID', () => {
      const id = showToast({ message: 'Test' });
      expect(id).toMatch(/^toast-\d+-[a-z0-9]+$/);
    });

    it('should auto-dismiss after duration', () => {
      showToast({ message: 'Test', duration: 1000 });
      expect(document.getElementById('toast-container')?.textContent).toContain('Test');

      vi.advanceTimersByTime(1000);
      expect(document.getElementById('toast-container')?.textContent).not.toContain('Test');
    });

    it('should not auto-dismiss when duration is 0', () => {
      showToast({ message: 'Persistent', duration: 0 });
      vi.advanceTimersByTime(10000);
      expect(document.getElementById('toast-container')?.textContent).toContain('Persistent');
    });
  });

  describe('dismissToast', () => {
    it('should remove toast by ID', () => {
      const id = showToast({ message: 'To dismiss', duration: 0 });
      expect(document.getElementById('toast-container')?.textContent).toContain('To dismiss');

      dismissToast(id);
      expect(document.getElementById('toast-container')?.textContent).not.toContain('To dismiss');
    });

    it('should handle non-existent ID gracefully', () => {
      expect(() => dismissToast('non-existent')).not.toThrow();
    });
  });

  describe('dismissAllToasts', () => {
    it('should remove all toasts', () => {
      showToast({ message: 'Toast 1', duration: 0 });
      showToast({ message: 'Toast 2', duration: 0 });
      showToast({ message: 'Toast 3', duration: 0 });

      const container = document.getElementById('toast-container');
      expect(container?.children.length).toBe(3);

      dismissAllToasts();
      expect(container?.children.length).toBe(0);
    });
  });

  describe('toast convenience methods', () => {
    it('should show success toast', () => {
      toast.success('Success!');
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Success!');
      expect(container?.innerHTML).toContain('bg-green-50');
    });

    it('should show error toast with persistent duration', () => {
      toast.error('Error!');
      vi.advanceTimersByTime(10000);
      // Error toasts should persist (duration: 0)
      expect(document.getElementById('toast-container')?.textContent).toContain('Error!');
    });

    it('should show info toast', () => {
      toast.info('Info message');
      const container = document.getElementById('toast-container');
      expect(container?.innerHTML).toContain('bg-blue-50');
    });

    it('should show warning toast', () => {
      toast.warning('Warning!');
      const container = document.getElementById('toast-container');
      expect(container?.innerHTML).toContain('bg-amber-50');
    });
  });

  describe('toast with action', () => {
    it('should render action button', () => {
      showToast({
        message: 'With action',
        action: { label: 'Undo', onClick: () => {} },
        duration: 0,
      });
      const container = document.getElementById('toast-container');
      expect(container?.textContent).toContain('Undo');
    });

    it('should call action onClick and dismiss', () => {
      const onClick = vi.fn();
      const id = showToast({
        message: 'With action',
        action: { label: 'Undo', onClick },
        duration: 0,
      });

      const actionBtn = document.getElementById(`${id}-action`);
      actionBtn?.click();

      expect(onClick).toHaveBeenCalledTimes(1);
      // Toast should be dismissed after action
      expect(document.getElementById('toast-container')?.textContent).not.toContain('With action');
    });
  });
});

import { renderAnswerForm, setupAnswerFormHandlers, clearFormStates } from './components/AnswerForm';

describe('AnswerForm structured inputs', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    clearFormStates();
  });

  afterEach(() => {
    document.body.removeChild(container);
    clearFormStates();
  });

  const TYPES_WITH_STRUCTURED = ['temporal', 'stale', 'conflict', 'missing', 'ambiguous', 'precision', 'weak-source'];
  const TYPES_WITHOUT = ['duplicate', 'corruption'];

  for (const type of TYPES_WITH_STRUCTURED) {
    it(`renders structured inputs for ${type}`, () => {
      container.innerHTML = renderAnswerForm('doc1', 0, type);
      expect(container.querySelector('.structured-inputs')).not.toBeNull();
      expect(container.querySelector('.freeform-fallback')).not.toBeNull();
      expect(container.querySelector('.toggle-freeform')).not.toBeNull();
    });
  }

  for (const type of TYPES_WITHOUT) {
    it(`renders freeform textarea for ${type}`, () => {
      container.innerHTML = renderAnswerForm('doc1', 0, type);
      expect(container.querySelector('.structured-inputs')).toBeNull();
      expect(container.querySelector('textarea')).not.toBeNull();
    });
  }

  it('temporal: renders start/end year inputs and unknown checkbox', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'temporal');
    expect(container.querySelector('input[name="t-start"]')).not.toBeNull();
    expect(container.querySelector('input[name="t-end"]')).not.toBeNull();
    expect(container.querySelector('input[name="t-unknown"]')).not.toBeNull();
    expect(container.querySelector('.t-preview')).not.toBeNull();
  });

  it('stale: renders still-accurate button and source URL input', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'stale');
    expect(container.querySelector('[data-action="still-accurate"]')).not.toBeNull();
    expect(container.querySelector('input[name="s-source"]')).not.toBeNull();
  });

  it('conflict: renders radio buttons for resolution options', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'conflict');
    const radios = container.querySelectorAll('input[name="c-resolution"]');
    expect(radios.length).toBeGreaterThanOrEqual(2);
  });

  it('missing: renders URL, title, and date inputs', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'missing');
    expect(container.querySelector('input[name="m-url"]')).not.toBeNull();
    expect(container.querySelector('input[name="m-title"]')).not.toBeNull();
    expect(container.querySelector('input[name="m-date"]')).not.toBeNull();
  });

  it('ambiguous: renders definition input', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'ambiguous');
    expect(container.querySelector('input[name="a-definition"]')).not.toBeNull();
  });

  it('precision: renders two-path radio selector', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'precision');
    const radios = container.querySelectorAll('input[name="p-path"]');
    expect(radios.length).toBe(2);
    expect(container.querySelector('input[name="p-value"]')).not.toBeNull();
    expect(container.querySelector('input[name="p-reason"]')).not.toBeNull();
  });

  it('weak-source: renders URL input and cannot-improve checkbox', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'weak-source');
    expect(container.querySelector('input[name="w-url"]')).not.toBeNull();
    expect(container.querySelector('input[name="w-cannot"]')).not.toBeNull();
  });

  it('toggle freeform shows/hides structured inputs', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'temporal');
    setupAnswerFormHandlers(container, { onSuccess: () => {}, onError: () => {} });

    const toggleBtn = container.querySelector('.toggle-freeform') as HTMLButtonElement;
    const structured = container.querySelector('.structured-inputs') as HTMLElement;
    const freeform = container.querySelector('.freeform-fallback') as HTMLElement;

    // Initially structured visible, freeform hidden
    expect(freeform.classList.contains('hidden')).toBe(true);
    expect(structured.classList.contains('hidden')).toBe(false);

    // Click toggle → show freeform
    toggleBtn.click();
    expect(freeform.classList.contains('hidden')).toBe(false);
    expect(structured.classList.contains('hidden')).toBe(true);
    expect(toggleBtn.textContent?.trim()).toBe('Structured');

    // Click again → back to structured
    toggleBtn.click();
    expect(freeform.classList.contains('hidden')).toBe(true);
    expect(structured.classList.contains('hidden')).toBe(false);
    expect(toggleBtn.textContent?.trim()).toBe('Freeform');
  });

  it('temporal: live preview updates on input', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'temporal');
    setupAnswerFormHandlers(container, { onSuccess: () => {}, onError: () => {} });

    const startInput = container.querySelector('input[name="t-start"]') as HTMLInputElement;
    const endInput = container.querySelector('input[name="t-end"]') as HTMLInputElement;
    const preview = container.querySelector('.t-preview') as HTMLElement;

    startInput.value = '2022';
    startInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(preview.textContent).toBe('@t[=2022]');

    endInput.value = '2024';
    endInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(preview.textContent).toBe('@t[2022..2024]');

    const unknownCb = container.querySelector('input[name="t-unknown"]') as HTMLInputElement;
    unknownCb.checked = true;
    unknownCb.dispatchEvent(new Event('input', { bubbles: true }));
    expect(preview.textContent).toBe('@t[?]');
  });

  it('renders dismiss and delete fact buttons', () => {
    container.innerHTML = renderAnswerForm('doc1', 0, 'temporal');
    const dismissBtn = container.querySelector('[data-action="dismiss"]');
    const deleteBtn = container.querySelector('[data-action="delete"]');
    expect(dismissBtn).not.toBeNull();
    expect(deleteBtn).not.toBeNull();
  });
});

import {
  classifyQuestions,
  renderTriageView,
  renderSessionSummary,
} from './components/TriageView';
import type { DocumentReview } from './api';

function makeDoc(id: string, questions: Partial<import('./api').ReviewQuestion>[]): DocumentReview {
  return {
    doc_id: id,
    doc_title: `Doc ${id}`,
    file_path: `/kb/${id}.md`,
    questions: questions.map((q, i) => ({
      question_type: q.question_type ?? 'temporal',
      description: q.description ?? `Question ${i}`,
      answered: q.answered ?? false,
      answer: q.answer,
      confidence: q.confidence,
      confidence_reason: q.confidence_reason,
      line_ref: q.line_ref,
    })),
  };
}

describe('TriageView', () => {
  describe('classifyQuestions', () => {
    it('puts high-confidence answered questions into quickApprovals', () => {
      const docs = [makeDoc('a', [{ confidence: 'high', answer: 'yes', answered: false }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickApprovals).toHaveLength(1);
      expect(buckets.quickAnswers).toHaveLength(0);
      expect(buckets.researchNeeded).toHaveLength(0);
    });

    it('puts deferred questions into researchNeeded', () => {
      const docs = [makeDoc('a', [{ confidence: 'deferred', answered: false }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.researchNeeded).toHaveLength(1);
      expect(buckets.quickApprovals).toHaveLength(0);
    });

    it('puts low-confidence questions into quickAnswers', () => {
      const docs = [makeDoc('a', [{ confidence: 'low', answered: false }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickAnswers).toHaveLength(1);
    });

    it('puts questions with no confidence into quickAnswers', () => {
      const docs = [makeDoc('a', [{ answered: false }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickAnswers).toHaveLength(1);
    });

    it('skips already-answered questions', () => {
      const docs = [makeDoc('a', [{ answered: true, confidence: 'high', answer: 'yes' }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickApprovals).toHaveLength(0);
      expect(buckets.quickAnswers).toHaveLength(0);
      expect(buckets.researchNeeded).toHaveLength(0);
    });

    it('classifies mixed questions correctly', () => {
      const docs = [makeDoc('a', [
        { confidence: 'high', answer: 'yes', answered: false },
        { confidence: 'deferred', answered: false },
        { answered: false },
      ])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickApprovals).toHaveLength(1);
      expect(buckets.researchNeeded).toHaveLength(1);
      expect(buckets.quickAnswers).toHaveLength(1);
    });

    it('preserves doc context on each item', () => {
      const docs = [makeDoc('doc42', [{ confidence: 'high', answer: 'x', answered: false }])];
      const buckets = classifyQuestions(docs);
      expect(buckets.quickApprovals[0].docId).toBe('doc42');
      expect(buckets.quickApprovals[0].docTitle).toBe('Doc doc42');
      expect(buckets.quickApprovals[0].questionIndex).toBe(0);
    });
  });

  describe('renderTriageView', () => {
    let container: HTMLElement;

    beforeEach(() => {
      container = document.createElement('div');
      document.body.appendChild(container);
    });

    afterEach(() => {
      document.body.removeChild(container);
    });

    it('renders empty state when no questions', () => {
      const buckets = { quickApprovals: [], quickAnswers: [], researchNeeded: [] };
      container.innerHTML = renderTriageView(buckets, 0, null);
      expect(container.textContent).toContain('No pending review questions');
    });

    it('renders session summary when all done and stats provided', () => {
      const buckets = { quickApprovals: [], quickAnswers: [], researchNeeded: [] };
      const stats = { approved: 3, answered: 2, skipped: 1, startCount: 6 };
      container.innerHTML = renderTriageView(buckets, 0, stats);
      expect(container.textContent).toContain('Session complete');
      expect(container.textContent).toContain('3');
      expect(container.textContent).toContain('2');
    });

    it('renders quick approvals section when items present', () => {
      const docs = [makeDoc('a', [{ confidence: 'high', answer: 'yes', answered: false }])];
      const buckets = classifyQuestions(docs);
      container.innerHTML = renderTriageView(buckets, 0, null);
      expect(container.textContent).toContain('Quick approvals');
      expect(container.querySelector('.triage-approve-btn')).not.toBeNull();
      expect(container.querySelector('.triage-reject-btn')).not.toBeNull();
    });

    it('renders quick answers section when items present', () => {
      const docs = [makeDoc('a', [{ answered: false }])];
      const buckets = classifyQuestions(docs);
      container.innerHTML = renderTriageView(buckets, 0, null);
      expect(container.textContent).toContain('Quick answers');
    });

    it('renders research needed section when items present', () => {
      const docs = [makeDoc('a', [{ confidence: 'deferred', answered: false }])];
      const buckets = classifyQuestions(docs);
      container.innerHTML = renderTriageView(buckets, 0, null);
      expect(container.textContent).toContain('Research needed');
    });

    it('shows agent suggestion in approval card', () => {
      const docs = [makeDoc('a', [{ confidence: 'high', answer: 'The answer is 42', answered: false }])];
      const buckets = classifyQuestions(docs);
      container.innerHTML = renderTriageView(buckets, 0, null);
      expect(container.textContent).toContain('The answer is 42');
    });

    it('shows only current approval card (others hidden)', () => {
      const docs = [makeDoc('a', [
        { confidence: 'high', answer: 'a1', answered: false },
        { confidence: 'high', answer: 'a2', answered: false },
      ])];
      const buckets = classifyQuestions(docs);
      container.innerHTML = renderTriageView(buckets, 0, null);
      const cards = container.querySelectorAll('.triage-approval-card');
      expect(cards[0].classList.contains('block')).toBe(true);
      expect(cards[1].classList.contains('hidden')).toBe(true);
    });
  });

  describe('renderSessionSummary', () => {
    it('shows correct counts', () => {
      const container = document.createElement('div');
      container.innerHTML = renderSessionSummary({ approved: 5, answered: 3, skipped: 2, startCount: 10 });
      expect(container.textContent).toContain('5');
      expect(container.textContent).toContain('3');
      expect(container.textContent).toContain('2');
      expect(container.textContent).toContain('Session complete');
    });
  });
});
