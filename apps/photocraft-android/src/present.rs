//! Presentation settings for latency experiments. Android has no environment variables, so a file
//! named `present` next to the stroke trace's `stroke-trace` flag (see `app.rs`) holds them,
//! words separated by spaces: `latency=1` (frames the swap chain may queue, 1 to 3) and
//! `mode=fifo` or `mode=novsync` (no vsync where the driver offers it; otherwise vsync stays).
//! They apply at the next start, and the stroke trace's first line names them.

/// What a `present` file asks for; `None` leaves the app's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Present {
    /// `Some(true)`: vsync (`fifo`); `Some(false)`: no vsync where offered (`novsync`).
    pub vsync: Option<bool>,
    pub latency: Option<u32>,
}

/// The settings in `text` and a message for each word that isn't one.
pub fn parse(text: &str) -> (Present, Vec<String>) {
    let mut out = Present::default();
    let mut problems = Vec::new();
    for word in text.split_whitespace() {
        match word.split_once('=') {
            Some(("mode", "fifo")) => out.vsync = Some(true),
            Some(("mode", "novsync")) => out.vsync = Some(false),
            Some(("latency", n)) if matches!(n.parse::<u32>(), Ok(1..=3)) => out.latency = n.parse().ok(),
            _ => problems.push(format!("present: ignoring {word:?} (use latency=1..3, mode=fifo or mode=novsync)")),
        }
    }
    (out, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_set_latency_and_mode() {
        assert_eq!(parse(""), (Present::default(), vec![]));
        assert_eq!(parse("latency=1 mode=novsync\n").0, Present { vsync: Some(false), latency: Some(1) });
        assert_eq!(parse("mode=fifo").0, Present { vsync: Some(true), latency: None });
    }

    #[test]
    fn anything_else_is_reported_and_ignored() {
        let (p, problems) = parse("latency=0 latency=9 latency=x mode=mailbox speed=3 latency=2");
        assert_eq!(p, Present { vsync: None, latency: Some(2) });
        assert_eq!(problems.len(), 5, "{problems:?}");
    }
}
