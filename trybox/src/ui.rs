//! A select prompt in the style of Claude Code's question tool: a list of
//! options, one marked "(Recommended)" and placed first, arrow keys to move,
//! Enter to choose. Falls back to a numbered list read from stdin when not on
//! a terminal, so scripts and tests can drive it.

use std::io::{BufRead, IsTerminal, Read, Write};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub label: String,
    pub detail: String,
    pub recommended: bool,
}

impl Choice {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            recommended: false,
        }
    }
    pub fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }
}

/// Put the recommended choice first, as the question tool does. Returns the new order as
/// indexes into the original slice.
pub fn order(choices: &[Choice]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..choices.len()).collect();
    if let Some(pos) = choices.iter().position(|c| c.recommended) {
        idx.remove(pos);
        idx.insert(0, pos);
    }
    idx
}

/// One frame of the menu. `cursor` is a position in `order`.
pub fn render(
    question: &str,
    choices: &[Choice],
    order: &[usize],
    cursor: usize,
    color: bool,
) -> String {
    let (bold, dim, cyan, reset) = if color {
        ("\x1b[1m", "\x1b[2m", "\x1b[36m", "\x1b[0m")
    } else {
        ("", "", "", "")
    };
    let mut out = format!("{bold}{question}{reset}\n");
    for (pos, &i) in order.iter().enumerate() {
        let c = &choices[i];
        let marker = if pos == cursor {
            format!("{cyan}❯{reset}")
        } else {
            " ".into()
        };
        let rec = if c.recommended { " (Recommended)" } else { "" };
        let label = if pos == cursor {
            format!("{cyan}{}{reset}", c.label)
        } else {
            c.label.clone()
        };
        out.push_str(&format!("{marker} {:>2}. {label}{rec}\n", pos + 1));
        if !c.detail.is_empty() {
            out.push_str(&format!("      {dim}{}{reset}\n", c.detail));
        }
    }
    out.push_str(&format!(
        "{dim}↑/↓ or j/k to move · Enter to choose · number to jump · q to go back{reset}\n"
    ));
    out
}

/// Parse a typed answer in the non-interactive fallback: a 1-based number, or a label prefix.
pub fn parse_fallback(line: &str, choices: &[Choice], order: &[usize]) -> Option<usize> {
    let t = line.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("q") {
        return None;
    }
    if let Ok(n) = t.parse::<usize>() {
        return order.get(n.checked_sub(1)?).copied();
    }
    order.iter().copied().find(|&i| {
        choices[i]
            .label
            .to_lowercase()
            .starts_with(&t.to_lowercase())
    })
}

struct RawMode {
    saved: Option<String>,
}

impl RawMode {
    fn enter() -> Self {
        let saved = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        let _ = Command::new("stty")
            .args(["-echo", "-icanon", "min", "0", "time", "1"])
            .stdin(Stdio::inherit())
            .status();
        print!("\x1b[?25l");
        let _ = std::io::stdout().flush();
        Self { saved }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
        if let Some(s) = &self.saved {
            let _ = Command::new("stty").arg(s).stdin(Stdio::inherit()).status();
        } else {
            let _ = Command::new("stty")
                .arg("sane")
                .stdin(Stdio::inherit())
                .status();
        }
    }
}

enum Key {
    Up,
    Down,
    Enter,
    Quit,
    Digit(usize),
    Other,
}

fn read_key(stdin: &mut impl Read) -> Result<Option<Key>, String> {
    let mut b = [0u8; 1];
    let n = stdin.read(&mut b).map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(match b[0] {
        b'\r' | b'\n' => Key::Enter,
        b'q' | 3 | 4 => Key::Quit,
        b'j' => Key::Down,
        b'k' => Key::Up,
        d @ b'1'..=b'9' => Key::Digit((d - b'0') as usize),
        0x1b => {
            let mut seq = [0u8; 2];
            let got = stdin.read(&mut seq).map_err(|e| e.to_string())?;
            match (got, seq) {
                (2, [b'[', b'A']) => Key::Up,
                (2, [b'[', b'B']) => Key::Down,
                (0, _) => Key::Quit,
                _ => Key::Other,
            }
        }
        _ => Key::Other,
    }))
}

/// Ask. Returns the index into `choices` of the pick, or None if the user backed out.
pub fn select(question: &str, choices: &[Choice]) -> Result<Option<usize>, String> {
    if choices.is_empty() {
        return Ok(None);
    }
    let order = order(choices);
    let mut stdout = std::io::stdout();
    if !std::io::stdin().is_terminal() || !stdout.is_terminal() {
        print!("{}> ", render(question, choices, &order, 0, false));
        stdout.flush().ok();
        let mut line = String::new();
        if std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            println!();
            return Ok(None);
        }
        return Ok(parse_fallback(&line, choices, &order));
    }
    let _raw = RawMode::enter();
    let mut cursor = 0usize;
    let mut stdin = std::io::stdin().lock();
    let mut lines = 0usize;
    loop {
        let frame = render(question, choices, &order, cursor, true);
        if lines > 0 {
            print!("\x1b[{lines}A\x1b[J");
        }
        print!("{frame}");
        stdout.flush().ok();
        lines = frame.lines().count();
        let key = loop {
            if let Some(k) = read_key(&mut stdin)? {
                break k;
            }
        };
        match key {
            Key::Up => cursor = cursor.checked_sub(1).unwrap_or(order.len() - 1),
            Key::Down => cursor = (cursor + 1) % order.len(),
            Key::Digit(d) if d <= order.len() => cursor = d - 1,
            Key::Enter | Key::Quit => {
                print!("\x1b[{lines}A\x1b[J");
                let picked = matches!(key, Key::Enter).then_some(order[cursor]);
                match picked {
                    Some(i) => println!(
                        "\x1b[2m{question}\x1b[0m \x1b[36m❯\x1b[0m {}",
                        choices[i].label
                    ),
                    None => println!("\x1b[2m{question} · back\x1b[0m"),
                }
                stdout.flush().ok();
                return Ok(picked);
            }
            _ => {}
        }
    }
}

/// A yes/no built from select. `yes_label` is what the affirmative does; "Keep it" is the alternative.
pub fn confirm(question: &str, yes_label: &str, no_label: &str) -> Result<bool, String> {
    let choices = [Choice::new(yes_label, ""), Choice::new(no_label, "")];
    Ok(select(question, &choices)? == Some(0))
}
