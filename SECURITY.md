# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability in factbase, please report it responsibly.

**Email**: Open a private issue on the [repository](https://github.com/everyonce/factbase) or contact the maintainer directly.

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact

We will acknowledge reports within 72 hours and aim to release a fix promptly.

**Do not** open public issues for security vulnerabilities.

## Security Considerations

### Data Storage

Factbase stores all indexed content in a **plaintext SQLite database** (`factbase.db`). There is no encryption at rest. Anyone with filesystem access to the database file can read all indexed document content, embeddings, and metadata.

If your knowledge base contains sensitive information:
- Restrict filesystem permissions on the database file and its parent directory
- Use full-disk encryption at the OS level
- Do not place the database on shared or unencrypted storage

### Network Communication

**Amazon Bedrock** (default inference backend): All API calls use HTTPS via the AWS SDK. Credentials are managed through standard AWS credential chains (environment variables, `~/.aws/credentials`, IAM roles). No credentials are stored by factbase.

**Ollama** (alternative backend): Calls go to `localhost` by default over HTTP. If you configure a remote Ollama endpoint, ensure you use a trusted network or TLS termination.

### MCP Server

The MCP server binds to `127.0.0.1:3000` by default (localhost only). It does **not** require authentication. Do not expose it to untrusted networks.

The optional web UI binds to `127.0.0.1:3001` with the same constraints.

### File System Access

Factbase reads and writes markdown files in registered repositories. It injects a tracking comment (`<!-- factbase:XXXXXX -->`) into files on first scan. The `review --apply` and `organize` commands modify file content via the agent — use `--dry-run` to preview changes.

## Supported Versions

Security fixes are applied to the latest release only.
