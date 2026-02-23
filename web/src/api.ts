/**
 * Factbase API client.
 * Typed functions for all backend API endpoints.
 */

// ============================================================================
// Types
// ============================================================================

export interface ApiError {
  error: string;
  code: string;
}

// Stats types
export interface AggregateStats {
  repos_count: number;
  docs_count: number;
  db_size_bytes: number;
  last_scan?: string;
}

export interface ReviewStats {
  total: number;
  answered: number;
  unanswered: number;
}

export interface OrganizeStats {
  merge_candidates: number;
  misplaced_candidates: number;
  orphan_count: number;
}

// Review types
export interface ReviewQuestion {
  question_type: string;
  description: string;
  line_ref?: number;
  answered: boolean;
  answer?: string;
}

export interface DocumentReview {
  doc_id: string;
  doc_title: string;
  file_path: string;
  questions: ReviewQuestion[];
}

export interface ReviewQueueResponse {
  documents: DocumentReview[];
  total: number;
  answered: number;
  unanswered: number;
}

export interface AnswerResult {
  success: boolean;
  doc_id: string;
  question_index: number;
}

export interface BulkAnswerResult {
  success: boolean;
  results: AnswerResult[];
  errors: string[];
}

// Organize types
export interface MergeCandidate {
  doc1_id: string;
  doc1_title: string;
  doc2_id: string;
  doc2_title: string;
  similarity: number;
}

export interface MisplacedCandidate {
  doc_id: string;
  doc_title: string;
  current_type: string;
  suggested_type: string;
  reason: string;
}

export interface SuggestionsResponse {
  merge: MergeCandidate[];
  misplaced: MisplacedCandidate[];
  total: number;
}

export interface OrphanEntry {
  content: string;
  source_doc?: string;
  source_line?: number;
  answered: boolean;
  answer?: string;
  line_number: number;
}

export interface OrphansResponse {
  orphans: OrphanEntry[];
  total: number;
  answered: number;
  unanswered: number;
}

// Document types
export interface DocumentLink {
  id: string;
  title: string;
}

export interface Document {
  id: string;
  title: string;
  doc_type: string;
  repo_id: string;
  file_path: string;
  content?: string;
  preview?: string;
  links_to?: DocumentLink[];
  linked_from?: DocumentLink[];
}

export interface DocumentLinks {
  id: string;
  title: string;
  links_to: DocumentLink[];
  linked_from: DocumentLink[];
}

export interface Repository {
  id: string;
  name: string;
  path: string;
  doc_count: number;
}

// ============================================================================
// API Client
// ============================================================================

const BASE_URL = '';  // Same origin

class ApiClient {
  private async request<T>(path: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${BASE_URL}${path}`, {
      headers: { 'Content-Type': 'application/json' },
      ...options,
    });

    if (!response.ok) {
      const error: ApiError = await response.json().catch(() => ({
        error: `HTTP ${response.status}: ${response.statusText}`,
        code: 'HTTP_ERROR',
      }));
      throw new ApiRequestError(error.error, error.code, response.status);
    }

    return response.json();
  }

  // ---------------------------------------------------------------------------
  // Stats endpoints
  // ---------------------------------------------------------------------------

  async getStats(): Promise<AggregateStats> {
    return this.request('/api/stats');
  }

  async getReviewStats(): Promise<ReviewStats> {
    return this.request('/api/stats/review');
  }

  async getOrganizeStats(): Promise<OrganizeStats> {
    return this.request('/api/stats/organize');
  }

  // ---------------------------------------------------------------------------
  // Review endpoints
  // ---------------------------------------------------------------------------

  async getReviewQueue(params?: { repo?: string; type?: string }): Promise<ReviewQueueResponse> {
    const query = new URLSearchParams();
    if (params?.repo) query.set('repo', params.repo);
    if (params?.type) query.set('type', params.type);
    const qs = query.toString();
    return this.request(`/api/review/queue${qs ? `?${qs}` : ''}`);
  }

  async getDocumentReview(docId: string): Promise<DocumentReview> {
    return this.request(`/api/review/queue/${encodeURIComponent(docId)}`);
  }

  async answerQuestion(docId: string, questionIndex: number, answer: string): Promise<AnswerResult> {
    return this.request(`/api/review/answer/${encodeURIComponent(docId)}`, {
      method: 'POST',
      body: JSON.stringify({ question_index: questionIndex, answer }),
    });
  }

  async bulkAnswerQuestions(
    answers: Array<{ doc_id: string; question_index: number; answer: string }>
  ): Promise<BulkAnswerResult> {
    return this.request('/api/review/bulk-answer', {
      method: 'POST',
      body: JSON.stringify({ answers }),
    });
  }

  async getReviewStatus(): Promise<ReviewStats> {
    return this.request('/api/review/status');
  }

  // ---------------------------------------------------------------------------
  // Organize endpoints
  // ---------------------------------------------------------------------------

  async getSuggestions(params?: {
    repo?: string;
    type?: string;
    threshold?: number;
  }): Promise<SuggestionsResponse> {
    const query = new URLSearchParams();
    if (params?.repo) query.set('repo', params.repo);
    if (params?.type) query.set('type', params.type);
    if (params?.threshold !== undefined) query.set('threshold', params.threshold.toString());
    const qs = query.toString();
    return this.request(`/api/organize/suggestions${qs ? `?${qs}` : ''}`);
  }

  async getDocumentSuggestions(docId: string): Promise<SuggestionsResponse> {
    return this.request(`/api/organize/suggestions/${encodeURIComponent(docId)}`);
  }

  async dismissSuggestion(
    type: 'merge' | 'misplaced',
    docId: string,
    targetId?: string
  ): Promise<{ success: boolean }> {
    return this.request('/api/organize/dismiss', {
      method: 'POST',
      body: JSON.stringify({ type, doc_id: docId, target_id: targetId }),
    });
  }

  async getOrphans(repo: string): Promise<OrphansResponse> {
    return this.request(`/api/organize/orphans?repo=${encodeURIComponent(repo)}`);
  }

  async assignOrphan(
    repo: string,
    lineNumber: number,
    target: string
  ): Promise<{ success: boolean; assigned?: number; dismissed?: number; remaining?: number }> {
    return this.request('/api/organize/assign-orphan', {
      method: 'POST',
      body: JSON.stringify({
        repo,
        line_number: lineNumber,
        target,
      }),
    });
  }

  // ---------------------------------------------------------------------------
  // Document endpoints
  // ---------------------------------------------------------------------------

  async getDocument(
    id: string,
    params?: { include_preview?: boolean; max_content_length?: number }
  ): Promise<Document> {
    const query = new URLSearchParams();
    if (params?.include_preview !== undefined)
      query.set('include_preview', params.include_preview.toString());
    if (params?.max_content_length !== undefined)
      query.set('max_content_length', params.max_content_length.toString());
    const qs = query.toString();
    return this.request(`/api/documents/${encodeURIComponent(id)}${qs ? `?${qs}` : ''}`);
  }

  async getDocumentLinks(id: string): Promise<DocumentLinks> {
    return this.request(`/api/documents/${encodeURIComponent(id)}/links`);
  }

  async getRepositories(): Promise<{ repositories: Repository[] }> {
    return this.request('/api/repos');
  }
}

// ============================================================================
// Error class
// ============================================================================

export class ApiRequestError extends Error {
  constructor(
    message: string,
    public code: string,
    public status: number
  ) {
    super(message);
    this.name = 'ApiRequestError';
  }

  get isNotFound(): boolean {
    return this.status === 404 || this.code === 'NOT_FOUND';
  }

  get isBadRequest(): boolean {
    return this.status === 400 || this.code === 'BAD_REQUEST';
  }

  get isServerError(): boolean {
    return this.status >= 500;
  }
}

// ============================================================================
// Singleton export
// ============================================================================

export const api = new ApiClient();
