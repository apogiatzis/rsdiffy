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
  $ rsdiffy export --status open         Export only open comments"#
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

    let (cmd, args) = resolve_agent_command(&agent);

    println!("  {} Reviewing with {}...", "●".cyan(), agent.bold());

    let output = std::process::Command::new(&cmd)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(system_prompt.as_bytes());
            }
            child.wait_with_output()
        });

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            eprintln!(
                "{}",
                format!("Agent exited with status {}", o.status).red()
            );
            process::exit(1);
        }
        Err(e) => {
            eprintln!(
                "{}",
                format!(
                    "Failed to run '{}': {}. Is the agent installed and in PATH?",
                    cmd, e
                )
                .red()
            );
            process::exit(1);
        }
    };

    let comments = parse_agent_comments(&output);

    if comments.is_empty() {
        println!("  {} Agent returned no comments.", "✓".green());
        return;
    }

    let conn = db::open_db(&rsdiffy_dir).unwrap_or_else(|e| {
        eprintln!("{}", format!("Error opening database: {}", e).red());
        process::exit(1);
    });

    let author_name = agent_to_author_name(&agent);

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
            &author_name,
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

fn resolve_agent_command(agent: &str) -> (String, Vec<String>) {
    match agent {
        "claude" => ("claude".to_string(), vec!["-p".to_string()]),
        "codex" => ("codex".to_string(), vec!["-q".to_string()]),
        custom => {
            let parts: Vec<&str> = custom.split_whitespace().collect();
            let cmd = parts[0].to_string();
            let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            (cmd, args)
        }
    }
}

fn agent_to_author_name(agent: &str) -> String {
    let base = match agent {
        "claude" | "codex" => agent,
        custom => custom.split_whitespace().next().unwrap_or(custom),
    };
    base.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        s.to_string()
    }
}
