mod agent;
mod assets;
mod db;
mod registry;
mod routes;
mod server;
mod session;
mod threads;
mod tours;
mod unescape;

use std::process;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use rusqlite::Connection;

use rsdiffy_git::{diff, repo};

use server::ServerOptions;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PORT: u16 = 5391;

#[derive(Parser)]
#[command(
    name = "rsdiffy",
    about = "GitLab-style git diff viewer in the browser",
    version = VERSION,
    after_help = r#"Common usage:
  $ rsdiffy                              See all uncommitted changes
  $ rsdiffy main                         What changed since main
  $ rsdiffy HEAD~1                       Review your last commit
  $ rsdiffy main..feature                Compare two branches
  $ rsdiffy --base main --compare feature   Same as above
  $ rsdiffy v1.0.0 v2.0.0               Compare two tags
  $ rsdiffy staged                       Only staged changes
  $ rsdiffy unstaged                     Only unstaged changes
  $ rsdiffy --dark --unified             Dark mode, unified view
  $ rsdiffy --new                        Force restart existing instance

Other commands:
  $ rsdiffy tree                         Browse repository files
  $ rsdiffy list                         List running instances
  $ rsdiffy kill                         Stop all running instances
  $ rsdiffy prune                        Remove all rsdiffy data
  $ rsdiffy review main                   AI review of changes from main
  $ rsdiffy review --agent codex          Use a different agent
  $ rsdiffy export                       Export review comments as JSON
  $ rsdiffy export --status open         Export only open comments

Comment management (for agents/scripts):
  $ rsdiffy comment add --file src/main.rs --line 42 --body "Bug here"
  $ rsdiffy comment list --status open   List open threads
  $ rsdiffy comment reply --thread <id> --body "Fixed"
  $ rsdiffy comment resolve --thread <id>
  $ rsdiffy import --ref main < review.json   Batch import comments"#
)]
struct Cli {
    /// Git refs to diff
    #[arg(trailing_var_arg = true)]
    refs: Vec<String>,

    /// Base ref to compare from (e.g. main, HEAD~3, v1.0.0)
    #[arg(long)]
    base: Option<String>,

    /// Ref to compare against base (default: working tree)
    #[arg(long)]
    compare: Option<String>,

    /// Port to use (default: auto-assigned from 5391)
    #[arg(long)]
    port: Option<u16>,

    /// Do not open browser automatically
    #[arg(long = "no-open")]
    no_open: bool,

    /// Minimal terminal output
    #[arg(long)]
    quiet: bool,

    /// Open in dark mode
    #[arg(long)]
    dark: bool,

    /// Open in unified view (default: split)
    #[arg(long)]
    unified: bool,

    /// Stop existing instance and start fresh
    #[arg(long = "new")]
    force_new: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Browse repository files
    Tree {
        /// Port to use
        #[arg(long)]
        port: Option<u16>,
        /// Do not open browser automatically
        #[arg(long = "no-open")]
        no_open: bool,
        /// Open in dark mode
        #[arg(long)]
        dark: bool,
        /// Minimal terminal output
        #[arg(long)]
        quiet: bool,
        /// Stop existing instance and start fresh
        #[arg(long = "new")]
        force_new: bool,
    },
    /// List all running rsdiffy instances
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Stop all running rsdiffy instances
    Kill,
    /// Remove all rsdiffy data
    Prune,
    /// Check that rsdiffy can run correctly
    Doctor,
    /// Run an AI agent to review changes and leave comments
    Review {
        /// Git ref to review (e.g. main, HEAD~1, staged)
        #[arg(name = "REF")]
        git_ref: Option<String>,
        /// Agent to use: claude, codex, or a custom command
        #[arg(long, default_value = "claude")]
        agent: String,
        /// Custom prompt to prepend to the review instructions
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Export review comments for the current session
    Export {
        /// Git ref to export comments for (default: current session)
        #[arg(long)]
        git_ref: Option<String>,
        /// Filter by status: open, resolved, all (default: all)
        #[arg(long, default_value = "all")]
        status: String,
    },
    /// Manage review comments from the CLI
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },
    /// Batch import comments from JSON (stdin or file)
    Import {
        /// Git ref for session (default: current session or "work")
        #[arg(long = "ref")]
        git_ref: Option<String>,
        /// Author name for imported comments
        #[arg(long, default_value = "cli")]
        author: String,
        /// Author type: user or bot
        #[arg(long = "author-type", default_value = "bot")]
        author_type: String,
        /// Read from file instead of stdin
        #[arg(long)]
        file: Option<String>,
    },
}

#[derive(Subcommand)]
enum CommentAction {
    /// Add a comment on a specific file and line
    Add {
        /// File path relative to repo root
        #[arg(long)]
        file: String,
        /// Start line number
        #[arg(long)]
        line: i64,
        /// End line number (defaults to start line)
        #[arg(long)]
        end_line: Option<i64>,
        /// Comment body in markdown
        #[arg(long)]
        body: String,
        /// Author name
        #[arg(long, default_value = "cli")]
        author: String,
        /// Author type: user or bot
        #[arg(long = "author-type", default_value = "bot")]
        author_type: String,
        /// Side: new or old
        #[arg(long, default_value = "new")]
        side: String,
        /// Git ref for session
        #[arg(long = "ref")]
        git_ref: Option<String>,
    },
    /// Reply to an existing comment thread
    Reply {
        /// Thread ID (supports 8+ char prefix)
        #[arg(long)]
        thread: String,
        /// Reply body in markdown
        #[arg(long)]
        body: String,
        /// Author name
        #[arg(long, default_value = "cli")]
        author: String,
        /// Author type: user or bot
        #[arg(long = "author-type", default_value = "bot")]
        author_type: String,
    },
    /// Resolve or reopen a comment thread
    Resolve {
        /// Thread ID (supports 8+ char prefix)
        #[arg(long)]
        thread: String,
        /// Reopen instead of resolving
        #[arg(long)]
        reopen: bool,
        /// Summary message to attach
        #[arg(long)]
        summary: Option<String>,
    },
    /// List comment threads
    List {
        /// Filter by status: open, resolved, all
        #[arg(long, default_value = "all")]
        status: String,
        /// Git ref for session
        #[arg(long = "ref")]
        git_ref: Option<String>,
        /// Filter to threads on a specific file
        #[arg(long)]
        file: Option<String>,
    },
    /// Get a single thread with all replies
    Get {
        /// Thread ID (supports 8+ char prefix)
        #[arg(long)]
        thread: String,
    },
    /// Delete a thread or individual comment
    Delete {
        /// Thread ID to delete
        #[arg(long, conflicts_with = "comment")]
        thread: Option<String>,
        /// Comment ID to delete
        #[arg(long, conflicts_with = "thread")]
        comment: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        match command {
            Commands::Tree {
                port,
                no_open,
                dark,
                quiet,
                force_new,
            } => run_tree(port, no_open, dark, quiet, force_new),
            Commands::List { json } => run_list(json),
            Commands::Kill => run_kill(),
            Commands::Prune => run_prune(),
            Commands::Doctor => run_doctor(),
            Commands::Review { git_ref, agent, prompt } => run_review(git_ref, agent, prompt),
            Commands::Export { git_ref, status } => run_export(git_ref, status),
            Commands::Comment { action } => run_comment(action),
            Commands::Import { git_ref, author, author_type, file } => {
                run_import(git_ref, author, author_type, file)
            }
        }
    } else {
        run_diff(cli);
    }
}

fn run_diff(cli: Cli) {
    if !repo::is_git_repo() {
        eprintln!("{}", "Error: Not a git repository".red());
        process::exit(1);
    }

    let mut refs = cli.refs;

    if cli.base.is_some() || cli.compare.is_some() {
        if !refs.is_empty() {
            eprintln!("{}", "Error: Cannot use --base/--compare with positional ref arguments.".red());
            process::exit(1);
        }
        if cli.compare.is_some() && cli.base.is_none() {
            eprintln!("{}", "Error: --compare requires --base.".red());
            process::exit(1);
        }
        if let Some(base) = cli.base {
            refs.push(base);
        }
        if let Some(compare) = cli.compare {
            refs.push(compare);
        }
    }

    for r in refs.iter_mut() {
        if r == "." {
            *r = "work".to_string();
        }
    }

    for r in &refs {
        if diff::WORKING_TREE_REFS.contains(&r.as_str()) {
            continue;
        }
        if !repo::is_valid_git_ref(r) {
            eprintln!("{}", format!("Error: '{}' is not a valid git reference.", r).red());
            process::exit(1);
        }
    }

    let mut diff_args: Vec<String> = Vec::new();
    let description;
    let effective_ref;

    match refs.len() {
        0 => {
            description = "Unstaged changes".to_string();
            effective_ref = "work".to_string();
        }
        1 => {
            let r = &refs[0];
            if diff::WORKING_TREE_REFS.contains(&r.as_str()) {
                description = format!(
                    "{} changes",
                    r.chars().next().unwrap().to_uppercase().to_string() + &r[1..]
                );
                effective_ref = r.clone();
            } else {
                if let Ok(normalized) = diff::normalize_ref(r) {
                    diff_args.push(normalized);
                } else {
                    diff_args.push(r.clone());
                }
                description = if r.contains("..") {
                    r.clone()
                } else {
                    format!("Changes from {}", r)
                };
                effective_ref = r.clone();
            }
        }
        _ => {
            let combined = format!("{}..{}", refs[0], refs[1]);
            if let Ok(normalized) = diff::normalize_ref(&combined) {
                diff_args.push(normalized);
            } else {
                diff_args.push(combined.clone());
            }
            description = combined.clone();
            effective_ref = combined;
        }
    }

    let repo_root = repo::get_repo_root().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error: {}", e).red());
        process::exit(1);
    });

    if let Ok(Some(existing)) = registry::find_instance(&repo_root, &effective_ref) {
        if !cli.force_new {
            let url = build_url(existing.port, &effective_ref, cli.dark, cli.unified);
            if !cli.quiet {
                println!();
                println!("  {}", "rsdiffy".bold());
                println!("  {}", "Already running for this repo".dimmed());
                println!();
                println!("  {} {}", "→".green(), url.cyan());
                println!();
            }
            if !cli.no_open {
                let _ = open::that(&url);
            }
            return;
        }
        let _ = registry::kill_instance(existing.pid);
        if !cli.quiet {
            println!("  {}", "Stopped existing instance".dimmed());
        }
    }

    let port = cli.port.unwrap_or_else(|| registry::find_available_port(DEFAULT_PORT));
    let port_is_explicit = cli.port.is_some();

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error creating data directory: {}", e).red());
        process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match server::start_server(ServerOptions {
            port,
            port_is_explicit,
            diff_args,
            description: description.clone(),
            effective_ref: effective_ref.clone(),
            rsdiffy_dir: rsdiffy_dir.clone(),
        })
        .await
        {
            Ok(handle) => {
                let actual_port = handle.port;

                let _ = registry::register_instance(actual_port, &repo_root, &effective_ref);

                let url = build_url(actual_port, &effective_ref, cli.dark, cli.unified);

                if !cli.quiet {
                    println!();
                    println!("  {}", "rsdiffy".bold());
                    println!("  {}", description.dimmed());
                    println!();
                    println!("  {} {}", "→".green(), url.cyan());
                    println!("  {}", "Press Ctrl+C to stop".dimmed());
                    println!();
                }

                if !cli.no_open {
                    let _ = open::that(&url);
                }

                tokio::signal::ctrl_c().await.ok();

                if !cli.quiet {
                    println!("\n  {}", "Shutting down...".dimmed());
                }

                let _ = registry::unregister_instance();
                handle.shutdown();
            }
            Err(e) => {
                eprintln!("{}", format!("Failed to start server: {}", e).red());
                process::exit(1);
            }
        }
    });
}

fn run_tree(port: Option<u16>, no_open: bool, dark: bool, quiet: bool, force_new: bool) {
    if !repo::is_git_repo() {
        eprintln!("{}", "Error: Not a git repository".red());
        process::exit(1);
    }

    let repo_root = repo::get_repo_root().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error: {}", e).red());
        process::exit(1);
    });

    if let Ok(Some(existing)) = registry::find_instance(&repo_root, "__tree__") {
        if !force_new {
            let mut url = format!("http://localhost:{}/tree", existing.port);
            if dark {
                url.push_str("?theme=dark");
            }
            if !quiet {
                println!();
                println!("  {}", "rsdiffy tree".bold());
                println!("  {}", "Reusing running instance".dimmed());
                println!();
                println!("  {} {}", "→".green(), url.cyan());
                println!();
            }
            if !no_open {
                let _ = open::that(&url);
            }
            return;
        }
        let _ = registry::kill_instance(existing.pid);
    }

    let port = port.unwrap_or_else(|| registry::find_available_port(DEFAULT_PORT));
    let port_is_explicit = port != registry::find_available_port(DEFAULT_PORT);

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error creating data directory: {}", e).red());
        process::exit(1);
    });

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match server::start_server(ServerOptions {
            port,
            port_is_explicit,
            diff_args: vec![],
            description: "Repository file browser".to_string(),
            effective_ref: "__tree__".to_string(),
            rsdiffy_dir,
        })
        .await
        {
            Ok(handle) => {
                let actual_port = handle.port;
                let _ = registry::register_instance(actual_port, &repo_root, "__tree__");

                let mut url = format!("http://localhost:{}/tree", actual_port);
                if dark {
                    url.push_str("?theme=dark");
                }

                if !quiet {
                    println!();
                    println!("  {}", "rsdiffy tree".bold());
                    println!("  {}", "Repository file browser".dimmed());
                    println!();
                    println!("  {} {}", "→".green(), url.cyan());
                    println!("  {}", "Press Ctrl+C to stop".dimmed());
                    println!();
                }

                if !no_open {
                    let _ = open::that(&url);
                }

                tokio::signal::ctrl_c().await.ok();

                if !quiet {
                    println!("\n  {}", "Shutting down...".dimmed());
                }

                let _ = registry::unregister_instance();
                handle.shutdown();
            }
            Err(e) => {
                eprintln!("{}", format!("Failed to start server: {}", e).red());
                process::exit(1);
            }
        }
    });
}

fn run_list(json: bool) {
    let instances = registry::list_instances().unwrap_or_default();

    if json {
        println!("{}", serde_json::to_string_pretty(&instances).unwrap());
        return;
    }

    if instances.is_empty() {
        println!("{}", "No running rsdiffy instances.".dimmed());
        return;
    }

    println!();
    println!(
        "  {}   {}{}{}{}",
        "PORT".dimmed(),
        "PID".dimmed(),
        pad("", 5),
        pad("REPO", 22).dimmed(),
        "STARTED".dimmed(),
    );

    for entry in &instances {
        println!(
            "  {}   {}{}{}",
            pad(&entry.port.to_string(), 7),
            pad(&entry.pid.to_string(), 8),
            pad(&truncate(&entry.repo_root, 20), 22),
            entry.started_at.dimmed(),
        );
    }
    println!();
}

fn run_kill() {
    let instances = registry::list_instances().unwrap_or_default();

    if instances.is_empty() {
        println!("{}", "No running rsdiffy instances.".dimmed());
        return;
    }

    let count = instances.len();
    for entry in instances {
        let _ = registry::kill_instance(entry.pid);
    }

    println!(
        "{}",
        format!(
            "Stopped {} instance{}.",
            count,
            if count > 1 { "s" } else { "" }
        )
        .green()
    );
}

fn run_prune() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{}/.config/rsdiffy", home);

    if !std::path::Path::new(&dir).exists() {
        println!("{}", "Nothing to prune.".dimmed());
        return;
    }

    let running = registry::list_instances().unwrap_or_default();
    for entry in &running {
        let _ = registry::kill_instance(entry.pid);
    }
    if !running.is_empty() {
        println!(
            "  {}",
            format!(
                "Stopped {} running instance{}.",
                running.len(),
                if running.len() > 1 { "s" } else { "" }
            )
            .dimmed()
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
    println!("{}", "Pruned all rsdiffy data (~/.config/rsdiffy).".green());
}

fn run_doctor() {
    let mut ok = true;

    print!("  git          ");
    match std::process::Command::new("git")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("{}", format!("✓ {}", version).green());
        }
        _ => {
            println!("{}", "✗ git not found".red());
            ok = false;
        }
    }

    print!("  git repo     ");
    if repo::is_git_repo() {
        println!("{}", "✓ inside a git repository".green());
    } else {
        println!("{}", "- not inside a git repository".yellow());
    }

    print!("  sqlite       ");
    println!("{}", "✓ bundled rusqlite".green());

    print!("  version      ");
    println!("{}", format!("✓ rsdiffy {}", VERSION).green());

    println!();
    if ok {
        println!("{}", "  All checks passed.".green());
    } else {
        println!("{}", "  Some checks failed.".red());
        process::exit(1);
    }
}

fn run_review(git_ref: Option<String>, agent: String, custom_prompt: Option<String>) {
    if !repo::is_git_repo() {
        eprintln!("{}", "Error: Not a git repository".red());
        process::exit(1);
    }

    let effective_ref = git_ref.unwrap_or_else(|| "work".to_string());

    for r in effective_ref.split("..") {
        if !diff::WORKING_TREE_REFS.contains(&r) && !repo::is_valid_git_ref(r) {
            eprintln!("{}", format!("Error: '{}' is not a valid git reference.", r).red());
            process::exit(1);
        }
    }

    let raw_diff = diff::resolve_ref(&effective_ref, &[]).unwrap_or_else(|e| {
        eprintln!("{}", format!("Error getting diff: {}", e).red());
        process::exit(1);
    });

    if raw_diff.trim().is_empty() {
        println!("{}", "No changes to review.".dimmed());
        return;
    }

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error: {}", e).red());
        process::exit(1);
    });

    let head_hash = repo::get_head_hash().unwrap_or_default();
    let session = session::find_or_create_session(&rsdiffy_dir, &effective_ref, &head_hash)
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error creating session: {}", e).red());
            process::exit(1);
        });

    let system_prompt = build_review_prompt(&raw_diff, custom_prompt.as_deref());

    if agent::resolve_agent_command(&agent).is_none() {
        eprintln!(
            "{}",
            format!("Unsupported agent '{}'. Supported agents: claude, codex", agent).red()
        );
        process::exit(1);
    }

    println!("  {} Reviewing with {}...", "●".cyan(), agent.bold());

    let output = agent::invoke_agent(&agent, &system_prompt).unwrap_or_else(|e| {
        eprintln!("{}", e.red());
        process::exit(1);
    });

    let comments = parse_agent_comments(&output);

    if comments.is_empty() {
        println!("  {} Agent returned no comments.", "✓".green());
        return;
    }

    let conn = db::open_db(&rsdiffy_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Error opening database: {}", e).red());
        process::exit(1);
    });

    let author_name = &agent;

    let mut count = 0;
    for c in &comments {
        if threads::create_thread(
            &conn,
            &session.id,
            &c.file_path,
            &c.side,
            c.start_line,
            c.end_line,
            &c.body,
            author_name,
            "bot",
            None,
        )
        .is_ok()
        {
            count += 1;
        }
    }

    println!(
        "  {} {} comment{} added to session.",
        "✓".green(),
        count,
        if count == 1 { "" } else { "s" }
    );
    println!(
        "  {}",
        format!("Run `rsdiffy {}` to view them.", effective_ref).dimmed()
    );
}

fn build_review_prompt(diff: &str, custom_prompt: Option<&str>) -> String {
    let preamble = custom_prompt.unwrap_or(
        "You are a senior code reviewer. Review the following diff carefully.",
    );

    format!(
        r#"{preamble}

For each issue you find, output a JSON comment object. Return ONLY a JSON array — no markdown fences, no explanation outside the array.

Each object must have these fields:
- "filePath": the file path as shown in the diff header (e.g. "src/main.rs")
- "startLine": the line number in the NEW file where the comment applies
- "endLine": same as startLine for single-line comments, or the last line for multi-line
- "side": always "new"
- "body": your review comment in markdown

Example output:
[
  {{
    "filePath": "src/lib.rs",
    "startLine": 42,
    "endLine": 42,
    "side": "new",
    "body": "This unwrap() will panic if the input is None. Consider using `ok_or()` to return a meaningful error."
  }}
]

If there are no issues, return an empty array: []

Here is the diff to review:

{diff}
"#
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentComment {
    file_path: String,
    start_line: i64,
    end_line: i64,
    side: String,
    body: String,
}

fn parse_agent_comments(output: &str) -> Vec<AgentComment> {
    let trimmed = output.trim();

    // Try parsing directly
    if let Ok(comments) = serde_json::from_str::<Vec<AgentComment>>(trimmed) {
        return comments;
    }

    // Try extracting JSON array from markdown fences or surrounding text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            let slice = &trimmed[start..=end];
            if let Ok(comments) = serde_json::from_str::<Vec<AgentComment>>(slice) {
                return comments;
            }
        }
    }

    eprintln!(
        "{}",
        "Warning: Could not parse agent output as JSON comments.".yellow()
    );
    Vec::new()
}

fn run_export(git_ref: Option<String>, status: String) {
    if !repo::is_git_repo() {
        eprintln!("{}", "Error: Not a git repository".red());
        process::exit(1);
    }

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", format!("Error: {}", e).red());
        process::exit(1);
    });

    let conn = db::open_db(&rsdiffy_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Error opening database: {}", e).red());
        process::exit(1);
    });

    let session_id = match &git_ref {
        Some(r) => {
            let head_hash = repo::get_head_hash().unwrap_or_default();
            match session::find_or_create_session(&rsdiffy_dir, r, &head_hash) {
                Ok(s) => s.id,
                Err(e) => {
                    eprintln!("{}", format!("Error finding session for ref '{}': {}", r, e).red());
                    process::exit(1);
                }
            }
        }
        None => match session::get_current_session(&rsdiffy_dir) {
            Ok(Some(s)) => s.id,
            _ => {
                eprintln!("{}", "No current session. Run rsdiffy first or pass --git-ref.".red());
                process::exit(1);
            }
        },
    };

    let status_filter = match status.as_str() {
        "all" => None,
        s => Some(s),
    };

    let thread_list = threads::get_threads_for_session(&conn, &session_id, status_filter)
        .unwrap_or_else(|e| {
            eprintln!("{}", format!("Error reading threads: {}", e).red());
            process::exit(1);
        });

    let tour_list = tours::get_tours_for_session(&conn, &session_id).unwrap_or_default();

    let repo_root = repo::get_repo_root().unwrap_or_default();

    let export = ExportPayload {
        version: 1,
        repo_root,
        git_ref: git_ref.unwrap_or_else(|| {
            session::get_current_session(&rsdiffy_dir)
                .ok()
                .flatten()
                .map(|s| s.git_ref)
                .unwrap_or_default()
        }),
        session_id,
        comments: thread_list
            .into_iter()
            .map(|t| {
                let location = if t.start_line == t.end_line {
                    format!("{}:{}", t.file_path, t.start_line)
                } else {
                    format!("{}:{}-{}", t.file_path, t.start_line, t.end_line)
                };
                ExportComment {
                    file_path: t.file_path,
                    start_line: t.start_line,
                    end_line: t.end_line,
                    location,
                    side: t.side,
                    status: t.status,
                    anchor_content: t.anchor_content,
                    messages: t
                        .comments
                        .into_iter()
                        .map(|c| ExportMessage {
                            author: c.author.name,
                            body: c.body,
                            created_at: c.created_at,
                        })
                        .collect(),
                }
            })
            .collect(),
        tours: tour_list
            .into_iter()
            .map(|t| ExportTour {
                topic: t.topic,
                body: t.body,
                status: t.status,
                steps: t
                    .steps
                    .into_iter()
                    .map(|s| ExportTourStep {
                        file_path: s.file_path.clone(),
                        start_line: s.start_line,
                        end_line: s.end_line,
                        location: if s.start_line == s.end_line {
                            format!("{}:{}", s.file_path, s.start_line)
                        } else {
                            format!("{}:{}-{}", s.file_path, s.start_line, s.end_line)
                        },
                        body: s.body,
                        annotation: s.annotation,
                    })
                    .collect(),
            })
            .collect(),
    };

    println!("{}", serde_json::to_string_pretty(&export).unwrap());
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportPayload {
    version: u32,
    repo_root: String,
    git_ref: String,
    session_id: String,
    comments: Vec<ExportComment>,
    tours: Vec<ExportTour>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportComment {
    file_path: String,
    start_line: i64,
    end_line: i64,
    location: String,
    side: String,
    status: String,
    anchor_content: Option<String>,
    messages: Vec<ExportMessage>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportMessage {
    author: String,
    body: String,
    created_at: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportTour {
    topic: String,
    body: String,
    status: String,
    steps: Vec<ExportTourStep>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportTourStep {
    file_path: String,
    start_line: i64,
    end_line: i64,
    location: String,
    body: String,
    annotation: Option<String>,
}

fn build_url(port: u16, effective_ref: &str, dark: bool, unified: bool) -> String {
    let mut params = Vec::new();
    params.push(format!("ref={}", effective_ref));
    if dark {
        params.push("theme=dark".to_string());
    }
    if unified {
        params.push("view=unified".to_string());
    }
    format!("http://localhost:{}/diff?{}", port, params.join("&"))
}

fn pad(s: &str, width: usize) -> String {
    format!("{:width$}", s, width = width)
}

// ---------------------------------------------------------------------------
// Shared helper: resolve session + open DB
// ---------------------------------------------------------------------------

struct SessionContext {
    session: session::Session,
    conn: Connection,
}

fn resolve_session_and_db(git_ref: Option<&str>) -> SessionContext {
    if !repo::is_git_repo() {
        eprintln!("{}", serde_json::json!({"error": "Not a git repository"}));
        process::exit(1);
    }

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": e.to_string()}));
        process::exit(1);
    });

    let conn = db::open_db(&rsdiffy_dir).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Database error: {}", e)}));
        process::exit(1);
    });

    let session = match git_ref {
        Some(r) => {
            let head_hash = repo::get_head_hash().unwrap_or_default();
            session::find_or_create_session(&rsdiffy_dir, r, &head_hash).unwrap_or_else(|e| {
                eprintln!("{}", serde_json::json!({"error": format!("Session error for ref '{}': {}", r, e)}));
                process::exit(1);
            })
        }
        None => match session::get_current_session(&rsdiffy_dir) {
            Ok(Some(s)) => s,
            _ => {
                let head_hash = repo::get_head_hash().unwrap_or_default();
                session::find_or_create_session(&rsdiffy_dir, "work", &head_hash).unwrap_or_else(|e| {
                    eprintln!("{}", serde_json::json!({"error": format!("Session error: {}", e)}));
                    process::exit(1);
                })
            }
        },
    };

    SessionContext { session, conn }
}

/// Open DB without requiring a session (for thread-specific operations).
fn open_db_or_exit() -> Connection {
    if !repo::is_git_repo() {
        eprintln!("{}", serde_json::json!({"error": "Not a git repository"}));
        process::exit(1);
    }

    let rsdiffy_dir = repo::get_rsdiffy_dir().unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": e.to_string()}));
        process::exit(1);
    });

    db::open_db(&rsdiffy_dir).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Database error: {}", e)}));
        process::exit(1);
    })
}

// ---------------------------------------------------------------------------
// rsdiffy comment <action>
// ---------------------------------------------------------------------------

fn run_comment(action: CommentAction) {
    match action {
        CommentAction::Add {
            file,
            line,
            end_line,
            body,
            author,
            author_type,
            side,
            git_ref,
        } => run_comment_add(file, line, end_line, body, author, author_type, side, git_ref),
        CommentAction::Reply {
            thread,
            body,
            author,
            author_type,
        } => run_comment_reply(thread, body, author, author_type),
        CommentAction::Resolve {
            thread,
            reopen,
            summary,
        } => run_comment_resolve(thread, reopen, summary),
        CommentAction::List {
            status,
            git_ref,
            file,
        } => run_comment_list(status, git_ref, file),
        CommentAction::Get { thread } => run_comment_get(thread),
        CommentAction::Delete { thread, comment } => run_comment_delete(thread, comment),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_comment_add(
    file: String,
    line: i64,
    end_line: Option<i64>,
    body: String,
    author: String,
    author_type: String,
    side: String,
    git_ref: Option<String>,
) {
    let ctx = resolve_session_and_db(git_ref.as_deref());
    let end_line = end_line.unwrap_or(line);

    let thread = threads::create_thread(
        &ctx.conn,
        &ctx.session.id,
        &file,
        &side,
        line,
        end_line,
        &body,
        &author,
        &author_type,
        None,
    )
    .unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Failed to create thread: {}", e)}));
        process::exit(1);
    });

    println!("{}", serde_json::to_string_pretty(&thread).unwrap());
}

fn run_comment_reply(thread_id: String, body: String, author: String, author_type: String) {
    let conn = open_db_or_exit();

    // Resolve prefix to full thread ID
    let thread = threads::get_thread(&conn, &thread_id).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Thread not found: {}", e)}));
        process::exit(1);
    });

    let comment =
        threads::add_reply(&conn, &thread.id, &body, &author, &author_type).unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Failed to add reply: {}", e)}));
            process::exit(1);
        });

    println!("{}", serde_json::to_string_pretty(&comment).unwrap());
}

fn run_comment_resolve(thread_id: String, reopen: bool, summary: Option<String>) {
    let conn = open_db_or_exit();

    let thread = threads::get_thread(&conn, &thread_id).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Thread not found: {}", e)}));
        process::exit(1);
    });

    let status = if reopen { "open" } else { "resolved" };

    let (summary_body, summary_name, summary_type) = if let Some(ref s) = summary {
        (Some(s.as_str()), Some("system"), Some("bot"))
    } else {
        (None, None, None)
    };

    threads::update_thread_status(&conn, &thread.id, status, summary_body, summary_name, summary_type)
        .unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Failed to update status: {}", e)}));
            process::exit(1);
        });

    println!(
        "{}",
        serde_json::json!({
            "threadId": thread.id,
            "status": status,
        })
    );
}

fn run_comment_list(status: String, git_ref: Option<String>, file_filter: Option<String>) {
    let ctx = resolve_session_and_db(git_ref.as_deref());

    let status_filter = match status.as_str() {
        "all" => None,
        s => Some(s),
    };

    let mut thread_list =
        threads::get_threads_for_session(&ctx.conn, &ctx.session.id, status_filter)
            .unwrap_or_else(|e| {
                eprintln!("{}", serde_json::json!({"error": format!("Failed to list threads: {}", e)}));
                process::exit(1);
            });

    if let Some(ref f) = file_filter {
        thread_list.retain(|t| t.file_path == *f);
    }

    println!("{}", serde_json::to_string_pretty(&thread_list).unwrap());
}

fn run_comment_get(thread_id: String) {
    let conn = open_db_or_exit();

    let thread = threads::get_thread(&conn, &thread_id).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Thread not found: {}", e)}));
        process::exit(1);
    });

    println!("{}", serde_json::to_string_pretty(&thread).unwrap());
}

fn run_comment_delete(thread_id: Option<String>, comment_id: Option<String>) {
    let conn = open_db_or_exit();

    if let Some(tid) = thread_id {
        let thread = threads::get_thread(&conn, &tid).unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Thread not found: {}", e)}));
            process::exit(1);
        });

        threads::delete_thread(&conn, &thread.id).unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Failed to delete thread: {}", e)}));
            process::exit(1);
        });

        println!(
            "{}",
            serde_json::json!({"ok": true, "deleted": "thread", "id": thread.id})
        );
    } else if let Some(cid) = comment_id {
        threads::delete_comment(&conn, &cid).unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Failed to delete comment: {}", e)}));
            process::exit(1);
        });

        println!(
            "{}",
            serde_json::json!({"ok": true, "deleted": "comment", "id": cid})
        );
    } else {
        eprintln!("{}", serde_json::json!({"error": "Specify --thread or --comment"}));
        process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// rsdiffy import
// ---------------------------------------------------------------------------

fn run_import(
    git_ref: Option<String>,
    author: String,
    author_type: String,
    file: Option<String>,
) {
    let ctx = resolve_session_and_db(git_ref.as_deref());

    let input = match file {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("{}", serde_json::json!({"error": format!("Failed to read '{}': {}", path, e)}));
            process::exit(1);
        }),
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
                eprintln!("{}", serde_json::json!({"error": format!("Failed to read stdin: {}", e)}));
                process::exit(1);
            });
            buf
        }
    };

    let comments: Vec<AgentComment> = serde_json::from_str(input.trim()).unwrap_or_else(|e| {
        eprintln!("{}", serde_json::json!({"error": format!("Invalid JSON: {}", e)}));
        process::exit(1);
    });

    let mut imported = 0;
    let mut failed = 0;
    let mut thread_ids = Vec::new();

    for c in &comments {
        match threads::create_thread(
            &ctx.conn,
            &ctx.session.id,
            &c.file_path,
            &c.side,
            c.start_line,
            c.end_line,
            &c.body,
            &author,
            &author_type,
            None,
        ) {
            Ok(t) => {
                thread_ids.push(t.id);
                imported += 1;
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    println!(
        "{}",
        serde_json::json!({
            "imported": imported,
            "failed": failed,
            "threads": thread_ids,
        })
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_agent_comments ---

    #[test]
    fn parse_valid_json_array() {
        let input = r#"[{"filePath":"src/main.rs","startLine":10,"endLine":10,"side":"new","body":"Bug here"}]"#;
        let comments = parse_agent_comments(input);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].file_path, "src/main.rs");
        assert_eq!(comments[0].start_line, 10);
        assert_eq!(comments[0].body, "Bug here");
    }

    #[test]
    fn parse_empty_array() {
        let comments = parse_agent_comments("[]");
        assert!(comments.is_empty());
    }

    #[test]
    fn parse_json_with_whitespace() {
        let input = "  \n  [] \n  ";
        let comments = parse_agent_comments(input);
        assert!(comments.is_empty());
    }

    #[test]
    fn parse_json_in_markdown_fences() {
        let input = r#"Here are my findings:

```json
[{"filePath":"lib.rs","startLine":5,"endLine":5,"side":"new","body":"Consider error handling"}]
```

That's all."#;
        let comments = parse_agent_comments(input);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].file_path, "lib.rs");
        assert_eq!(comments[0].body, "Consider error handling");
    }

    #[test]
    fn parse_json_with_surrounding_text() {
        let input = r#"I found one issue:
[{"filePath":"a.rs","startLine":1,"endLine":2,"side":"new","body":"fix this"}]
Hope that helps!"#;
        let comments = parse_agent_comments(input);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].file_path, "a.rs");
    }

    #[test]
    fn parse_multiple_comments() {
        let input = r#"[
            {"filePath":"a.rs","startLine":1,"endLine":1,"side":"new","body":"first"},
            {"filePath":"b.rs","startLine":10,"endLine":15,"side":"new","body":"second"}
        ]"#;
        let comments = parse_agent_comments(input);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "first");
        assert_eq!(comments[1].body, "second");
        assert_eq!(comments[1].start_line, 10);
        assert_eq!(comments[1].end_line, 15);
    }

    #[test]
    fn parse_invalid_json_returns_empty() {
        let comments = parse_agent_comments("this is not json at all");
        assert!(comments.is_empty());
    }

    #[test]
    fn parse_malformed_json_returns_empty() {
        let comments = parse_agent_comments("[{bad json}]");
        assert!(comments.is_empty());
    }

    // --- build_review_prompt ---

    #[test]
    fn review_prompt_contains_diff() {
        let prompt = build_review_prompt("--- a/file.rs\n+++ b/file.rs", None);
        assert!(prompt.contains("--- a/file.rs"));
        assert!(prompt.contains("+++ b/file.rs"));
    }

    #[test]
    fn review_prompt_uses_default_preamble() {
        let prompt = build_review_prompt("diff", None);
        assert!(prompt.contains("senior code reviewer"));
    }

    #[test]
    fn review_prompt_uses_custom_preamble() {
        let prompt = build_review_prompt("diff", Some("Focus on security issues only."));
        assert!(prompt.contains("Focus on security issues only."));
        assert!(!prompt.contains("senior code reviewer"));
    }

    #[test]
    fn review_prompt_requests_json_output() {
        let prompt = build_review_prompt("diff", None);
        assert!(prompt.contains("filePath"));
        assert!(prompt.contains("startLine"));
        assert!(prompt.contains("JSON array"));
    }
}
