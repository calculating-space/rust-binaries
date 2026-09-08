use serde::{Deserialize, Serialize};
use std::process::Command;

/// What the sandbox will run on. Recorded in the manifest and shown to the agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub chip: String,
    pub memory_gb: u64,
}

fn sysctl(key: &str) -> Option<String> {
    let out = Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn host_info() -> HostInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let (chip, memory_gb) = if os == "macos" {
        let chip = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "unknown".into());
        let mem = sysctl("hw.memsize")
            .and_then(|s| s.parse::<u64>().ok())
            .map(|b| b >> 30)
            .unwrap_or(0);
        (chip, mem)
    } else {
        let chip = std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
            })
            .unwrap_or_else(|| "unknown".into());
        let mem = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal"))
                    .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            })
            .map(|kb| kb >> 20)
            .unwrap_or(0);
        (chip, mem)
    };
    HostInfo {
        os,
        arch,
        chip,
        memory_gb,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub host: HostInfo,
    pub tools: Vec<ToolStatus>,
    pub notes: Vec<String>,
}

fn version_of(bin: &str, args: &[&str]) -> ToolStatus {
    match Command::new(bin).args(args).output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let text = if text.trim().is_empty() {
                String::from_utf8_lossy(&out.stderr)
            } else {
                text
            };
            ToolStatus {
                name: bin.into(),
                available: true,
                detail: text.lines().next().unwrap_or("").trim().to_string(),
            }
        }
        Ok(out) => ToolStatus {
            name: bin.into(),
            available: false,
            detail: format!("exit {}", out.status),
        },
        Err(e) => ToolStatus {
            name: bin.into(),
            available: false,
            detail: e.to_string(),
        },
    }
}

/// Which backends and the agent CLI are usable right now.
pub fn doctor() -> Report {
    let host = host_info();
    let mut tools = vec![
        version_of("uv", &["--version"]),
        version_of("python3", &["--version"]),
        version_of("docker", &["--version"]),
        version_of("claude", &["--version"]),
    ];
    let daemon = Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output();
    let daemon_up = matches!(&daemon, Ok(o) if o.status.success());
    tools.push(ToolStatus {
        name: "docker-daemon".into(),
        available: daemon_up,
        detail: if daemon_up {
            "running".into()
        } else {
            "not running (start Docker Desktop to use --backend docker)".into()
        },
    });
    let mut notes = Vec::new();
    if host.os == "macos" {
        notes.push("docker on macOS runs a Linux VM with no Metal GPU access; use uv or venv to measure Apple Silicon performance".into());
    }
    Report { host, tools, notes }
}
