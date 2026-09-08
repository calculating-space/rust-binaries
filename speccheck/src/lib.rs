//! speccheck: a machine spec, a requirements matrix, and a verdict.
//!
//! Three JSON contracts, all versioned:
//! - `Spec`: what this machine is (detected by [`spec::detect`] or supplied).
//! - `Requirements`: what something needs, as hard rules, sense rules, and capability tiers.
//! - `Verdict`: the result of [`check`], with one outcome and per-rule findings.
//!
//! The library is pure: `check` never touches the machine. Only `spec::detect` does.

pub mod spec;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuKind {
    Metal,
    Cuda,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gpu {
    pub kind: GpuKind,
    pub name: String,
    /// Dedicated VRAM in GB; `None` for unified memory (Metal), where system memory is the GPU's.
    pub memory_gb: Option<u64>,
}

/// What a machine is. `tools` maps a tool name to its version line; absent means not found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub version: u32,
    pub os: String,
    pub arch: String,
    pub chip: String,
    pub cores: u32,
    pub memory_gb: u64,
    /// Free space, in GB, on the filesystem where things would be installed.
    pub disk_free_gb: u64,
    pub disk_path: String,
    pub gpus: Vec<Gpu>,
    pub tools: BTreeMap<String, String>,
}

/// One condition on a spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    /// Allowed (os, arch) pairs.
    Platform {
        any_of: Vec<(String, String)>,
    },
    Gpu {
        any_of: Vec<GpuKind>,
    },
    Tool {
        name: String,
    },
    MinMemoryGb {
        gb: u64,
    },
    MinDiskFreeGb {
        gb: u64,
    },
    MinCores {
        n: u32,
    },
}

/// What failing a rule means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Nice to have. Failing it only produces a note.
    Recommended,
    /// It will run, but the reason to run it is gone (for example no GPU for a GPU framework).
    Pointless,
    /// It will not run at all.
    Blocks,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub check: Check,
    pub severity: Severity,
    pub why: String,
}

/// A capability level: what you unlock if all of its checks pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    pub name: String,
    pub enables: String,
    pub checks: Vec<Check>,
}

/// The requirements matrix for one subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirements {
    pub version: u32,
    pub subject: String,
    pub rules: Vec<Rule>,
    pub tiers: Vec<Tier>,
}

impl Requirements {
    /// Every tool name any rule or tier asks about; feed it to `spec::detect`.
    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .rules
            .iter()
            .map(|r| &r.check)
            .chain(self.tiers.iter().flat_map(|t| t.checks.iter()))
            .filter_map(|c| {
                if let Check::Tool { name } = c {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    CanRun,
    Pointless,
    CannotRun,
}

impl Outcome {
    /// Process exit status for CLIs: 0 run, 2 pointless, 3 cannot.
    pub fn exit_code(self) -> u8 {
        match self {
            Outcome::CanRun => 0,
            Outcome::Pointless => 2,
            Outcome::CannotRun => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub check: Check,
    pub severity: Severity,
    pub pass: bool,
    pub why: String,
    /// What the machine actually has, for the row.
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierResult {
    pub name: String,
    pub enables: String,
    pub ok: bool,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub version: u32,
    pub subject: String,
    pub outcome: Outcome,
    pub findings: Vec<Finding>,
    pub tiers: Vec<TierResult>,
}

/// Human wording of a check, for tables.
pub fn need(c: &Check) -> String {
    match c {
        Check::Platform { any_of } => any_of
            .iter()
            .map(|(o, a)| format!("{o}/{a}"))
            .collect::<Vec<_>>()
            .join(" or "),
        Check::Gpu { any_of } => format!(
            "{} GPU",
            any_of
                .iter()
                .map(|k| format!("{k:?}").to_lowercase())
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        Check::Tool { name } => format!("`{name}` installed"),
        Check::MinMemoryGb { gb } => format!("{gb} GB memory"),
        Check::MinDiskFreeGb { gb } => format!("{gb} GB free disk"),
        Check::MinCores { n } => format!("{n} CPU cores"),
    }
}

/// Evaluate one check. Returns pass and what the machine actually has.
pub fn evaluate(spec: &Spec, c: &Check) -> (bool, String) {
    match c {
        Check::Platform { any_of } => (
            any_of.iter().any(|(o, a)| *o == spec.os && *a == spec.arch),
            format!("{}/{}", spec.os, spec.arch),
        ),
        Check::Gpu { any_of } => {
            let have: Vec<_> = spec
                .gpus
                .iter()
                .map(|g| format!("{:?}", g.kind).to_lowercase())
                .collect();
            (
                spec.gpus.iter().any(|g| any_of.contains(&g.kind)),
                if have.is_empty() {
                    "no GPU".into()
                } else {
                    have.join(", ")
                },
            )
        }
        Check::Tool { name } => match spec.tools.get(name) {
            Some(v) => (true, v.clone()),
            None => (false, "not found".into()),
        },
        Check::MinMemoryGb { gb } => (spec.memory_gb >= *gb, format!("{} GB", spec.memory_gb)),
        Check::MinDiskFreeGb { gb } => (
            spec.disk_free_gb >= *gb,
            format!("{} GB free", spec.disk_free_gb),
        ),
        Check::MinCores { n } => (spec.cores >= *n, format!("{} cores", spec.cores)),
    }
}

/// Pure check of a spec against requirements.
pub fn check(spec: &Spec, req: &Requirements) -> Verdict {
    let findings: Vec<Finding> = req
        .rules
        .iter()
        .map(|r| {
            let (pass, actual) = evaluate(spec, &r.check);
            Finding {
                check: r.check.clone(),
                severity: r.severity,
                pass,
                why: r.why.clone(),
                actual,
            }
        })
        .collect();
    let worst = findings
        .iter()
        .filter(|f| !f.pass)
        .map(|f| f.severity)
        .max();
    let outcome = match worst {
        Some(Severity::Blocks) => Outcome::CannotRun,
        Some(Severity::Pointless) => Outcome::Pointless,
        _ => Outcome::CanRun,
    };
    let tiers = req
        .tiers
        .iter()
        .map(|t| {
            let missing: Vec<String> = t
                .checks
                .iter()
                .filter(|c| !evaluate(spec, c).0)
                .map(need)
                .collect();
            TierResult {
                name: t.name.clone(),
                enables: t.enables.clone(),
                ok: missing.is_empty(),
                missing,
            }
        })
        .collect();
    Verdict {
        version: CONTRACT_VERSION,
        subject: req.subject.clone(),
        outcome,
        findings,
        tiers,
    }
}

/// Plain-text rendering: a verdict line, a rule table, and a tier list.
pub fn render(v: &Verdict) -> String {
    let mut out = String::new();
    let headline = match v.outcome {
        Outcome::CanRun => "CAN RUN",
        Outcome::Pointless => "RUNS, BUT DOES NOT MAKE SENSE HERE",
        Outcome::CannotRun => "CANNOT RUN",
    };
    out.push_str(&format!("{}: {headline}\n\n", v.subject));
    let width = v
        .findings
        .iter()
        .map(|f| need(&f.check).len())
        .max()
        .unwrap_or(4)
        .max(4);
    out.push_str(&format!(
        "  {:<3} {:<width$}  {:<12} {:<14} {}\n",
        "", "NEED", "SEVERITY", "THIS MACHINE", "WHY"
    ));
    for f in &v.findings {
        let mark = if f.pass { "ok" } else { "--" };
        let sev = match f.severity {
            Severity::Blocks => "blocks",
            Severity::Pointless => "pointless",
            Severity::Recommended => "recommended",
        };
        let actual: String = if f.actual.chars().count() > 14 {
            format!("{}…", f.actual.chars().take(13).collect::<String>())
        } else {
            f.actual.clone()
        };
        out.push_str(&format!(
            "  {mark:<3} {:<width$}  {sev:<12} {actual:<14} {}\n",
            need(&f.check),
            f.why
        ));
    }
    if !v.tiers.is_empty() {
        out.push_str("\n  TIERS\n");
        for t in &v.tiers {
            if t.ok {
                out.push_str(&format!("  ok  {:<28} {}\n", t.name, t.enables));
            } else {
                out.push_str(&format!(
                    "  --  {:<28} {} (needs {})\n",
                    t.name,
                    t.enables,
                    t.missing.join(", ")
                ));
            }
        }
    }
    out
}
