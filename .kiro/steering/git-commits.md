# Git Commit Requirements

After making any changes to:
- Code files
- Documentation
- Requirements or specifications
- Configuration files

Always perform a git commit with:
1. A clear, descriptive commit message summarizing what changed
2. Use conventional commit format (e.g., `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`)
3. Include details about why the change was made in the commit body if relevant

Do not batch unrelated changes - commit each logical change separately.

Once complete, do a git push if possible to commit to remote repo.

## Version Bumps

The version in `Cargo.toml` drives npm package publishing. At the end of a work session any commits were made:

1. Bump the version in `Cargo.toml` following semver:
   - `feat:` changes → bump minor (e.g., 0.5.0 → 0.6.0)
   - `fix:` only → bump patch (e.g., 0.5.0 → 0.5.1)
2. Commit as `release: vX.Y.Z` with a summary of what changed
3. The npm publish script (`scripts/npm-publish.sh`) reads this version automatically

Do bump the version on every individual commit — only once at the end of a session when ready to release.