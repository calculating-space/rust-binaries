//! Detect the machine. The only impure part of the crate.

use crate::{CONTRACT_VERSION, Gpu, GpuKind, Probes, Spec};
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
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

fn memory_gb() -> u64 {
    if cfg!(target_os = "macos") {
        return sysctl("hw.memsize")
            .and_then(|s| s.parse::<u64>().ok())
            .map(|b| b >> 30)
            .unwrap_or(0);
    }
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
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
        .and_then(|s| {
            s.lines()
                .nth(1)
                .and_then(|l| l.split_whitespace().nth(3)?.parse::<u64>().ok())
        })
        .map(|kb| kb >> 20)
        .unwrap_or(0)
}

fn gpus(os: &str, arch: &str, chip: &str) -> Vec<Gpu> {
    let mut out = Vec::new();
    if os == "macos" && arch == "aarch64" {
        out.push(Gpu {
            kind: GpuKind::Metal,
            name: format!("{chip} (unified memory)"),
            memory_gb: None,
        });
    }
    if let Some(text) = run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    ) {
        for line in text.lines() {
            let mut parts = line.split(',').map(str::trim);
            let name = parts.next().unwrap_or("NVIDIA").to_string();
            let mb = parts.next().and_then(|m| m.parse::<u64>().ok());
            out.push(Gpu {
                kind: GpuKind::Cuda,
                name,
                memory_gb: mb.map(|m| m / 1024),
            });
        }
    }
    out
}

/// `None` when the tool is not on PATH; otherwise its `--version` line, or `found` when it
/// has no such flag (`say`, `ffmpeg`). Only `--version` is tried: guessing other flags
/// misfires (`say -version` is `say -v ersion`, half a second of voice lookup).
fn tool_version(name: &str) -> Option<String> {
    if !on_path(name) {
        return None;
    }
    let version = Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let text = if text.trim().is_empty() {
                String::from_utf8_lossy(&out.stderr).to_string()
            } else {
                text
            };
            let line = text.lines().next().unwrap_or("").trim().to_string();
            (!line.is_empty()).then_some(line)
        });
    Some(version.unwrap_or_else(|| "found".to_string()))
}

fn on_path(name: &str) -> bool {
    if name.contains('/') {
        return is_executable(Path::new(name));
    }
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| is_executable(&dir.join(name))))
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// Text-to-speech voices as locale codes. macOS `say -v ?`; nothing is detected elsewhere yet.
fn voices() -> Vec<String> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let Some(text) = run("say", &["-v", "?"]) else {
        return Vec::new();
    };
    let is_locale = |t: &str| {
        let mut parts = t.splitn(2, '_');
        matches!(
            (parts.next(), parts.next()),
            (Some(lang), Some(region))
                if !lang.is_empty()
                    && lang.chars().all(|c| c.is_ascii_lowercase())
                    && !region.is_empty()
                    && region.chars().all(|c| c.is_ascii_alphanumeric())
        )
    };
    let mut out: Vec<String> = text
        .lines()
        .filter_map(|l| {
            l.split_whitespace()
                .find(|t| is_locale(t))
                .map(str::to_string)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Audio capture devices by name. macOS: `system_profiler`; Linux: `/proc/asound/pcm`.
fn audio_inputs() -> Vec<String> {
    if cfg!(target_os = "macos") {
        let Some(text) = run("system_profiler", &["SPAudioDataType"]) else {
            return Vec::new();
        };
        // Devices are headers ("Name:") nested under "Devices:"; inputs list "Input Channels".
        let mut out = Vec::new();
        let mut device = String::new();
        for line in text.lines() {
            let body = line.trim();
            let indent = line.len() - line.trim_start().len();
            if indent > 4 && body.ends_with(':') && !body.contains(": ") {
                device = body.trim_end_matches(':').to_string();
            } else if body.starts_with("Input Channels:") && !device.is_empty() {
                out.push(device.clone());
            }
        }
        return out;
    }
    std::fs::read_to_string("/proc/asound/pcm")
        .map(|s| {
            s.lines()
                .filter(|l| l.contains("capture"))
                .filter_map(|l| l.split(':').nth(1))
                .map(|n| n.trim().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Detect this machine. `disk_path` is where installs would go; `probes` says which optional
/// things (tools, voices, microphones) to look for, each costing a subprocess.
pub fn detect(disk_path: &Path, probes: &Probes) -> Spec {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let chip = chip();
    let gpus = gpus(&os, &arch, &chip);
    // Each probe is a subprocess and `say -v ?` alone takes most of a second: run them side by side.
    let (found, audio_inputs, voices) = std::thread::scope(|scope| {
        let tools: Vec<_> = probes
            .tools
            .iter()
            .map(|t| scope.spawn(move || tool_version(t).map(|v| (t.clone(), v))))
            .collect();
        let audio = scope.spawn(|| probes.audio_inputs.then(audio_inputs).unwrap_or_default());
        let voice = scope.spawn(|| probes.voices.then(voices).unwrap_or_default());
        let found: BTreeMap<String, String> = tools
            .into_iter()
            .filter_map(|h| h.join().unwrap_or(None))
            .collect();
        (
            found,
            audio.join().unwrap_or_default(),
            voice.join().unwrap_or_default(),
        )
    });
    Spec {
        version: CONTRACT_VERSION,
        cores: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1),
        memory_gb: memory_gb(),
        disk_free_gb: disk_free_gb(disk_path),
        disk_path: disk_path.display().to_string(),
        os,
        arch,
        chip,
        gpus,
        tools: found,
        audio_inputs,
        voices,
    }
}
