//! Small constructors for requirement matrices, shared by the recipe modules.

use speccheck::{Check, Rule, Severity, Tier};

pub(super) fn rule(check: Check, severity: Severity, why: &str) -> Rule {
    Rule {
        check,
        severity,
        why: why.into(),
    }
}
pub(super) fn tier(name: &str, enables: &str, checks: Vec<Check>) -> Tier {
    Tier {
        name: name.into(),
        enables: enables.into(),
        checks,
    }
}
pub(super) fn platform(pairs: &[(&str, &str)]) -> Check {
    Check::Platform {
        any_of: pairs
            .iter()
            .map(|(o, a)| (o.to_string(), a.to_string()))
            .collect(),
    }
}
pub(super) fn tool(name: &str) -> Check {
    Check::Tool { name: name.into() }
}
pub(super) fn mem(gb: u64) -> Check {
    Check::MinMemoryGb { gb }
}
pub(super) fn disk(gb: u64) -> Check {
    Check::MinDiskFreeGb { gb }
}
pub(super) fn voice(language: &str) -> Check {
    Check::Voice {
        language: language.into(),
    }
}
pub(super) fn microphone() -> Check {
    Check::AudioInput
}
