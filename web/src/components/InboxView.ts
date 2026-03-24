/**
 * InboxView component.
 * Daily review entry point: 3-bucket summary with KB health trend.
 *
 * Buckets:
 *  🟢 Auto-resolved   — already answered, no human action needed
 *  🟡 Needs approval  — agent has high-confidence suggestion, human confirms/rejects
 *  🔴 Needs your input — agent deferred, genuinely uncertain
 */

export interface InboxCounts {
  autoResolved: number;
  needsApproval: number;
  needsInput: number;
}

export interface TrendEntry {
  date: string; // YYYY-MM-DD
  count: number;
}

const TREND_KEY = 'factbase_inbox_trend';
const TREND_DAYS = 7;

export function loadTrend(): TrendEntry[] {
  try {
    const raw = localStorage.getItem(TREND_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as TrendEntry[];
  } catch {
    return [];
  }
}

export function saveTrend(needsInputCount: number): TrendEntry[] {
  const today = new Date().toISOString().slice(0, 10);
  let entries = loadTrend();
  const idx = entries.findIndex(e => e.date === today);
  if (idx >= 0) {
    entries[idx].count = needsInputCount;
  } else {
    entries.push({ date: today, count: needsInputCount });
  }
  // Prune to last TREND_DAYS days
  const cutoff = new Date();
  cutoff.setDate(cutoff.getDate() - TREND_DAYS);
  const cutoffStr = cutoff.toISOString().slice(0, 10);
  entries = entries.filter(e => e.date >= cutoffStr);
  try {
    localStorage.setItem(TREND_KEY, JSON.stringify(entries));
  } catch {
    // ignore storage errors
  }
  return entries;
}

/** Compute a health trend message from the last 7 days of needsInput counts. */
export function computeTrendMessage(entries: TrendEntry[], currentNeedsInput: number): string {
  if (entries.length < 3) return '';

  // Compare average of first half vs second half
  const mid = Math.floor(entries.length / 2);
  const firstHalf = entries.slice(0, mid);
  const secondHalf = entries.slice(mid);
  const avgFirst = firstHalf.reduce((s, e) => s + e.count, 0) / firstHalf.length;
  const avgSecond = secondHalf.reduce((s, e) => s + e.count, 0) / secondHalf.length;
  const delta = avgSecond - avgFirst;

  if (delta > 0.5 && currentNeedsInput > 0) {
    const perDay = Math.round(delta * 10) / 10;
    return `⚠️ Unresolvable questions are increasing (+${perDay}/day average). This may indicate agent prompt degradation or KB structural issues — consider running a maintain pass.`;
  }

  if (currentNeedsInput <= 1) {
    return '✅ KB health good — agent is handling most questions automatically.';
  }

  return '';
}

export function renderInboxView(counts: InboxCounts, trendMessage: string): string {
  const approvalDisabled = counts.needsApproval === 0;
  const inputDisabled = counts.needsInput === 0;

  return `
    <div class="inbox-view space-y-4">
      <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6">
        <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-4">KB Health — last 7 days</h3>
        <div class="space-y-3">
          <div class="flex items-center justify-between p-4 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-100 dark:border-green-800">
            <div class="flex items-center space-x-3">
              <span class="text-xl" aria-hidden="true">🟢</span>
              <div>
                <p class="text-sm font-medium text-gray-800 dark:text-gray-200">Auto-resolved</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">agent handled, no human action needed</p>
              </div>
            </div>
            <span class="text-3xl font-bold text-green-600 dark:text-green-400" aria-label="${counts.autoResolved} auto-resolved">${counts.autoResolved}</span>
          </div>

          <button
            id="inbox-needs-approval-btn"
            class="w-full flex items-center justify-between p-4 rounded-lg bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-100 dark:border-yellow-800 text-left transition-colors ${approvalDisabled ? 'opacity-50 cursor-default' : 'hover:bg-yellow-100 dark:hover:bg-yellow-900/40 cursor-pointer'}"
            ${approvalDisabled ? 'disabled' : ''}
            aria-label="Needs approval: ${counts.needsApproval} questions"
          >
            <div class="flex items-center space-x-3">
              <span class="text-xl" aria-hidden="true">🟡</span>
              <div>
                <p class="text-sm font-medium text-gray-800 dark:text-gray-200">Needs approval</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">agent has suggestion, human confirms/rejects</p>
              </div>
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-3xl font-bold text-yellow-600 dark:text-yellow-400">${counts.needsApproval}</span>
              ${!approvalDisabled ? '<span class="text-gray-400 dark:text-gray-500 text-lg">›</span>' : ''}
            </div>
          </button>

          <button
            id="inbox-needs-input-btn"
            class="w-full flex items-center justify-between p-4 rounded-lg bg-red-50 dark:bg-red-900/20 border border-red-100 dark:border-red-800 text-left transition-colors ${inputDisabled ? 'opacity-50 cursor-default' : 'hover:bg-red-100 dark:hover:bg-red-900/40 cursor-pointer'}"
            ${inputDisabled ? 'disabled' : ''}
            aria-label="Needs your input: ${counts.needsInput} questions"
          >
            <div class="flex items-center space-x-3">
              <span class="text-xl" aria-hidden="true">🔴</span>
              <div>
                <p class="text-sm font-medium text-gray-800 dark:text-gray-200">Needs your input</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">agent tried and failed, genuinely uncertain</p>
              </div>
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-3xl font-bold text-red-600 dark:text-red-400">${counts.needsInput}</span>
              ${!inputDisabled ? '<span class="text-gray-400 dark:text-gray-500 text-lg">›</span>' : ''}
            </div>
          </button>
        </div>

        ${trendMessage ? `
          <div class="mt-4 p-3 rounded-lg text-sm ${trendMessage.startsWith('⚠️') ? 'bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 text-amber-800 dark:text-amber-200' : 'bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-700 text-green-800 dark:text-green-200'}">
            ${trendMessage}
          </div>
        ` : ''}
      </div>
    </div>
  `;
}
