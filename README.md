# rsdiffy

GitLab-style git diff viewer in the browser, written in Rust.

A single binary that starts a local web server, parses git diffs, and renders them with a rich UI — split/unified views, syntax highlighting, inline comments, code tours, and optional GitLab MR integration.

## Features

- **Split & unified diff views** with word-level highlighting
- **File tree browser** for navigating repository files
- **Inline review comments** stored locally in SQLite
- **Code tours** — annotated walkthroughs of changes
- **GitLab integration** — push/pull comments to merge requests (supports self-hosted)
- **Revert** individual files or hunks from the browser
- **Open in editor** — jump to file:line in VS Code
- **Instance management** — auto-reuse running instances, list, kill
- **Dark mode** and configurable themes
- **Single binary** — UI assets embedded at compile time

## Installation

### Quick install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/apogiatzis/rsdiffy/main/install.sh | sh
```

### Download from releases

Pre-built binaries are available on the [Releases](https://github.com/apogiatzis/rsdiffy/releases) page.

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `rsdiffy-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `rsdiffy-aarch64-unknown-linux-musl.tar.gz` |
| macOS Intel | `rsdiffy-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `rsdiffy-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `rsdiffy-x86_64-pc-windows-msvc.zip` |

### Build from source

Requires Rust 1.87+ and the UI assets built into `ui/`.

```bash
cargo install --path crates/cli
```

## Usage

```bash
# See all uncommitted changes
rsdiffy

# What changed since main
rsdiffy main

# Review your last commit
rsdiffy HEAD~1

# Compare two branches
rsdiffy main..feature
rsdiffy --base main --compare feature

# Compare two tags
rsdiffy v1.0.0 v2.0.0

# Only staged / unstaged changes
rsdiffy staged
rsdiffy unstaged

# Dark mode, unified view
rsdiffy --dark --unified

# Force restart existing instance
rsdiffy --new
```

### Subcommands

```bash
rsdiffy tree      # Browse repository files
rsdiffy list      # List running instances
rsdiffy kill      # Stop all running instances
rsdiffy prune     # Remove all rsdiffy data (~/.config/rsdiffy)
rsdiffy doctor    # Check that rsdiffy can run correctly
rsdiffy export    # Export review comments as JSON
rsdiffy review    # AI-powered code review
```

### GitLab integration

Set your GitLab token as an environment variable:

```bash
export GITLAB_TOKEN=glpat-xxxxxxxxxxxx
# or
export GITLAB_PRIVATE_TOKEN=glpat-xxxxxxxxxxxx
```

rsdiffy auto-detects the GitLab remote from your git config and enables push/pull of review comments to merge requests. Self-hosted GitLab instances are supported.

## Architecture

Cargo workspace with four crates:

| Crate | Purpose |
|-------|---------|
| `rsdiffy-parser` | Unified diff parser |
| `rsdiffy-git` | Git operations via shell-out |
| `rsdiffy-gitlab` | GitLab REST API client |
| `rsdiffy` (cli) | Axum HTTP server, CLI, embedded UI |

## License

[MIT](LICENSE)
