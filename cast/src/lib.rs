//! `cast`: a tiny script language for spawning agents.
//!
//! The input is a single expression in the shape `[a, b, c].map(x => role)`.
//! The list holds the items (paths, names, anything textual); `role` names
//! the prompt each agent gets, with the item substituted in. Every item
//! becomes one [`errand::Job`], and the CLI runs them side by side.
//!
//! This module is the pure part: parsing the expression and building prompts.

/// A parsed `[items].map(param => role)` expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    pub items: Vec<String>,
    /// The lambda parameter; `{param}` in a prompt file is replaced by the item.
    pub param: String,
    /// The role name: which prompt to use.
    pub role: String,
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\'', '`'] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Parse `[a, b].map(p => role)`. Accepted variations: quoted items, `->`
/// instead of `=>`, `(p) =>`, `role(p)` as the body, `.map(role)` with no
/// lambda (the parameter is then `p`), and a trailing semicolon.
pub fn parse(src: &str) -> Result<Expr, String> {
    let src = src.trim().trim_end_matches(';').trim();
    let rest = src
        .strip_prefix('[')
        .ok_or("expected the expression to start with a list: [a, b].map(p => role)")?;
    let close = rest
        .find(']')
        .ok_or("the list has no closing ]")?;
    let items: Vec<String> = rest[..close]
        .split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if items.is_empty() {
        return Err("the list is empty".into());
    }
    let after = rest[close + 1..].trim();
    let inner = after
        .strip_prefix(".map(")
        .or_else(|| after.strip_prefix(".map ("))
        .and_then(|s| s.trim_end().strip_suffix(')'))
        .ok_or("expected .map(p => role) after the list")?
        .trim();
    let (param, body) = match inner.split_once("=>").or_else(|| inner.split_once("->")) {
        Some((p, b)) => (p.trim().trim_start_matches('(').trim_end_matches(')').trim(), b.trim()),
        None => ("p", inner),
    };
    if !is_ident(param) {
        return Err(format!("bad parameter name {param:?} in .map(...)"));
    }
    let role = match body.split_once('(') {
        Some((name, _)) => name.trim(),
        None => body,
    };
    if !is_ident(role) {
        return Err(format!("bad role name {role:?}: use letters, digits, _ or -"));
    }
    Ok(Expr {
        items,
        param: param.to_string(),
        role: role.to_string(),
    })
}

/// The prompt used when no `<role>.md` file exists.
pub fn builtin_template(role: &str, param: &str) -> String {
    format!(
        "You are the {role}. Your input is: {{{param}}}\n\n\
If the input names a file or directory, read it first. Do the job the role \
name implies, thoroughly, and answer in Markdown. Do not modify any files.\n"
    )
}

/// Fill a template for one item. `{param}` is replaced wherever it appears;
/// a template without the placeholder gets the item appended as an input line.
pub fn prompt_for(template: &str, param: &str, item: &str) -> String {
    let placeholder = format!("{{{param}}}");
    if template.contains(&placeholder) {
        template.replace(&placeholder, item)
    } else {
        format!("{}\n\nInput: {item}\n", template.trim_end())
    }
}

/// A file-name-safe version of an item: `papers/one.pdf` becomes `papers-one-pdf`.
pub fn slug(item: &str) -> String {
    let s: String = item
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "item".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_canonical_form() {
        let e = parse("[paper1,paper2].map(p => researcher)").unwrap();
        assert_eq!(e.items, ["paper1", "paper2"]);
        assert_eq!(e.param, "p");
        assert_eq!(e.role, "researcher");
    }

    #[test]
    fn tolerates_spaces_quotes_arrows_and_calls() {
        let e = parse(" [ \"a.pdf\", 'b c' , d ] .map( (x) -> reviewer(x) ); ").unwrap();
        assert_eq!(e.items, ["a.pdf", "b c", "d"]);
        assert_eq!(e.param, "x");
        assert_eq!(e.role, "reviewer");
        let e = parse("[a].map(summariser)").unwrap();
        assert_eq!((e.param.as_str(), e.role.as_str()), ("p", "summariser"));
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse("paper1.map(p => r)").is_err());
        assert!(parse("[].map(p => r)").is_err());
        assert!(parse("[a, b]").is_err());
        assert!(parse("[a].map(p => )").is_err());
        assert!(parse("[a].map(p => two words)").is_err());
        assert!(parse("[a].map(1p => r)").is_err());
    }

    #[test]
    fn prompt_substitution_and_fallback() {
        assert_eq!(prompt_for("Read {p} now", "p", "x.pdf"), "Read x.pdf now");
        assert_eq!(prompt_for("Read it\n", "p", "x.pdf"), "Read it\n\nInput: x.pdf\n");
        assert!(builtin_template("researcher", "p").contains("{p}"));
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("papers/one.pdf"), "papers-one-pdf");
        assert_eq!(slug("  ../x  "), "x");
        assert_eq!(slug("///"), "item");
    }
}
