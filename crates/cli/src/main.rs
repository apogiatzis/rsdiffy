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
  $ rsdiffy prune                        Remove all rsdiffy data"#
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
    let dir = format!("{}/.rsdiffy", home);

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
    println!("{}", "Pruned all rsdiffy data (~/.rsdiffy).".green());
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
