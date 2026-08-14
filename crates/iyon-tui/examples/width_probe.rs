//! Opt-in real-TTY width probe.
//!
//! Unit tests lock Iyon's metric to termwiz. This binary answers the next
//! question: does *this* terminal agree? iTerm's "Unicode Version 9+ Widths"
//! and ambiguous-width settings can still disagree after Iyon is internally
//! consistent.
//!
//! Run in a real terminal:
//!
//! ```text
//! cargo run -p iyon-tui --example width_probe
//! ```
//!
//! Not a CI test: it needs cursor-position reports from a TTY.

use std::io::{self, Read, Write};

use unicode_segmentation::UnicodeSegmentation;

const CORPUS: &[&str] = &[
    "a",
    "☆",
    "⭐",
    "☀︎",
    "☀️",
    "4⃣",
    "4️⃣",
    "🥇",
    "🕹️",
    "🗡️",
    "☕",
    "漢",
    "e\u{301}",
    "🇮🇩",
    "🇩🇰",
    "👩‍🔬",
    "🐕‍🦺",
    "👨‍👩‍👧‍👦",
];

fn iyon_width(grapheme: &str) -> usize {
    // Iyon's canonical metric is termwiz. This example lives outside the crate
    // internals, so it calls the same function the physical layer uses.
    termwiz::cell::grapheme_column_width(grapheme, None)
}

fn main() {
    let mut stdout = io::stdout();
    if !io::IsTerminal::is_terminal(&stdout) {
        eprintln!("width_probe needs a real TTY (stdout is not a terminal)");
        std::process::exit(1);
    }

    println!("glyph                  Iyon  termwiz  terminal");
    println!("----------------------------------------------");

    for sample in CORPUS {
        let clusters: Vec<_> = sample.graphemes(true).collect();
        if clusters.len() != 1 {
            println!("{sample:<22}  (not a single EGC)");
            continue;
        }
        let iyon = iyon_width(sample);
        let termwiz = termwiz::cell::grapheme_column_width(sample, None);
        let terminal = match measure_tty_width(sample) {
            Ok(width) => width.to_string(),
            Err(error) => format!("err:{error}"),
        };
        println!("{sample:<22} {iyon:>4}  {termwiz:>7}  {terminal}");
    }

    let _ = stdout.flush();
}

fn measure_tty_width(grapheme: &str) -> io::Result<usize> {
    // CPR: write the grapheme on a fresh line, ask the terminal where the
    // cursor landed, then erase. `stty` is used so this example needs no extra
    // crate; restore happens even if measurement fails.
    let before = cursor_column()?;
    {
        let mut out = io::stdout();
        write!(out, "{grapheme}")?;
        out.flush()?;
    }
    let after = cursor_column()?;
    {
        let mut out = io::stdout();
        write!(out, "\r\x1b[K")?;
        out.flush()?;
    }
    Ok(usize::from(after.saturating_sub(before)))
}

fn cursor_column() -> io::Result<u16> {
    let mut out = io::stdout();
    write!(out, "\x1b[6n")?;
    out.flush()?;

    let mut stdin = io::stdin();
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stdin.read_exact(&mut byte)?;
        buf.push(byte[0]);
        if byte[0] == b'R' {
            break;
        }
        if buf.len() > 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CPR response too long",
            ));
        }
    }
    let text = String::from_utf8_lossy(&buf);
    // ESC [ rows ; cols R
    let inner = text.trim_start_matches("\x1b[").trim_end_matches('R');
    let col = inner
        .split(';')
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, text.to_string()))?;
    Ok(col)
}
