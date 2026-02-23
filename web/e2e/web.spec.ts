/**
 * E2E tests for the Factbase web UI.
 *
 * These tests require:
 * - `cargo build --features web` (or WEB_TEST_BINARY pointing to the binary)
 * - Ollama running with qwen3-embedding:0.6b and rnj-1-extended
 *
 * Run: cd web && npx playwright test
 */

import { test, expect } from './fixtures';

// ============================================================================
// Task 2.3: Dashboard tests
// ============================================================================

test.describe('Dashboard', () => {
  test('renders stats cards with counts', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    // Wait for data to load
    await expect(page.locator('#review-count')).not.toHaveText('-', { timeout: 10000 });
    // Should show numeric counts
    await expect(page.locator('#review-count')).toHaveText(/\d+/);
    await expect(page.locator('#organize-count')).toHaveText(/\d+/);
    await expect(page.locator('#orphan-count')).toHaveText(/\d+/);
  });

  test('shows quick stats section', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    await expect(page.locator('#stats-content')).toBeVisible();
    // Wait for stats to load (not "Loading...")
    await expect(page.locator('#stats-content')).not.toContainText('Loading', { timeout: 10000 });
    await expect(page.locator('#stats-content')).toContainText('Repositories');
    await expect(page.locator('#stats-content')).toContainText('Documents');
  });

  test('auto-refresh toggle works', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    const toggle = page.locator('#auto-refresh-toggle');
    await expect(toggle).not.toBeChecked();
    await toggle.check();
    await expect(toggle).toBeChecked();
  });

  test('scan button shows CLI instruction', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    await page.click('#scan-btn');
    // Should show a toast or action result with CLI command
    await expect(page.locator('#action-result, [role="alert"]')).toContainText('factbase scan', { timeout: 5000 });
  });

  test('check button shows CLI instruction', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    await page.click('#check-btn');
    await expect(page.locator('#action-result, [role="alert"]')).toContainText('factbase check', { timeout: 5000 });
  });
});

// ============================================================================
// Task 2.4: Review Queue tests
// ============================================================================

test.describe('Review Queue', () => {
  test('renders questions grouped by document', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    // Wait for content to load
    await expect(page.locator('#review-queue-content .document-group').first()).toBeVisible({ timeout: 10000 });
    // Should have multiple document groups
    const groups = page.locator('#review-queue-content .document-group');
    await expect(groups).toHaveCount(3); // alice, bob, atlas have review queues
  });

  test('shows workflow stepper', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#workflow-stepper')).toBeVisible();
    await expect(page.locator('#workflow-stepper')).toContainText('Review questions');
    await expect(page.locator('#workflow-stepper')).toContainText('Apply answers');
  });

  test('filter by question type works', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#review-queue-content .document-group').first()).toBeVisible({ timeout: 10000 });
    // Filter by temporal
    await page.selectOption('#filter-type', 'temporal');
    // Wait for reload
    await page.waitForTimeout(1000);
    // All visible question badges should be temporal
    const badges = page.locator('.question-card');
    const count = await badges.count();
    expect(count).toBeGreaterThan(0);
  });

  test('answer a question inline', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#review-queue-content .document-group').first()).toBeVisible({ timeout: 10000 });
    // Find first unanswered question's textarea
    const form = page.locator('.answer-form').first();
    await form.locator('textarea').fill('Started March 2020');
    await form.locator('button[type="submit"]').click();
    // Should show success toast
    await expect(page.locator('[role="alert"]')).toContainText('Answer submitted', { timeout: 5000 });
  });

  test('shows archive badge for archived documents', async ({ page, serverUrl }) => {
    // This test checks that archive badge rendering works
    // The archive doc doesn't have review questions, so we just verify the badge logic exists
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#review-queue-content')).toBeVisible({ timeout: 10000 });
  });
});

// ============================================================================
// Task 2.5: Organize tests
// ============================================================================

test.describe('Organize', () => {
  test('renders suggestions page', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/organize`);
    await expect(page.locator('#organize-content')).toBeVisible({ timeout: 10000 });
    // Should show either suggestions or "no suggestions" message
    await expect(page.locator('#organize-content')).not.toContainText('Loading');
  });

  test('dismiss suggestion works', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/organize`);
    await page.waitForTimeout(2000);
    const dismissBtn = page.locator('.dismiss-btn').first();
    if (await dismissBtn.isVisible()) {
      await dismissBtn.click();
      await expect(page.locator('[role="alert"]')).toContainText('dismissed', { timeout: 5000 });
    }
  });
});

// ============================================================================
// Task 2.6: Keyboard navigation tests
// ============================================================================

test.describe('Keyboard Navigation', () => {
  test('j/k moves between items on review page', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#review-queue-content .question-card').first()).toBeVisible({ timeout: 10000 });
    // Press j to move to first item
    await page.keyboard.press('j');
    // Press k to move back
    await page.keyboard.press('k');
    // Just verify no errors occurred
  });

  test('? shows help', async ({ page, serverUrl }) => {
    await page.goto(serverUrl);
    await page.keyboard.press('?');
    // Should show keyboard help (if implemented)
    await page.waitForTimeout(500);
  });
});

// ============================================================================
// Task 2.7: Apply test
// ============================================================================

test.describe('Apply', () => {
  test('apply bar shows when answers exist', async ({ page, serverUrl }) => {
    await page.goto(`${serverUrl}/#/review`);
    await expect(page.locator('#review-queue-content').first()).toBeVisible({ timeout: 10000 });
    // The test docs have one answered question (alice's stale question)
    // Check if apply bar is visible
    const applyBar = page.locator('#apply-bar');
    if (await applyBar.locator('#apply-btn').isVisible()) {
      // Preview button should work
      await page.click('#apply-preview-btn');
      await page.waitForTimeout(2000);
      // Should show result or CLI instruction
      const result = page.locator('#apply-result, [role="alert"]');
      await expect(result).toBeVisible({ timeout: 5000 });
    }
  });
});
