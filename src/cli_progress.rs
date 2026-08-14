//! Terminal-aware rendering for command progress.

use std::io::{IsTerminal, Write};

use req_trace::ProgressEvent;

pub(crate) fn print_progress(event: ProgressEvent<'_>) {
    let stderr = std::io::stderr();
    let interactive = stderr.is_terminal();
    let mut stderr = stderr.lock();

    match event {
        ProgressEvent::Message(message) => {
            clear_line(&mut stderr, interactive);
            let _ = writeln!(stderr, "[req-cov] {message}");
        }
        ProgressEvent::LiveStatus {
            message,
            log_when_redirected: _,
        } if interactive => {
            clear_line(&mut stderr, true);
            let _ = write!(stderr, "[req-cov] {message}");
            let _ = stderr.flush();
        }
        ProgressEvent::LiveStatus {
            message,
            log_when_redirected: true,
        } => {
            let _ = writeln!(stderr, "[req-cov] {message}");
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
