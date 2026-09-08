//! Bounded output folding for `sieve run`.
//!
//! The runner streams a command's merged stdout/stderr through a [`Folder`].
//! Under the cap the folder is transparent: every byte goes out in order.
//! Over the cap it prints the first `head` bytes, one marker line, and the
//! last `tail` bytes, while a spill file keeps the full output.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Output limits in bytes. `cap` must be at least `head + tail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub cap: usize,
    pub head: usize,
    pub tail: usize,
}

impl Bounds {
    pub fn validate(self) -> Result<Self, String> {
        if self.cap < self.head + self.tail {
            return Err(format!(
                "--cap {} is smaller than --head {} + --tail {}",
                self.cap, self.head, self.tail
            ));
        }
        Ok(self)
    }
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            cap: 16384,
            head: 8192,
            tail: 4096,
        }
    }
}

/// What happened to the output, for the caller's report or ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub total_bytes: u64,
    pub shown_bytes: u64,
    pub spilled: bool,
    pub spill_path: Option<PathBuf>,
    pub spill_error: Option<String>,
}

/// Where the full output goes while it streams.
pub struct Spill {
    path: PathBuf,
    file: Option<File>,
    error: Option<String>,
}

impl Spill {
    /// Open `path` for writing, creating parent directories. Never fails:
    /// an unwritable spill is recorded and reported in the marker instead.
    pub fn open(path: PathBuf) -> Spill {
        let created = path
            .parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()));
        let file = created.and_then(|()| File::create(&path));
        match file {
            Ok(f) => Spill {
                path,
                file: Some(f),
                error: None,
            },
            Err(e) => Spill {
                path,
                file: None,
                error: Some(e.to_string()),
            },
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct Folder<W: Write> {
    bounds: Bounds,
    out: W,
    out_open: bool,
    spill: Option<Spill>,
    total: u64,
    /// Bytes after the head: everything up to the cap, then only the tail.
    pending: Vec<u8>,
    over: bool,
    last_out: Option<u8>,
}

impl<W: Write> Folder<W> {
    pub fn new(bounds: Bounds, out: W, spill: Option<Spill>) -> Self {
        Folder {
            bounds,
            out,
            out_open: true,
            spill,
            total: 0,
            pending: Vec::new(),
            over: false,
            last_out: None,
        }
    }

    fn emit(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() || !self.out_open {
            return Ok(());
        }
        match self.out.write_all(bytes) {
            Ok(()) => {
                self.last_out = bytes.last().copied();
                Ok(())
            }
            // The reader went away; keep draining the child so it can exit.
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                self.out_open = false;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Feed one chunk of command output, in arrival order.
    pub fn feed(&mut self, mut chunk: &[u8]) -> io::Result<()> {
        if let Some(spill) = &mut self.spill
            && let Some(file) = &mut spill.file
            && let Err(e) = file.write_all(chunk)
        {
            spill.error = Some(e.to_string());
            spill.file = None;
        }
        let head = self.bounds.head as u64;
        if self.total < head {
            let room = (head - self.total) as usize;
            let n = room.min(chunk.len());
            self.emit(&chunk[..n])?;
            self.total += n as u64;
            chunk = &chunk[n..];
        }
        if chunk.is_empty() {
            return Ok(());
        }
        self.total += chunk.len() as u64;
        if self.over {
            // Keep only the last `tail` bytes; skip what cannot matter.
            let tail = self.bounds.tail;
            if chunk.len() >= tail {
                self.pending.clear();
                self.pending.extend_from_slice(&chunk[chunk.len() - tail..]);
            } else {
                self.pending.extend_from_slice(chunk);
                if self.pending.len() > tail {
                    let excess = self.pending.len() - tail;
                    self.pending.drain(..excess);
                }
            }
        } else {
            self.pending.extend_from_slice(chunk);
            if self.total > self.bounds.cap as u64 {
                self.over = true;
                let tail = self.bounds.tail;
                if self.pending.len() > tail {
                    let excess = self.pending.len() - tail;
                    self.pending.drain(..excess);
                }
            }
        }
        Ok(())
    }

    /// Flush the decision: everything if under the cap, else marker and tail.
    pub fn finish(mut self) -> io::Result<Summary> {
        let (spill_path, spill_error, spill_file) = match self.spill.take() {
            Some(s) => (Some(s.path), s.error, s.file),
            None => (None, None, None),
        };
        if let Some(mut f) = spill_file {
            let _ = f.flush();
        }
        if !self.over {
            let pending = std::mem::take(&mut self.pending);
            self.emit(&pending)?;
            self.out.flush().or_else(ignore_broken_pipe)?;
            let shown = self.total;
            return Ok(Summary {
                total_bytes: self.total,
                shown_bytes: shown,
                spilled: false,
                spill_path,
                spill_error,
            });
        }
        let tail_len = self.pending.len() as u64;
        let not_shown = self.total - self.bounds.head as u64 - tail_len;
        let mut marker = String::new();
        if self.last_out.is_some_and(|b| b != b'\n') {
            marker.push('\n');
        }
        marker.push_str(&format!(
            "[sieve: {} total, {} not shown; ",
            human(self.total),
            human(not_shown)
        ));
        match (&spill_path, &spill_error) {
            (Some(p), None) => marker.push_str(&format!(
                "full output in {}; Grep it or Read a range]\n",
                p.display()
            )),
            (_, Some(e)) => marker.push_str(&format!("full output not saved: {e}]\n")),
            (None, None) => marker.push_str("full output not saved]\n"),
        }
        self.emit(marker.as_bytes())?;
        let pending = std::mem::take(&mut self.pending);
        self.emit(&pending)?;
        self.out.flush().or_else(ignore_broken_pipe)?;
        Ok(Summary {
            total_bytes: self.total,
            shown_bytes: self.bounds.head as u64 + tail_len,
            spilled: true,
            spill_path,
            spill_error,
        })
    }
}

fn ignore_broken_pipe(e: io::Error) -> io::Result<()> {
    if e.kind() == io::ErrorKind::BrokenPipe {
        Ok(())
    } else {
        Err(e)
    }
}

/// Byte counts for people: `512 B`, `412.3 KB`, `3.2 MB`.
pub fn human(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

/// `$SIEVE_HOME`, else `~/.sieve`.
pub fn sieve_home() -> PathBuf {
    if let Some(h) = std::env::var_os("SIEVE_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".sieve")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(bounds: Bounds, chunks: &[&[u8]]) -> (Vec<u8>, Summary) {
        let mut out = Vec::new();
        let mut f = Folder::new(bounds, &mut out, None);
        for c in chunks {
            f.feed(c).unwrap();
        }
        let s = f.finish().unwrap();
        (out, s)
    }

    fn b(cap: usize, head: usize, tail: usize) -> Bounds {
        Bounds { cap, head, tail }.validate().unwrap()
    }

    #[test]
    fn under_cap_is_transparent() {
        let (out, s) = fold(b(10, 4, 3), &[b"ab", b"cdefgh", b"ij"]);
        assert_eq!(out, b"abcdefghij");
        assert!(!s.spilled);
        assert_eq!(s.total_bytes, 10);
        assert_eq!(s.shown_bytes, 10);
    }

    #[test]
    fn exactly_at_cap_is_transparent() {
        let (out, _) = fold(b(5, 2, 2), &[b"12345"]);
        assert_eq!(out, b"12345");
    }

    #[test]
    fn empty_output_is_empty() {
        let (out, s) = fold(b(5, 2, 2), &[]);
        assert!(out.is_empty());
        assert_eq!(s.total_bytes, 0);
    }

    #[test]
    fn over_cap_prints_head_marker_tail() {
        let (out, s) = fold(b(10, 4, 3), &[b"0123456789", b"ABCDEF"]);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "0123\n[sieve: 16 B total, 9 B not shown; full output not saved]\nDEF"
        );
        assert!(s.spilled);
        assert_eq!(s.shown_bytes, 7);
    }

    #[test]
    fn marker_needs_no_newline_after_a_newline() {
        let (out, _) = fold(b(6, 2, 2), &[b"a\n", b"bcdefgh"]);
        assert!(String::from_utf8(out).unwrap().starts_with("a\n[sieve:"));
    }

    #[test]
    fn tail_survives_many_small_chunks() {
        let bounds = b(20, 5, 4);
        let mut out = Vec::new();
        let mut f = Folder::new(bounds, &mut out, None);
        for i in 0..1000u32 {
            f.feed(&[b'a' + (i % 26) as u8]).unwrap();
        }
        let s = f.finish().unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("abcde\n[sieve: 1000 B total, 991 B not shown"));
        let expected_tail: String = (996..1000u32).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        assert!(text.ends_with(&expected_tail));
        assert_eq!(s.shown_bytes, 9);
    }

    #[test]
    fn big_chunk_after_over_keeps_last_tail() {
        let (out, _) = fold(b(6, 2, 2), &[b"xxxxxxx", &[b'y'; 5000], b"z"]);
        assert!(out.ends_with(b"yz"));
        assert_eq!(&out[..2], b"xx");
    }

    #[test]
    fn cap_below_head_plus_tail_is_rejected() {
        assert!(Bounds { cap: 5, head: 4, tail: 2 }.validate().is_err());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1023), "1023 B");
        assert_eq!(human(1024), "1.0 KB");
        assert_eq!(human(16384 + 1), "16.0 KB");
        assert_eq!(human(7105), "6.9 KB");
        assert_eq!(human(3 * 1024 * 1024 + 200_000), "3.2 MB");
    }

    #[test]
    fn spill_holds_everything_even_when_under_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("x.out");
        let mut out = Vec::new();
        let mut f = Folder::new(b(100, 10, 10), &mut out, Some(Spill::open(path.clone())));
        f.feed(b"hello ").unwrap();
        f.feed(b"world").unwrap();
        let s = f.finish().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
        assert_eq!(s.spill_path, Some(path));
        assert!(s.spill_error.is_none());
    }

    #[test]
    fn unwritable_spill_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plain");
        std::fs::write(&file, b"").unwrap();
        // A path whose parent is a regular file cannot be created.
        let path = file.join("child.out");
        let mut out = Vec::new();
        let mut f = Folder::new(b(4, 2, 2), &mut out, Some(Spill::open(path)));
        f.feed(b"0123456789").unwrap();
        let s = f.finish().unwrap();
        assert!(s.spill_error.is_some());
        assert!(String::from_utf8(out).unwrap().contains("full output not saved: "));
    }
}
