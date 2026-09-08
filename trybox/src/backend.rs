use crate::manifest::Manifest;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// uv-managed virtualenv with an isolated uv cache inside the sandbox. Full host GPU access.
    Uv,
    /// Plain `python3 -m venv` plus pip. Slower, no uv needed.
    Venv,
    /// Linux container built from a generated Dockerfile. No GPU on macOS.
    Docker,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Uv => "uv",
            Backend::Venv => "venv",
            Backend::Docker => "docker",
        }
    }
}

/// One shell step of a prepare plan. Kept as data so tests and `--dry-run` can inspect it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreparePlan {
    pub steps: Vec<Step>,
    pub files: Vec<(PathBuf, String)>,
}

fn s(v: &str) -> String {
    v.to_string()
}

/// Hugging Face downloads, kept tidy: no telemetry, no "set a HF_TOKEN" nag, one classic
/// progress bar per file instead of the xet backend's three, and bars that stay 80 columns
/// wide on a wide terminal.
pub const HF_QUIET: &[(&str, &str)] = &[
    ("HF_HUB_DISABLE_TELEMETRY", "1"),
    ("HF_HUB_VERBOSITY", "error"),
    ("HF_HUB_DISABLE_XET", "1"),
    ("TQDM_NCOLS", "80"),
];

fn hf_quiet_exports() -> String {
    HF_QUIET
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Environment that makes the sandbox's interpreter and caches the only ones visible.
pub fn sandbox_env(m: &Manifest) -> Vec<(String, String)> {
    let mut env = vec![(s("HF_HOME"), m.hf_dir().display().to_string())];
    env.extend(HF_QUIET.iter().map(|(k, v)| (s(k), s(v))));
    env.extend([
        (
            s("UV_CACHE_DIR"),
            m.cache_dir().join("uv").display().to_string(),
        ),
        (
            s("PIP_CACHE_DIR"),
            m.cache_dir().join("pip").display().to_string(),
        ),
        (s("TRYBOX_NAME"), m.name.clone()),
        (s("TRYBOX_DIR"), m.dir.display().to_string()),
    ]);
    if m.backend != Backend::Docker {
        let bin = m.venv_dir().join("bin");
        let path = std::env::var("PATH").unwrap_or_default();
        env.push((s("VIRTUAL_ENV"), m.venv_dir().display().to_string()));
        env.push((s("PATH"), format!("{}:{}", bin.display(), path)));
    }
    env
}

fn dockerfile(m: &Manifest) -> String {
    let mut out = format!(
        "FROM python:{}-slim\nENV HF_HOME=/hf PIP_NO_CACHE_DIR=1 {}\nWORKDIR /work\n",
        m.python,
        hf_quiet_exports()
    );
    if !m.packages.is_empty() {
        out.push_str(&format!(
            "RUN pip install --no-cache-dir {}\n",
            m.packages.join(" ")
        ));
    }
    out.push_str("CMD [\"bash\"]\n");
    out
}

/// The `x` wrapper: `./x <cmd...>` runs a command inside the environment for any backend.
fn exec_script(m: &Manifest) -> String {
    match m.backend {
        Backend::Docker => format!(
            "#!/bin/sh\n# run a command inside the {name} container; work/ is /work, hf/ is /hf\nTTY=\"\"; [ -t 0 ] && TTY=\"-t\"\nexec docker run --rm -i $TTY -v \"{work}:/work\" -v \"{hf}:/hf\" {tag} \"$@\"\n",
            name = m.name,
            work = m.work_dir().display(),
            hf = m.hf_dir().display(),
            tag = m.docker_tag()
        ),
        _ => format!(
            "#!/bin/sh\n# run a command inside the {name} sandbox environment\nexport VIRTUAL_ENV=\"{venv}\" HF_HOME=\"{hf}\" {quiet} UV_CACHE_DIR=\"{cache}/uv\" PIP_CACHE_DIR=\"{cache}/pip\"\nexport PATH=\"$VIRTUAL_ENV/bin:$PATH\"\ncd \"{work}\"\nexec \"$@\"\n",
            name = m.name,
            quiet = hf_quiet_exports(),
            venv = m.venv_dir().display(),
            hf = m.hf_dir().display(),
            cache = m.cache_dir().display(),
            work = m.work_dir().display()
        ),
    }
}

/// Everything `create` will do, as data. Nothing is executed here.
pub fn prepare(m: &Manifest) -> PreparePlan {
    let env = sandbox_env(m);
    let mut steps = Vec::new();
    let mut files = vec![(m.dir.join("x"), exec_script(m))];
    match m.backend {
        Backend::Uv => {
            steps.push(Step {
                title: s("create virtualenv with uv"),
                program: s("uv"),
                args: vec![
                    s("venv"),
                    s("--python"),
                    m.python.clone(),
                    m.venv_dir().display().to_string(),
                ],
                cwd: m.dir.clone(),
                env: env.clone(),
            });
            if !m.packages.is_empty() {
                let mut args = vec![
                    s("pip"),
                    s("install"),
                    s("--python"),
                    m.venv_dir().join("bin/python").display().to_string(),
                ];
                args.extend(m.packages.iter().cloned());
                steps.push(Step {
                    title: s("install packages with uv"),
                    program: s("uv"),
                    args,
                    cwd: m.dir.clone(),
                    env: env.clone(),
                });
            }
        }
        Backend::Venv => {
            steps.push(Step {
                title: s("create virtualenv with python3"),
                program: s("python3"),
                args: vec![s("-m"), s("venv"), m.venv_dir().display().to_string()],
                cwd: m.dir.clone(),
                env: env.clone(),
            });
            if !m.packages.is_empty() {
                let mut args = vec![s("-m"), s("pip"), s("install"), s("--quiet")];
                args.extend(m.packages.iter().cloned());
                steps.push(Step {
                    title: s("install packages with pip"),
                    program: m.venv_dir().join("bin/python").display().to_string(),
                    args,
                    cwd: m.dir.clone(),
                    env: env.clone(),
                });
            }
        }
        Backend::Docker => {
            files.push((m.docker_dir().join("Dockerfile"), dockerfile(m)));
            steps.push(Step {
                title: s("build container image"),
                program: s("docker"),
                args: vec![
                    s("build"),
                    s("-t"),
                    m.docker_tag(),
                    m.docker_dir().display().to_string(),
                ],
                cwd: m.dir.clone(),
                env: Vec::new(),
            });
        }
    }
    PreparePlan { steps, files }
}

/// Execute a plan in order, streaming subprocess output to the terminal.
pub fn run_plan(m: &Manifest, plan: &PreparePlan, quiet: bool) -> Result<(), String> {
    for dir in [
        m.dir.clone(),
        m.work_dir(),
        m.hf_dir(),
        m.cache_dir(),
        m.docker_dir(),
    ] {
        std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    for (path, content) in &plan.files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))?;
        if path.file_name().is_some_and(|f| f == "x") {
            set_executable(path)?;
        }
    }
    for (i, step) in plan.steps.iter().enumerate() {
        if !quiet {
            eprintln!("[{}/{}] {}", i + 1, plan.steps.len(), step.title);
        }
        let stdio = || {
            if quiet {
                Stdio::null()
            } else {
                Stdio::inherit()
            }
        };
        let status = Command::new(&step.program)
            .args(&step.args)
            .current_dir(&step.cwd)
            .envs(step.env.iter().map(|(k, v)| (k, v)))
            .stdout(stdio())
            .stderr(stdio())
            .status()
            .map_err(|e| format!("{}: cannot run {}: {e}", step.title, step.program))?;
        if !status.success() {
            return Err(format!("{} failed ({status})", step.title));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// A command that runs `argv` inside the sandbox, ready to spawn or exec.
pub fn exec_command(m: &Manifest, argv: &[String]) -> Command {
    refresh_wrapper(m);
    let mut cmd = Command::new(m.dir.join("x"));
    cmd.args(argv);
    cmd
}

/// Rewrite the `x` wrapper when it differs from what this build would generate, so a sandbox
/// made by an older trybox runs with today's environment. Best effort: a failure here shows
/// up as the old behaviour, not an error.
fn refresh_wrapper(m: &Manifest) {
    let path = m.dir.join("x");
    let want = exec_script(m);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(want.as_str())
        && std::fs::write(&path, &want).is_ok()
    {
        let _ = set_executable(&path);
    }
}

/// Remove the sandbox directory and, for docker, its image. Returns human-readable notes.
pub fn destroy(m: &Manifest) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    if m.backend == Backend::Docker {
        let out = Command::new("docker")
            .args(["rmi", "-f", &m.docker_tag()])
            .output();
        match out {
            Ok(o) if o.status.success() => notes.push(format!("removed image {}", m.docker_tag())),
            Ok(o) => notes.push(format!(
                "image {} not removed: {}",
                m.docker_tag(),
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => notes.push(format!(
                "docker not reachable, image {} left behind: {e}",
                m.docker_tag()
            )),
        }
    }
    let bytes = dir_size(&m.dir);
    std::fs::remove_dir_all(&m.dir).map_err(|e| format!("remove {}: {e}", m.dir.display()))?;
    notes.push(format!(
        "removed {} ({:.1} GB)",
        m.dir.display(),
        bytes as f64 / (1u64 << 30) as f64
    ));
    Ok(notes)
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(md) if md.is_dir() => dir_size(&e.path()),
            Ok(md) if md.is_file() => md.len(),
            _ => 0,
        })
        .sum()
}
