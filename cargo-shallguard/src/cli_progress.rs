//! Terminal-aware rendering for command progress.

use std::io::{IsTerminal, Write};

use shallguard::ProgressEvent;

use crate::cli_color;

#[shallguard::enforces("REQ-CLI-003")]
pub(crate) fn print_progress(event: ProgressEvent<'_>) {
    let stderr = std::io::stderr();
    let interactive = stderr.is_terminal();
    let color = cli_color::stderr_enabled();
    let mut stderr = stderr.lock();

    match event {
        ProgressEvent::Message(message) => {
            clear_line(&mut stderr, interactive);
            let message = cli_color::review_outcomes(message, color);
            let _ = writeln!(stderr, "[shallguard] {message}");
        }
        ProgressEvent::LiveStatus {
            message,
            log_when_redirected: _,
        } if interactive => {
            clear_line(&mut stderr, true);
            let message = cli_color::review_outcomes(message, color);
            let _ = write!(stderr, "[shallguard] {message}");
            let _ = stderr.flush();
        }
        ProgressEvent::LiveStatus {
            message,
            log_when_redirected: true,
        } => {
            let message = cli_color::review_outcomes(message, color);
            let _ = writeln!(stderr, "[shallguard] {message}");
        }
        ProgressEvent::LiveStatus {
            log_when_redirected: false,
            ..
        } => {}
        ProgressEvent::ClearLiveStatus => {
            clear_line(&mut stderr, interactive);
            let _ = stderr.flush();
        }
    }
}

fn clear_line(stderr: &mut impl Write, interactive: bool) {
    if interactive {
        let _ = write!(stderr, "\r\x1b[2K");
    }
}
