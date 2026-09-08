use cs::{Tool, build_command, listing, repo_root, tool};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

fn build(t: &Tool) -> Result<(), String> {
    eprintln!("cs: building {} (first use)", t.name);
    let argv = build_command(t);
    let status = Command::new(&argv[0]).args(&argv[1..]).status().map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("build of {} failed ({status})", t.name)) }
}

fn ensure_built(root: &std::path::Path, name: &str) -> Result<Tool, String> {
    let t = tool(root, name).ok_or_else(|| format!("no tool named {name:?} in {}\n{}", root.display(), listing(root)))?;
    if t.binary.is_some() {
        return Ok(t);
    }
    build(&t)?;
    tool(root, name).filter(|t| t.binary.is_some()).ok_or_else(|| format!("built {name} but no binary appeared"))
}

fn run() -> Result<ExitCode, String> {
    let root = repo_root();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("--help") | Some("-h") | Some("--list") => {
            print!("{}", listing(&root));
            Ok(ExitCode::SUCCESS)
        }
        Some("--version") | Some("-V") => {
            println!("cs {} (repo {})", env!("CARGO_PKG_VERSION"), root.display());
            Ok(ExitCode::SUCCESS)
        }
        Some("--where") => {
            let name = args.get(1).ok_or("usage: cs --where <tool>")?;
            let t = tool(&root, name).ok_or_else(|| format!("no tool named {name:?}"))?;
            match t.binary {
                Some(p) => {
                    println!("{}", p.display());
                    Ok(ExitCode::SUCCESS)
                }
                None => Err(format!("{name} is not built; run `cs --build {name}`")),
            }
        }
        Some("--build") => {
            let names: Vec<String> = if args.len() > 1 { args[1..].to_vec() } else { cs::tools(&root).into_iter().map(|t| t.name).collect() };
            for n in names {
                let t = tool(&root, &n).ok_or_else(|| format!("no tool named {n:?}"))?;
                build(&t)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(name) if name.starts_with('-') => Err(format!("unknown option {name}; try `cs` for the list")),
        Some(name) => {
            let t = ensure_built(&root, name)?;
            let bin = t.binary.expect("ensured");
            let err = Command::new(&bin).args(&args[1..]).exec();
            Err(format!("cannot exec {}: {err}", bin.display()))
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cs: {e}");
            ExitCode::FAILURE
        }
    }
}
