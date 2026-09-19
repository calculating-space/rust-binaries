//! A small column table that renders as aligned text or GitHub Markdown.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

#[derive(Debug, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub aligns: Vec<Align>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Headers ending in `>` are right-aligned (the marker is stripped).
    pub fn new(headers: &[&str]) -> Table {
        let mut t = Table::default();
        for h in headers {
            if let Some(h) = h.strip_suffix('>') {
                t.headers.push(h.to_string());
                t.aligns.push(Align::Right);
            } else {
                t.headers.push(h.to_string());
                t.aligns.push(Align::Left);
            }
        }
        t
    }

    pub fn row<S: Into<String>>(&mut self, cells: Vec<S>) {
        self.rows.push(cells.into_iter().map(Into::into).collect());
    }

    fn widths(&self) -> Vec<usize> {
        let mut w: Vec<usize> = self.headers.iter().map(|h| h.chars().count()).collect();
        for r in &self.rows {
            for (i, c) in r.iter().enumerate() {
                if i < w.len() {
                    w[i] = w[i].max(c.chars().count());
                }
            }
        }
        w
    }

    pub fn text(&self, indent: &str) -> String {
        let w = self.widths();
        let line = |cells: &[String]| -> String {
            let mut s = String::from(indent);
            for (i, c) in cells.iter().enumerate() {
                if i > 0 {
                    s.push_str("  ");
                }
                let pad = w[i].saturating_sub(c.chars().count());
                match self.aligns[i] {
                    Align::Right => {
                        s.push_str(&" ".repeat(pad));
                        s.push_str(c);
                    }
                    Align::Left => {
                        s.push_str(c);
                        if i + 1 < cells.len() {
                            s.push_str(&" ".repeat(pad));
                        }
                    }
                }
            }
            s.trim_end().to_string()
        };
        let mut out = String::new();
        if self.headers.iter().any(|h| !h.is_empty()) {
            out = line(&self.headers);
            out.push('\n');
        }
        for r in &self.rows {
            out.push_str(&line(r));
            out.push('\n');
        }
        out
    }

    pub fn markdown(&self) -> String {
        let esc = |s: &str| s.replace('|', "\\|");
        let mut out = String::from("|");
        for h in &self.headers {
            out.push_str(&format!(" {} |", esc(h)));
        }
        out.push_str("\n|");
        for a in &self.aligns {
            out.push_str(match a {
                Align::Right => " ---: |",
                Align::Left => " --- |",
            });
        }
        out.push('\n');
        for r in &self.rows {
            out.push('|');
            for c in r {
                out.push_str(&format!(" {} |", esc(c)));
            }
            out.push('\n');
        }
        out
    }
}
