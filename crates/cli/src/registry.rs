use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub pid: u32,
    pub port: u16,
    pub repo_root: String,
    pub git_ref: String,
    pub started_at: String,
}

fn registry_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".rsdiffy").join("registry.json")
}

fn ensure_registry_dir() -> Result<()> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn read_registry() -> Result<Vec<Instance>> {
    let path = registry_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    let instances: Vec<Instance> = serde_json::from_str(&content).unwrap_or_default();
    Ok(instances)
}

fn write_registry(instances: &[Instance]) -> Result<()> {
    ensure_registry_dir()?;
    let content = serde_json::to_string_pretty(instances)?;
    fs::write(registry_path(), content)?;
    Ok(())
}

/// Register a running instance.
pub fn register_instance(port: u16, repo_root: &str, git_ref: &str) -> Result<()> {
    let mut instances = read_registry()?;

    instances.push(Instance {
        pid: std::process::id(),
        port,
        repo_root: repo_root.to_string(),
        git_ref: git_ref.to_string(),
        started_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    });

    write_registry(&instances)
}

/// Unregister the current process.
pub fn unregister_instance() -> Result<()> {
    let pid = std::process::id();
    let mut instances = read_registry()?;
    instances.retain(|i| i.pid != pid);
    write_registry(&instances)
}

/// List all live instances (prunes dead ones).
pub fn list_instances() -> Result<Vec<Instance>> {
    let instances = read_registry()?;
    let sys = System::new_all();

    let alive: Vec<Instance> = instances
        .into_iter()
        .filter(|i| sys.process(sysinfo::Pid::from_u32(i.pid)).is_some())
        .collect();

    write_registry(&alive)?;
    Ok(alive)
}

/// Find a running instance for the given repo + ref.
pub fn find_instance(repo_root: &str, git_ref: &str) -> Result<Option<Instance>> {
    let instances = list_instances()?;
    Ok(instances
        .into_iter()
        .find(|i| i.repo_root == repo_root && i.git_ref == git_ref))
}

/// Kill an instance by PID.
pub fn kill_instance(pid: u32) -> Result<bool> {
    let sys = System::new_all();
    if let Some(process) = sys.process(sysinfo::Pid::from_u32(pid)) {
        process.kill();
        let mut instances = read_registry()?;
        instances.retain(|i| i.pid != pid);
        write_registry(&instances)?;
        Ok(true)
    } else {
        let mut instances = read_registry()?;
        instances.retain(|i| i.pid != pid);
        write_registry(&instances)?;
        Ok(false)
    }
}

/// Find an available port starting from the given port.
pub fn find_available_port(start: u16) -> u16 {
    let instances = read_registry().unwrap_or_default();
    let used_ports: Vec<u16> = instances.iter().map(|i| i.port).collect();

    let mut port = start;
    loop {
        if !used_ports.contains(&port) && port_is_free(port) {
            return port;
        }
        port += 1;
    }
}

fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}
