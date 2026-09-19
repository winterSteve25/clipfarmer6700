//! Human-readable progress output for long-running CLI operations.
//!
//! Progress is written to stderr so command results printed to stdout (including JSON) remain
//! machine-readable.

use std::time::{Duration, Instant};

pub struct Step {
    label: String,
    started_at: Instant,
    finished: bool,
}

impl Step {
    pub fn start(label: impl Into<String>) -> Self {
        let label = label.into();
        eprintln!("  … {label}");
        Self {
            label,
            started_at: Instant::now(),
            finished: false,
        }
    }

    pub fn done(mut self, detail: impl AsRef<str>) {
        self.finished = true;
        let detail = detail.as_ref();
        if detail.is_empty() {
            eprintln!(
                "  ✓ {} ({})",
                self.label,
                elapsed(self.started_at.elapsed())
            );
        } else {
            eprintln!(
                "  ✓ {} — {} ({})",
                self.label,
                detail,
                elapsed(self.started_at.elapsed())
            );
        }
    }

    pub fn skipped(mut self, reason: impl AsRef<str>) {
        self.finished = true;
        eprintln!("  ↷ {} — {}", self.label, reason.as_ref());
    }

    pub fn failed(mut self, reason: impl AsRef<str>) {
        self.finished = true;
        eprintln!(
            "  ✗ {} — {} ({})",
            self.label,
            reason.as_ref(),
            elapsed(self.started_at.elapsed())
        );
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if !self.finished {
            eprintln!(
                "  ✗ {} — stopped after {}",
                self.label,
                elapsed(self.started_at.elapsed())
            );
        }
    }
}

pub fn section(title: impl AsRef<str>) {
    eprintln!("\n▶ {}", title.as_ref());
}

pub fn info(message: impl AsRef<str>) {
    eprintln!("  • {}", message.as_ref());
}

pub fn success(message: impl AsRef<str>) {
    eprintln!("  ✓ {}", message.as_ref());
}

pub fn warning(message: impl AsRef<str>) {
    eprintln!("  ! {}", message.as_ref());
}

pub fn timestamp(milliseconds: i64) -> String {
    let total_seconds = milliseconds.max(0) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn elapsed(duration: Duration) -> String {
    if duration.as_secs() >= 60 {
        format!(
            "{}m {:02}s",
            duration.as_secs() / 60,
            duration.as_secs() % 60
        )
    } else if duration.as_secs() >= 1 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_media_timestamps() {
        assert_eq!(timestamp(0), "00:00");
        assert_eq!(timestamp(65_999), "01:05");
        assert_eq!(timestamp(3_661_000), "01:01:01");
    }
}
