//! `oneshot`: hand a prompt to an agent and get its result back.
//!
//! The agent is any command that reads a prompt on stdin and answers on
//! stdout. By default that is `claude -p --output-format json`, Claude Code in
//! headless mode, whose single JSON object carries the result text, whether it
//! failed, the session id, cost and turn count. `oneshot` runs the command with
//! an optional working directory and wall-clock timeout, then folds everything
//! it learned into one [`Record`]. The CLI prints the result text or the record.
//!
//! Nothing here talks to a network or a model itself; the agent does.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Version tag written into every record.
pub const RECORD_VERSION: &str = "oneshot/1";

/// Exit status used when the agent ran out of time, as `timeout(1)` does.
pub const EXIT_TIMEOUT: i32 = 124;

/// The agent run when none is given: Claude Code, headless, one JSON result.
pub fn default_agent() -> Vec<String> {
    ["claude", "-p", "--output-format", "json"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// How to read what the agent printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parse {
    /// One JSON object in the shape `claude -p --output-format json` prints.
    Json,
    /// Stdout verbatim is the result.
    Text,
}

/// One oneshot: a prompt and the agent to hand it to.
#[derive(Debug, Clone)]
pub struct Job {
    pub prompt: String,
    /// Program and arguments. The prompt goes to the program's stdin.
    pub agent: Vec<String>,
    pub parse: Parse,
    /// Working directory for the agent; `None` means inherit.
    pub cwd: Option<PathBuf>,
    /// Kill the agent (and its process group) after this long.
    pub timeout: Option<Duration>,
}

impl Job {
    pub fn new(prompt: impl Into<String>) -> Self {
        Job {
            prompt: prompt.into(),
            agent: default_agent(),
            parse: Parse::Json,
            cwd: None,
            timeout: None,
        }
    }
}

/// Everything known about one run. Serialised as the `--json` / `--record` output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub version: String,
    pub prompt: String,
    pub agent: Vec<String>,
    pub cwd: String,
    pub started_unix: u64,
    pub duration_ms: u64,
    /// The agent's exit code; `None` when it died from a signal (including our timeout).
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Exit 0, not timed out, and (for JSON agents) a parsable object without `is_error`.
    pub ok: bool,
    /// The agent's answer. For JSON agents the `result` field; otherwise stdout.
    pub result: String,
    pub stderr: String,
    /// Why `ok` is false, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_turns: Option<u64>,
    /// The agent's whole JSON object, untouched, for fields not lifted above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_output: Option<Value>,
}

impl Record {
    /// Exit code for the CLI: 0 ok, 124 timeout, the agent's own code (or 1) otherwise.
    pub fn exit_code(&self) -> i32 {
        if self.ok {
            0
        } else if self.timed_out {
            EXIT_TIMEOUT
        } else {
            match self.exit_code {
                Some(0) | None => 1,
                Some(c) => c,
            }
        }
    }
}

/// Fields lifted out of a `claude -p --output-format json` object.
#[derive(Debug, Default, PartialEq)]
pub struct AgentJson {
    pub result: String,
    pub is_error: bool,
    pub session_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub num_turns: Option<u64>,
    pub raw: Value,
}

/// Parse the agent's stdout as one JSON object. Leading and trailing whitespace
/// is fine; anything else that is not the object is an error.
pub fn parse_agent_json(stdout: &str) -> Result<AgentJson, String> {
    let raw: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("agent output is not a JSON object: {e}"))?;
    let obj = raw
        .as_object()
        .ok_or_else(|| "agent output is JSON but not an object".to_string())?;
    let result = match obj.get("result") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    Ok(AgentJson {
        result,
        is_error: obj.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        session_id: obj.get("session_id").and_then(Value::as_str).map(String::from),
        cost_usd: obj.get("total_cost_usd").and_then(Value::as_f64),
        num_turns: obj.get("num_turns").and_then(Value::as_u64),
        raw,
    })
}

/// Errors before the agent produced anything: the program could not be started.
#[derive(Debug)]
pub struct SpawnError(pub String);

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SpawnError {}

fn kill_group(pid: u32) {
    // SAFETY: plain syscall on a pid we spawned into its own process group.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    thread::sleep(Duration::from_millis(200));
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

/// Run the job to completion (or timeout) and describe what happened.
pub fn run(job: &Job) -> Result<Record, SpawnError> {
    let (program, args) = job
        .agent
        .split_first()
        .ok_or_else(|| SpawnError("agent command is empty".into()))?;
    let cwd = match &job.cwd {
        Some(d) => d.clone(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let started_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let start = Instant::now();
    let mut child = cmd.spawn().map_err(|e| {
        SpawnError(format!("cannot run {program}: {e} (cwd {})", cwd.display()))
    })?;

    let mut stdin = child.stdin.take().expect("piped");
    let prompt = job.prompt.clone();
    let feeder = thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
        // dropping closes the pipe so the agent sees end of input
    });
    let mut out_pipe = child.stdout.take().expect("piped");
    let mut err_pipe = child.stderr.take().expect("piped");
    let out_reader = thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out_pipe.read_to_end(&mut v);
        v
    });
    let err_reader = thread::spawn(move || {
        let mut v = Vec::new();
        let _ = err_pipe.read_to_end(&mut v);
        v
    });

    let mut timed_out = false;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| SpawnError(format!("wait failed: {e}")))? {
            break s;
        }
        if let Some(limit) = job.timeout
            && start.elapsed() >= limit
        {
            timed_out = true;
            kill_group(child.id());
            break child
                .wait()
                .map_err(|e| SpawnError(format!("wait after kill failed: {e}")))?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let _ = feeder.join();
    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    let duration_ms = start.elapsed().as_millis() as u64;

    let exit_code = status.code();
    let mut record = Record {
        version: RECORD_VERSION.into(),
        prompt: job.prompt.clone(),
        agent: job.agent.clone(),
        cwd: cwd.display().to_string(),
        started_unix,
        duration_ms,
        exit_code,
        timed_out,
        ok: false,
        result: stdout.clone(),
        stderr,
        error: None,
        session_id: None,
        cost_usd: None,
        num_turns: None,
        agent_output: None,
    };

    if timed_out {
        record.error = Some(format!(
            "agent killed after {} s",
            job.timeout.map(|t| t.as_secs()).unwrap_or(0)
        ));
        return Ok(record);
    }

    match job.parse {
        Parse::Text => {
            record.ok = status.success();
        }
        Parse::Json => match parse_agent_json(&stdout) {
            Ok(a) => {
                record.result = a.result;
                record.session_id = a.session_id;
                record.cost_usd = a.cost_usd;
                record.num_turns = a.num_turns;
                record.agent_output = Some(a.raw);
                if a.is_error {
                    record.error = Some("agent reported is_error".into());
                }
                record.ok = status.success() && !a.is_error;
            }
            Err(e) => {
                record.error = Some(e);
            }
        },
    }
    if !record.ok && record.error.is_none() {
        record.error = Some(match (exit_code, status.signal()) {
            (Some(c), _) => format!("agent exited with status {c}"),
            (None, Some(sig)) => format!("agent killed by signal {sig}"),
            (None, None) => "agent ended abnormally".into(),
        });
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_shape() {
        let s = r#"{"type":"result","subtype":"success","is_error":false,"result":"pong","session_id":"s","total_cost_usd":0.02,"num_turns":1,"usage":{"input_tokens":9}}"#;
        let a = parse_agent_json(s).unwrap();
        assert_eq!(a.result, "pong");
        assert!(!a.is_error);
        assert_eq!(a.session_id.as_deref(), Some("s"));
        assert_eq!(a.cost_usd, Some(0.02));
        assert_eq!(a.num_turns, Some(1));
        assert_eq!(a.raw["usage"]["input_tokens"], 9);
    }

    #[test]
    fn missing_fields_are_none_not_errors() {
        let a = parse_agent_json("  {} \n").unwrap();
        assert_eq!(a, AgentJson { raw: serde_json::json!({}), ..Default::default() });
    }

    #[test]
    fn non_object_is_an_error() {
        assert!(parse_agent_json("not json").is_err());
        assert!(parse_agent_json("[1,2]").is_err());
        assert!(parse_agent_json("").is_err());
    }

    #[test]
    fn exit_code_mapping() {
        let mut r = Record {
            version: RECORD_VERSION.into(),
            prompt: String::new(),
            agent: vec![],
            cwd: String::new(),
            started_unix: 0,
            duration_ms: 0,
            exit_code: Some(0),
            timed_out: false,
            ok: true,
            result: String::new(),
            stderr: String::new(),
            error: None,
            session_id: None,
            cost_usd: None,
            num_turns: None,
            agent_output: None,
        };
        assert_eq!(r.exit_code(), 0);
        r.ok = false;
        assert_eq!(r.exit_code(), 1, "is_error with exit 0 still fails");
        r.exit_code = Some(3);
        assert_eq!(r.exit_code(), 3);
        r.timed_out = true;
        assert_eq!(r.exit_code(), EXIT_TIMEOUT);
    }
}
