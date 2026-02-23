/**
 * API client unit tests.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { api, ApiRequestError } from './api';

// Mock fetch globally
const mockFetch = vi.fn();
global.fetch = mockFetch;

describe('ApiClient', () => {
  beforeEach(() => {
    mockFetch.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('request handling', () => {
    it('should make GET request with correct headers', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ repos_count: 1, docs_count: 10, db_size_bytes: 1024 }),
      });

      await api.getStats();

      expect(mockFetch).toHaveBeenCalledWith('/api/stats', {
        headers: { 'Content-Type': 'application/json' },
      });
    });

    it('should make POST request with body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, doc_id: 'abc123', question_index: 0 }),
      });

      await api.answerQuestion('abc123', 0, 'test answer');

      expect(mockFetch).toHaveBeenCalledWith('/api/review/answer/abc123', {
        headers: { 'Content-Type': 'application/json' },
        method: 'POST',
        body: JSON.stringify({ question_index: 0, answer: 'test answer' }),
      });
    });

    it('should throw ApiRequestError on non-ok response', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        json: () => Promise.resolve({ error: 'Document not found', code: 'NOT_FOUND' }),
      });

      await expect(api.getDocument('nonexistent')).rejects.toThrow(ApiRequestError);
    });

    it('should handle JSON parse error in error response', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        json: () => Promise.reject(new Error('Invalid JSON')),
      });

      try {
        await api.getStats();
        expect.fail('Should have thrown');
      } catch (e) {
        expect(e).toBeInstanceOf(ApiRequestError);
        expect((e as ApiRequestError).message).toBe('HTTP 500: Internal Server Error');
      }
    });
  });

  describe('query parameter building', () => {
    it('should build query params for getReviewQueue', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ documents: [], total: 0, answered: 0, unanswered: 0 }),
      });

      await api.getReviewQueue({ repo: 'main', type: 'temporal' });

      expect(mockFetch).toHaveBeenCalledWith(
        '/api/review/queue?repo=main&type=temporal',
        expect.any(Object)
      );
    });

    it('should omit empty query params', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ documents: [], total: 0, answered: 0, unanswered: 0 }),
      });

      await api.getReviewQueue({});

      expect(mockFetch).toHaveBeenCalledWith('/api/review/queue', expect.any(Object));
    });

    it('should build query params for getSuggestions with threshold', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ merge: [], misplaced: [], total: 0 }),
      });

      await api.getSuggestions({ repo: 'main', threshold: 0.9 });

      expect(mockFetch).toHaveBeenCalledWith(
        '/api/organize/suggestions?repo=main&threshold=0.9',
        expect.any(Object)
      );
    });

    it('should build query params for getDocument', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            id: 'abc123',
            title: 'Test',
            doc_type: 'note',
            repo_id: 'main',
            file_path: 'test.md',
          }),
      });

      await api.getDocument('abc123', { include_preview: true, max_content_length: 1000 });

      expect(mockFetch).toHaveBeenCalledWith(
        '/api/documents/abc123?include_preview=true&max_content_length=1000',
        expect.any(Object)
      );
    });

    it('should encode special characters in path params', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ documents: [], total: 0, answered: 0, unanswered: 0 }),
      });

      await api.getDocumentReview('doc/with/slashes');

      expect(mockFetch).toHaveBeenCalledWith(
        '/api/review/queue/doc%2Fwith%2Fslashes',
        expect.any(Object)
      );
    });
  });

  describe('ApiRequestError', () => {
    it('should identify 404 as not found', () => {
      const error = new ApiRequestError('Not found', 'NOT_FOUND', 404);
      expect(error.isNotFound).toBe(true);
      expect(error.isBadRequest).toBe(false);
      expect(error.isServerError).toBe(false);
    });

    it('should identify 400 as bad request', () => {
      const error = new ApiRequestError('Bad request', 'BAD_REQUEST', 400);
      expect(error.isNotFound).toBe(false);
      expect(error.isBadRequest).toBe(true);
      expect(error.isServerError).toBe(false);
    });

    it('should identify 500+ as server error', () => {
      const error = new ApiRequestError('Server error', 'INTERNAL_ERROR', 500);
      expect(error.isNotFound).toBe(false);
      expect(error.isBadRequest).toBe(false);
      expect(error.isServerError).toBe(true);
    });

    it('should identify NOT_FOUND code regardless of status', () => {
      const error = new ApiRequestError('Not found', 'NOT_FOUND', 200);
      expect(error.isNotFound).toBe(true);
    });
  });

  describe('endpoint methods', () => {
    it('should call getStats endpoint', async () => {
      const mockData = { repos_count: 2, docs_count: 50, db_size_bytes: 2048 };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockData),
      });

      const result = await api.getStats();
      expect(result).toEqual(mockData);
    });

    it('should call bulkAnswerQuestions with correct body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, results: [], errors: [] }),
      });

      const answers = [
        { doc_id: 'doc1', question_index: 0, answer: 'answer1' },
        { doc_id: 'doc2', question_index: 1, answer: 'answer2' },
      ];
      await api.bulkAnswerQuestions(answers);

      expect(mockFetch).toHaveBeenCalledWith('/api/review/bulk-answer', {
        headers: { 'Content-Type': 'application/json' },
        method: 'POST',
        body: JSON.stringify({ answers }),
      });
    });

    it('should call assignOrphan with correct body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, assigned: 1, remaining: 5 }),
      });

      await api.assignOrphan('main', 5, 'target_doc');

      expect(mockFetch).toHaveBeenCalledWith('/api/organize/assign-orphan', {
        headers: { 'Content-Type': 'application/json' },
        method: 'POST',
        body: JSON.stringify({ repo: 'main', line_number: 5, target: 'target_doc' }),
      });
    });

    it('should call dismissSuggestion with correct body', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      await api.dismissSuggestion('merge', 'doc1', 'doc2');

      expect(mockFetch).toHaveBeenCalledWith('/api/organize/dismiss', {
        headers: { 'Content-Type': 'application/json' },
        method: 'POST',
        body: JSON.stringify({ type: 'merge', doc_id: 'doc1', target_id: 'doc2' }),
      });
    });

    it('should call getOrphans with repo param', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ orphans: [], total: 0, answered: 0, unanswered: 0 }),
      });

      await api.getOrphans('main');

      expect(mockFetch).toHaveBeenCalledWith('/api/organize/orphans?repo=main', expect.any(Object));
    });
  });
});
