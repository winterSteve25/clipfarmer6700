//! Human-readable progress output for long-running CLI operations.
//!
//! Interactive terminals get animated progress bars. Redirected stderr keeps stable line-oriented
//! output, and stdout remains available for machine-readable command results.
//! These helpers keep operational feedback separate from the backend's domain and persistence logic.

use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

const BAR_WIDTH: usize = 20;
const TICK: Duration = Duration::from_millis(80);

pub struct Step {
    label: String,
    started_at: Instant,
    animation: Option<Animation>,
    finished: bool,
}

impl Step {
    pub fn start(label: impl Into<String>) -> Self {
        let label = label.into();
        let started_at = Instant::now();
        let animation = if interactive() {
            let animated_label = label.clone();
            Some(Animation::start(move |frame| {
                draw_active(&animated_label, frame, &elapsed(started_at.elapsed()));
            }))
        } else {
            eprintln!("  … {label}");
            None
        };
        Self {
            label,
            started_at,
            animation,
            finished: false,
        }
    }

    pub fn done(mut self, detail: impl AsRef<str>) {
        self.stop_animation();
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
        self.stop_animation();
        self.finished = true;
        eprintln!("  ↷ {} — {}", self.label, reason.as_ref());
    }

    pub fn failed(mut self, reason: impl AsRef<str>) {
        self.stop_animation();
        self.finished = true;
        eprintln!(
            "  ✗ {} — {} ({})",
            self.label,
            reason.as_ref(),
            elapsed(self.started_at.elapsed())
        );
    }

    fn stop_animation(&mut self) {
        if let Some(mut animation) = self.animation.take() {
            animation.stop();
        }
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if !self.finished {
            self.stop_animation();
            eprintln!(
                "  ✗ {} — stopped after {}",
                self.label,
                elapsed(self.started_at.elapsed())
            );
        }
    }
}

/// An indeterminate download bar that reports the growing file's byte count and transfer rate.
/// Streamlink and chat_downloader do not expose a reliable total size before downloading.
pub struct ByteProgress {
    label: String,
    path: PathBuf,
    started_at: Instant,
    animation: Option<Animation>,
    finished: bool,
}

impl ByteProgress {
    pub fn start(label: impl Into<String>, path: impl AsRef<Path>) -> Self {
        let label = label.into();
        let path = path.as_ref().to_owned();
        let started_at = Instant::now();
        let animation = if interactive() {
            let animated_label = label.clone();
            let watched_path = path.clone();
            Some(Animation::start(move |frame| {
                let transferred = file_size(&watched_path);
                let seconds = started_at.elapsed().as_secs_f64().max(0.001);
                let rate = transferred as f64 / seconds;
                draw_active(
                    &animated_label,
                    frame,
                    &format!(
                        "{} • {}/s • {}",
                        bytes(transferred),
                        bytes(rate as u64),
                        elapsed(started_at.elapsed())
                    ),
                );
            }))
        } else {
            eprintln!("  … {label}");
            None
        };
        Self {
            label,
            path,
            started_at,
            animation,
            finished: false,
        }
    }

    pub fn done(mut self, detail: impl AsRef<str>) {
        let transferred = file_size(&self.path);
        self.finish(transferred, detail.as_ref());
    }

    pub fn done_with_size(mut self, transferred: u64, detail: impl AsRef<str>) {
        self.finish(transferred, detail.as_ref());
    }

    fn finish(&mut self, transferred: u64, detail: &str) {
        self.stop_animation();
        self.finished = true;
        eprintln!(
            "  ✓ {} — {}{} ({})",
            self.label,
            bytes(transferred),
            if detail.is_empty() {
                String::new()
            } else {
                format!(" • {detail}")
            },
            elapsed(self.started_at.elapsed())
        );
    }

    pub fn failed(mut self, reason: impl AsRef<str>) {
        let transferred = file_size(&self.path);
        self.stop_animation();
        self.finished = true;
        eprintln!(
            "  ✗ {} — {} downloaded • {} ({})",
            self.label,
            bytes(transferred),
            reason.as_ref(),
            elapsed(self.started_at.elapsed())
        );
    }

    fn stop_animation(&mut self) {
        if let Some(mut animation) = self.animation.take() {
            animation.stop();
        }
    }
}

impl Drop for ByteProgress {
    fn drop(&mut self) {
        if !self.finished {
            let transferred = file_size(&self.path);
            self.stop_animation();
            eprintln!(
                "  ✗ {} — stopped after {} at {}",
                self.label,
                elapsed(self.started_at.elapsed()),
                bytes(transferred)
            );
        }
    }
}

struct Animation {
    state: Arc<(Mutex<bool>, Condvar)>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Animation {
    fn start(mut draw: impl FnMut(usize) + Send + 'static) -> Self {
        let state = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            let mut frame = 0;
            loop {
                draw(frame);
                frame = frame.wrapping_add(1);
                let (lock, wake) = &*worker_state;
                let Ok(stopped) = lock.lock() else {
                    break;
                };
                if *stopped {
                    break;
                }
                let Ok((stopped, _)) = wake.wait_timeout(stopped, TICK) else {
                    break;
                };
                if *stopped {
                    break;
                }
            }
        });
        Self {
            state,
            handle: Some(handle),
        }
    }

    fn stop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let (lock, wake) = &*self.state;
        if let Ok(mut stopped) = lock.lock() {
            *stopped = true;
            wake.notify_one();
        }
        let _ = handle.join();
        clear_active();
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        self.stop();
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

pub fn bytes(bytes: u64) -> String {
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn draw_active(label: &str, frame: usize, status: &str) {
    let bar = pulse(frame);
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2K  [{bar}] {label} • {status}");
    let _ = stderr.flush();
}

fn clear_active() {
    if interactive() {
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[2K");
        let _ = stderr.flush();
    }
}

fn pulse(frame: usize) -> String {
    const PULSE_WIDTH: usize = 4;
    let travel = BAR_WIDTH - PULSE_WIDTH;
    let cycle = travel * 2;
    let offset = frame % cycle;
    let start = if offset <= travel {
        offset
    } else {
        cycle - offset
    };
    (0..BAR_WIDTH)
        .map(|index| {
            if (start..start + PULSE_WIDTH).contains(&index) {
                '█'
            } else {
                '░'
            }
        })
        .collect()
}

fn interactive() -> bool {
    io::stderr().is_terminal()
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
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

    #[test]
    fn formats_byte_counts() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1_536), "1.5 KiB");
        assert_eq!(bytes(2_097_152), "2.0 MiB");
    }

    #[test]
    fn pulse_is_a_fixed_width_bouncing_bar() {
        assert_eq!(pulse(0).chars().count(), BAR_WIDTH);
        assert_eq!(pulse(0).matches('█').count(), 4);
        assert_ne!(pulse(0), pulse(5));
        assert_eq!(pulse(0), pulse((BAR_WIDTH - 4) * 2));
    }
}
