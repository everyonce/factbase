/**
 * Shared fixture: starts `factbase serve` with a temp repo.
 *
 * Usage: run `cargo build --features web` first, then:
 *   WEB_TEST_BINARY=/path/to/factbase npx playwright test
 *
 * The fixture creates a temp directory with test documents,
 * initializes a repo, scans it, and starts the web server.
 */

import { test as base, expect } from '@playwright/test';
import { execSync, spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const BINARY = process.env.WEB_TEST_BINARY || 'factbase';
const WEB_PORT = 3001;
const MCP_PORT = 3000;

interface Fixtures {
  serverUrl: string;
}

// Test documents with review questions
const TEST_DOCS: Record<string, string> = {
  'people/alice.md': `# Alice Smith

- Works at Acme Corp @t[2020..] [^1]
- Lives in Austin @t[~2024-01]

## Review Queue

- @q[temporal] When did Alice start at Acme Corp? Exact month?
- @q[stale] Is Austin still current? Last verified over a year ago.
  - [x] Still accurate as of today

---
[^1]: LinkedIn profile, checked 2024-01-15
`,
  'people/bob.md': `# Bob Jones

- Senior engineer at Widgets Inc
- Previously at StartupCo @t[2018..2020]

## Review Queue

- @q[missing] No source for Widgets Inc employment
- @q[temporal] When did Bob join Widgets Inc?
`,
  'projects/atlas.md': `# Project Atlas

- Internal codename for the Q4 initiative @t[=2024-Q3]
- Led by Alice Smith
- Budget: $2M

## Review Queue

- @q[ambiguous] Which Q4 — 2023 or 2024?
`,
  'topics/rust.md': `# Rust Programming

- Systems programming language
- Used by Acme Corp for backend services @t[2022..]
`,
  'archive/old-project.md': `# Old Project

- Completed in 2019
- No longer relevant
`,
};

let serverProcess: ChildProcess | null = null;
let tempDir: string | null = null;

export const test = base.extend<Fixtures>({
  serverUrl: async ({}, use) => {
    // Create temp directory with test documents
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'factbase-e2e-'));
    const repoDir = path.join(tempDir, 'repo');

    // Write test documents
    for (const [relPath, content] of Object.entries(TEST_DOCS)) {
      const fullPath = path.join(repoDir, relPath);
      fs.mkdirSync(path.dirname(fullPath), { recursive: true });
      fs.writeFileSync(fullPath, content);
    }

    // Create config
    const configDir = path.join(tempDir, 'config');
    fs.mkdirSync(configDir, { recursive: true });
    const dbPath = path.join(tempDir, 'factbase.db');
    fs.writeFileSync(path.join(configDir, 'config.yaml'), `
database:
  path: ${dbPath}
  pool_size: 2
embedding:
  provider: ollama
  model: qwen3-embedding:0.6b
  base_url: http://localhost:11434
  dimension: 1024
llm:
  provider: ollama
  model: rnj-1-extended
  base_url: http://localhost:11434
server:
  host: 127.0.0.1
  port: ${MCP_PORT}
web:
  enabled: true
  port: ${WEB_PORT}
`);

    // Initialize and scan
    const env = { ...process.env, XDG_CONFIG_HOME: tempDir };
    try {
      execSync(`${BINARY} init "${repoDir}"`, { env, timeout: 10000 });
      execSync(`${BINARY} scan --skip-links --skip-embeddings`, { env, cwd: repoDir, timeout: 30000 });
    } catch (e) {
      console.error('Setup failed:', e);
      throw e;
    }

    // Start serve
    serverProcess = spawn(BINARY, ['serve'], {
      env,
      cwd: repoDir,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    // Wait for web server to be ready
    const ready = await waitForServer(`http://127.0.0.1:${WEB_PORT}/api/health`, 15000);
    if (!ready) {
      throw new Error('Web server did not start in time');
    }

    await use(`http://127.0.0.1:${WEB_PORT}`);

    // Cleanup
    if (serverProcess) {
      serverProcess.kill('SIGTERM');
      serverProcess = null;
    }
    if (tempDir) {
      fs.rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  },
});

async function waitForServer(url: string, timeoutMs: number): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch(url);
      if (resp.ok) return true;
    } catch {
      // Not ready yet
    }
    await new Promise(r => setTimeout(r, 500));
  }
  return false;
}

export { expect };
