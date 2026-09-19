//! The policy file: the only knob. Missing file means defaults; a broken
//! file means defaults plus a warning on stderr (the hook fails open).

use crate::Bounds;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub enabled: bool,
    pub bash: Bounds,
    pub ledger: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            enabled: true,
            bash: Bounds::default(),
            ledger: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: Option<String>,
    enabled: Option<bool>,
    bash: Option<BashSection>,
    ledger: Option<LedgerSection>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BashSection {
    cap_bytes: Option<usize>,
    head_bytes: Option<usize>,
    tail_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerSection {
    enabled: Option<bool>,
}

impl Policy {
    /// Apply one policy file's keys over `self`. Returns a warning instead
    /// of failing when the file cannot be read or parsed.
    pub fn apply_file(&mut self, path: &Path) -> Result<(), String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        self.apply_str(&text)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn apply_str(&mut self, text: &str) -> Result<(), String> {
        let f: File = toml::from_str(text).map_err(|e| e.message().to_string())?;
        if let Some(v) = &f.version
            && v != "1"
        {
            return Err(format!("unsupported policy version {v:?}"));
        }
        if let Some(e) = f.enabled {
            self.enabled = e;
        }
        if let Some(b) = f.bash {
            if let Some(v) = b.cap_bytes {
                self.bash.cap = v;
            }
            if let Some(v) = b.head_bytes {
                self.bash.head = v;
            }
            if let Some(v) = b.tail_bytes {
                self.bash.tail = v;
            }
        }
        if let Some(l) = f.ledger
            && let Some(e) = l.enabled
        {
            self.ledger = e;
        }
        self.bash.validate()?;
        Ok(())
    }

    /// User policy at `<home>/policy.toml`, then the project's
    /// `<cwd>/.claude/sieve.toml` key by key, then `SIEVE=off`.
    /// Warnings go back to the caller; the result is always usable.
    pub fn load(home: &Path, cwd: Option<&Path>) -> (Policy, Vec<String>) {
        let mut p = Policy::default();
        let mut warnings = Vec::new();
        for path in [Some(home.join("policy.toml")), cwd.map(|c| c.join(".claude").join("sieve.toml"))]
            .into_iter()
            .flatten()
        {
            let mut trial = p.clone();
            match trial.apply_file(&path) {
                Ok(()) => p = trial,
                Err(w) => warnings.push(w),
            }
        }
        if std::env::var("SIEVE").is_ok_and(|v| v == "off" || v == "0") {
            p.enabled = false;
        }
        (p, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let mut p = Policy::default();
        p.apply_str("").unwrap();
        assert_eq!(p, Policy::default());
    }

    #[test]
    fn overrides_key_by_key() {
        let mut p = Policy::default();
        p.apply_str("version = \"1\"\n[bash]\ncap_bytes = 1000\nhead_bytes = 600\ntail_bytes = 300\n").unwrap();
        assert_eq!(p.bash, Bounds { cap: 1000, head: 600, tail: 300 });
        p.apply_str("[bash]\ntail_bytes = 100\n").unwrap();
        assert_eq!(p.bash, Bounds { cap: 1000, head: 600, tail: 100 });
        assert!(p.enabled && p.ledger);
    }

    #[test]
    fn unknown_key_is_an_error() {
        let mut p = Policy::default();
        assert!(p.apply_str("[bash]\ncap = 1\n").is_err());
        assert!(p.apply_str("[read]\nmax_lines = 1\n").is_err());
    }

    #[test]
    fn bad_bounds_are_an_error() {
        let mut p = Policy::default();
        assert!(p.apply_str("[bash]\ncap_bytes = 10\n").is_err());
    }

    #[test]
    fn wrong_version_is_an_error() {
        let mut p = Policy::default();
        assert!(p.apply_str("version = \"2\"\n").is_err());
    }
}
