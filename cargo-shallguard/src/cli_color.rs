//! ANSI styling for interactive command output.

use std::borrow::Cow;
use std::io::IsTerminal;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";
const BOLD: &str = "\x1b[1m";
const BOLD_BLUE: &str = "\x1b[1;34m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

pub(crate) fn stdout_enabled() -> bool {
    color_enabled(
        std::io::stdout().is_terminal(),
        std::env::var_os("CI").is_some(),
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("TERM").ok().as_deref(),
    )
}

pub(crate) fn stderr_enabled() -> bool {
    color_enabled(
        std::io::stderr().is_terminal(),
        std::env::var_os("CI").is_some(),
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("TERM").ok().as_deref(),
    )
}

#[shallguard::enforces("REQ-CLI-003")]
fn color_enabled(interactive: bool, ci: bool, no_color: bool, term: Option<&str>) -> bool {
    interactive && !ci && !no_color && term != Some("dumb")
}

pub(crate) fn review_outcomes(message: &str, enabled: bool) -> Cow<'_, str> {
    if !enabled {
        return Cow::Borrowed(message);
    }

    let mut styled = message.to_string();
    for (outcome, color) in [
        ("insufficient_evidence", YELLOW),
        ("insufficient evidence", YELLOW),
        ("not_impacted", CYAN),
        ("not impacted", CYAN),
        ("satisfied", GREEN),
        ("violated", RED),
    ] {
        styled = styled.replace(outcome, &format!("{color}{outcome}{RESET}"));
    }
    Cow::Owned(styled)
}

pub(crate) fn section(message: &str, enabled: bool) -> Cow<'_, str> {
    style(message, BOLD_BLUE, enabled)
}

pub(crate) fn label(message: &str, enabled: bool) -> Cow<'_, str> {
    style(message, BOLD, enabled)
}

pub(crate) fn identifier(message: &str, enabled: bool) -> Cow<'_, str> {
    style(message, MAGENTA, enabled)
}

pub(crate) fn path(message: &str, enabled: bool) -> Cow<'_, str> {
    style(message, CYAN, enabled)
}

pub(crate) fn review_status(message: &str, enabled: bool) -> Cow<'_, str> {
    let color = match message {
        "completed" => GREEN,
        "running" | "pending" | "insufficient_evidence" => YELLOW,
        "violated" | "unavailable" | "invalid" => RED,
        "satisfied" => GREEN,
        "not_impacted" => CYAN,
        _ => BOLD,
    };
    style(message, color, enabled)
}

pub(crate) fn severity(message: &str, enabled: bool) -> Cow<'_, str> {
    let color = match message {
        "critical" | "high" => RED,
        "medium" => YELLOW,
        "low" => CYAN,
        "note" => DIM,
        _ => BOLD,
    };
    style(message, color, enabled)
}

fn style<'a>(message: &'a str, color: &str, enabled: bool) -> Cow<'a, str> {
    if enabled {
        Cow::Owned(format!("{color}{message}{RESET}"))
    } else {
        Cow::Borrowed(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shallguard::verifies("REQ-CLI-003")]
    #[test]
    fn color_requires_an_interactive_non_ci_terminal() {
        assert!(color_enabled(true, false, false, Some("xterm-256color")));
        assert!(!color_enabled(false, false, false, Some("xterm")));
        assert!(!color_enabled(true, true, false, Some("xterm")));
        assert!(!color_enabled(true, false, true, Some("xterm")));
        assert!(!color_enabled(true, false, false, Some("dumb")));

        let message = "1 satisfied, 1 violated, 2 insufficient evidence, 1 not impacted";
        assert!(review_outcomes(message, true).contains("\x1b[32msatisfied\x1b[0m"));
        assert_eq!(review_outcomes(message, false), message);
    }
}
