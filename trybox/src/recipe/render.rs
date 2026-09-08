//! Recipes as text: the table, the man page, and the requirements matrix checked here.

use super::{Example, RECIPES, Recipe};
use speccheck::{Severity, Verdict};

pub fn wrap(text: &str, indent: usize, width: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width - indent {
            out.push_str(&pad);
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(&pad);
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn example(out: &mut String, label: &str, e: &Example) {
    out.push_str(&format!("    {label}{}\n", e.title));
    out.push_str(&wrap(e.learn, 8, 78));
    if !e.models.is_empty() {
        out.push_str(&wrap(
            &format!("Fetched first: {}", e.models.join(", ")),
            8,
            78,
        ));
    }
    for (i, l) in e.run.lines().enumerate() {
        out.push_str(&format!("        {} {l}\n", if i == 0 { "$" } else { " " }));
    }
    out.push_str(&wrap(&format!("You should see: {}", e.expect), 8, 78));
    out.push('\n');
}

/// One-line-per-recipe table for `trybox recipes`.
pub fn table() -> String {
    let mut out = format!("{:<8} {:<6} {}\n", "RECIPE", "STEPS", "WHAT IT IS");
    for r in RECIPES {
        out.push_str(&format!(
            "{:<8} {:<6} {}\n",
            r.name,
            r.tour.len() + 1,
            r.summary
        ));
    }
    out
}

/// The REQUIREMENTS section: the matrix, annotated with this machine's verdict when given.
pub fn requirements_section(r: &Recipe, verdict: Option<&Verdict>) -> String {
    let req = (r.requirements)();
    let mut out = String::from("REQUIREMENTS\n");
    if let Some(v) = verdict {
        let line = match v.outcome {
            speccheck::Outcome::CanRun => "This machine can run it.",
            speccheck::Outcome::Pointless => {
                "This machine can run it, but it does not make sense here (see below)."
            }
            speccheck::Outcome::CannotRun => "This machine cannot run it (see below).",
        };
        out.push_str(&format!("    {line}\n\n"));
    }
    let width = req
        .rules
        .iter()
        .map(|x| speccheck::need(&x.check).len())
        .max()
        .unwrap_or(4)
        .max(4);
    for (i, x) in req.rules.iter().enumerate() {
        let (mark, actual) = match verdict.and_then(|v| v.findings.get(i)) {
            Some(f) => (if f.pass { "ok" } else { "--" }, format!(" [{}]", f.actual)),
            None => ("  ", String::new()),
        };
        let sev = match x.severity {
            Severity::Blocks => "required",
            Severity::Pointless => "for it to make sense",
            Severity::Recommended => "recommended",
        };
        out.push_str(&format!(
            "    {mark} {:<width$}  {sev}{actual}\n",
            speccheck::need(&x.check)
        ));
        out.push_str(&wrap(&x.why, 8 + width, 78 + width));
    }
    if !req.tiers.is_empty() {
        out.push_str("\n    What your machine unlocks:\n");
        for (i, t) in req.tiers.iter().enumerate() {
            let needs: Vec<String> = t.checks.iter().map(speccheck::need).collect();
            let mark = match verdict.and_then(|v| v.tiers.get(i)) {
                Some(tr) if tr.ok => "ok",
                Some(_) => "--",
                None => "  ",
            };
            out.push_str(&format!(
                "    {mark} {:<22} {} ({})\n",
                t.name,
                t.enables,
                needs.join(", ")
            ));
        }
    }
    out.push('\n');
    out
}

/// A man page for one recipe: what it is, hello world, the tour, where to go next.
/// Pass a verdict to annotate the requirements with this machine's actual values.
pub fn man(r: &Recipe, verdict: Option<&Verdict>) -> String {
    let upper = r.name.to_uppercase();
    let mut out = format!(
        "TRYBOX({upper})\n\nNAME\n    {} - {}\n\n",
        r.name, r.summary
    );
    out.push_str("WHAT IT IS\n");
    out.push_str(&wrap(r.what, 4, 78));
    out.push_str(&format!("    {}\n\n", r.repo));
    out.push_str(&requirements_section(r, verdict));
    out.push_str("HELLO WORLD\n");
    example(&mut out, "", &r.hello);
    out.push_str("TOUR\n");
    for (i, e) in r.tour.iter().enumerate() {
        example(&mut out, &format!("{}. ", i + 1), e);
    }
    if !r.suggestions.is_empty() {
        out.push_str("GO FURTHER\n    Things the resident agent can set up for you:\n");
        for s in r.suggestions {
            out.push_str(&wrap(&format!("- {s}"), 4, 78));
        }
        out.push('\n');
    }
    if !r.caveats.is_empty() {
        out.push_str("CAVEATS\n");
        for c in r.caveats {
            out.push_str(&wrap(&format!("- {c}"), 4, 78));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "TRY IT\n    trybox check {n}          can this machine run it, and does it make sense\n    trybox explore {n}        run the hello world, then pick tour steps\n    trybox agent {n}          open-ended: an agent inside the sandbox\n    trybox dispose {n} -y     delete everything it downloaded\n\nUNDER THE HOOD\n    backend {} · python {} · packages: {}\n",
        r.backend.as_str(),
        r.python,
        if r.packages.is_empty() { "none".to_string() } else { r.packages.join(" ") },
        n = r.name
    ));
    out
}

/// Detect this machine with the tools the recipe cares about, and check it.
pub fn check_here(r: &Recipe, disk_path: &std::path::Path) -> Verdict {
    let req = (r.requirements)();
    let spec = speccheck::spec::detect(disk_path, &req.probes());
    speccheck::check(&spec, &req)
}

/// The verdict for this machine with only the outcome-deciding rules probed. For menus that
/// show "can run here"; the recommended rows and tiers are not to be trusted from this one.
pub fn outcome_here(r: &Recipe, disk_path: &std::path::Path) -> Verdict {
    let req = (r.requirements)();
    let spec = speccheck::spec::detect(disk_path, &req.outcome_probes(Severity::Pointless));
    speccheck::check(&spec, &req)
}
