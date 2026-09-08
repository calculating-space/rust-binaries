use clap::{Args, Parser, Subcommand};
use sieve::{Bounds, Folder, Spill, sieve_home};
use std::io::{self, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};

#[derive(Parser)]
#[command(name = "sieve", version, about = "Context governor: keep what enters an agent's context small")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a shell command with bounded output; the full output goes to a spill file
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Print everything when the output is at most this many bytes
    #[arg(long, default_value_t = 16384)]
    cap: usize,
    /// Bytes shown from the start when the output exceeds the cap
    #[arg(long, default_value_t = 8192)]
    head: usize,
    /// Bytes shown from the end when the output exceeds the cap
    #[arg(long, default_value_t = 4096)]
    tail: usize,
    /// Name of the spill file (without .out); default: time and pid
    #[arg(long)]
    id: Option<String>,
    /// Session subdirectory under the spill directory
    #[arg(long)]
    session: Option<String>,
    /// Where spill files go [default: $SIEVE_HOME/spill or ~/.sieve/spill]
    #[arg(long, value_name = "DIR")]
    spill_dir: Option<PathBuf>,
    /// Keep the spill file even when the output fit under the cap
    #[arg(long)]
    keep: bool,
    /// The command, run with `sh -c`; several words are joined with spaces
    #[arg(trailing_var_arg = true, required = true, num_args = 1..)]
    command: Vec<String>,
}

static CHILD: AtomicI32 = AtomicI32::new(0);
/// A signal that arrived before the child existed; delivered right after spawn.
static PENDING: AtomicI32 = AtomicI32::new(0);

extern "C" fn forward(sig: libc::c_int) {
    let pid = CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        // kill(2) is async-signal-safe.
        unsafe {
            libc::kill(pid, sig);
        }
    } else {
        PENDING.store(sig, Ordering::SeqCst);
    }
}

fn install_forwarding() {
    let handler = forward as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe {
            libc::signal(sig, handler);
        }
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c == '/' || c == '\\' || c == '\0' { '_' } else { c })
        .collect()
}

fn run(args: RunArgs) -> Result<i32, String> {
    let bounds = Bounds {
        cap: args.cap,
        head: args.head,
        tail: args.tail,
    }
    .validate()?;
    let command = args.command.join(" ");
    // First thing: from here on a signal is either forwarded or queued.
    install_forwarding();

    let dir = args
        .spill_dir
        .unwrap_or_else(|| sieve_home().join("spill"));
    let dir = match &args.session {
        Some(s) => dir.join(sanitize(s)),
        None => dir,
    };
    let id = args
        .id
        .map(|s| sanitize(&s))
        .unwrap_or_else(|| format!("{}-{}", now_millis(), std::process::id()));
    let spill = Spill::open(dir.join(format!("{id}.out")));
    let spill_path = spill.path().to_path_buf();

    let (mut reader, writer) = io::pipe().map_err(|e| format!("cannot create pipe: {e}"))?;
    let writer2 = writer
        .try_clone()
        .map_err(|e| format!("cannot clone pipe: {e}"))?;

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::from(writer2));
    let mut child = cmd.spawn().map_err(|e| format!("cannot run sh: {e}"))?;
    // Release the parent's copies of the write end so EOF arrives.
    drop(cmd);
    CHILD.store(child.id() as i32, Ordering::SeqCst);
    let pending = PENDING.swap(0, Ordering::SeqCst);
    if pending > 0 {
        forward(pending);
    }

    let stdout = io::stdout();
    let mut folder = Folder::new(bounds, io::BufWriter::new(stdout.lock()), Some(spill));
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => folder.feed(&buf[..n]).map_err(|e| format!("cannot write output: {e}"))?,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("cannot read command output: {e}")),
        }
    }
    let status = child.wait().map_err(|e| format!("cannot wait for command: {e}"))?;
    CHILD.store(0, Ordering::SeqCst);
    let summary = folder.finish().map_err(|e| format!("cannot write output: {e}"))?;
    if !summary.spilled && !args.keep {
        let _ = std::fs::remove_file(&spill_path);
    }

    let code = status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1);
    Ok(code)
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run(args) => run(args),
    };
    match result {
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(msg) => {
            let _ = writeln!(io::stderr(), "sieve: {msg}");
            ExitCode::from(1)
        }
    }
}
