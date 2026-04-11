# Changelog

## [Unreleased]

## [0.2.0] - 2026-04-11

### Added
- `rsdiffy review` subcommand — runs an AI agent (Claude or Codex) to review changes and leave comments directly in rsdiffy. Supports `--agent`, `--prompt`, and positional ref argument.
- `rsdiffy export` subcommand — exports review comments and code tours as structured JSON for consumption by LLM agents and scripts. Supports `--git-ref` and `--status` filters.
- **@agent mentions in comments** — write `@claude` or `@codex` in any comment or reply to invoke the agent inline. The agent receives the file-specific diff, thread context (file, lines, conversation history), and is pre-granted read permissions to explore the codebase. Responses appear as bot replies in the same discussion thread.

### Changed
- Data directory moved from `~/.rsdiffy/` to `~/.config/rsdiffy/` to conform with XDG Base Directory conventions.
- Linux release binaries now use musl (static linking) instead of glibc for broader compatibility.
- TLS backend switched from OpenSSL (`native-tls`) to `rustls` — no system dependencies required.

## [0.1.0] - 2026-04-10

Initial public release.

### Features
- Split and unified diff views with word-level highlighting
- File tree browser for navigating repository files
- Inline review comments stored locally in SQLite
- Code tours — annotated walkthroughs of changes
- GitLab integration — push/pull comments to merge requests (supports self-hosted)
- Revert individual files or hunks from the browser
- Open in editor — jump to file:line in VS Code
- Instance management — auto-reuse running instances, list, kill
- Dark mode and configurable themes
- Single binary — UI assets embedded at compile time

### Platforms
- Linux x86_64 (static musl)
- Linux ARM64 (static musl)
- macOS Intel
- macOS Apple Silicon
- Windows x86_64
