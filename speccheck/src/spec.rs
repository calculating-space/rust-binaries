//! Detect the machine. The only impure part of the crate.

use crate::{CONTRACT_VERSION, Gpu, GpuKind, Spec};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

fn run(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn sysctl(key: &str) -> Option<String> {
    run("/usr/sbin/sysctl", &["-n", key])
}

fn chip() -> String {
    if cfg!(target_os = "macos") {
        return sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "unknown".into());
    }
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("model name")).map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string()))
        .unwrap_or_else(|| "unknown".into())
}

fn memory_gb() -> u64 {
    if cfg!(target_os = "macos") {
        return sysctl("hw.memsize").and_then(|s| s.parse::<u64>().ok()).map(|b| b >> 30).unwrap_or(0);
    }
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()))
        .map(|kb| kb >> 20)
        .unwrap_or(0)
}

/// Free GB on the filesystem that holds `path` (or its nearest existing ancestor, so a
/// not-yet-created install directory still measures the right disk). Uses `df -k`.
pub fn disk_free_gb(path: &Path) -> u64 {
    let mut probe = path;
    while !probe.exists() {
        match probe.parent() {
            Some(p) if !p.as_os_str().is_empty() => probe = p,
            _ => break,
        }
    }
    run("df", &["-k", &probe.display().to_string()])
        .and_then(|s| s.lines().nth(1).and_then(|l| l.split_whitespace().nth(3)?.parse::<u64>().ok()))
        .map(|kb| kb >> 20)
        .unwrap_or(0)
}

fn gpus(os: &str, arch: &str, chip: &str) -> Vec<Gpu> {
    let mut out = Vec::new();
    if os == "macos" && arch == "aarch64" {
        out.push(Gpu { kind: GpuKind::Metal, name: format!("{chip} (unified memory)"), memory_gb: None });
    }
    if let Some(text) = run("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]) {
        for line in text.lines() {
            let mut parts = line.split(',').map(str::trim);
            let name = parts.next().unwrap_or("NVIDIA").to_string();
            let mb = parts.next().and_then(|m| m.parse::<u64>().ok());
            out.push(Gpu { kind: GpuKind::Cuda, name, memory_gb: mb.map(|m| m / 1024) });
        }
    }
    out
}

fn tool_version(name: &str) -> Option<String> {
    let out = Command::new(name).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let text = if text.trim().is_empty() { String::from_utf8_lossy(&out.stderr).to_string() } else { text.to_string() };
    Some(text.lines().next().unwrap_or("").trim().to_string())
}

/// Detect this machine. `disk_path` is where installs would go; `tools` are probed by `--version`.
pub fn detect(disk_path: &Path, tools: &[String]) -> Spec {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let chip = chip();
    let gpus = gpus(&os, &arch, &chip);
    let mut found = BTreeMap::new();
    for t in tools {
        if let Some(v) = tool_version(t) {
            found.insert(t.clone(), v);
        }
    }
    Spec {
        version: CONTRACT_VERSION,
        cores: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
        memory_gb: memory_gb(),
        disk_free_gb: disk_free_gb(disk_path),
        disk_path: disk_path.display().to_string(),
        os,
        arch,
        chip,
        gpus,
        tools: found,
    }
}
