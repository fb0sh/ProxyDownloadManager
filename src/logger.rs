// =============================================================================
// logger.rs — Simple file logger to ~/.pdm/logs/proxydm.log
// =============================================================================

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

use crate::types::pdm_dir;

static LOGGER: Mutex<Option<LoggerInner>> = Mutex::new(None);

struct LoggerInner {
    file: std::fs::File,
    buf: Vec<u8>,
}

fn ensure_init() {
    if let Ok(mut guard) = LOGGER.lock() {
        if guard.is_none() {
            let log_dir = pdm_dir().join("logs");
            let _ = fs::create_dir_all(&log_dir);
            let path = log_dir.join("proxydm.log");
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok();
            *guard = file.map(|f| LoggerInner {
                file: f,
                buf: Vec::with_capacity(4096),
            });
        }
    }
}

/// Log a formatted message with timestamp.
pub fn log(msg: std::fmt::Arguments<'_>) {
    ensure_init();
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let formatted = format!("[{}] {}", ts, msg);
    if let Ok(mut guard) = LOGGER.lock() {
        if let Some(ref mut inner) = *guard {
            inner.buf.extend_from_slice(formatted.as_bytes());
            inner.buf.push(b'\n');
            if inner.buf.len() > 2048 {
                let _ = inner.file.write_all(&inner.buf);
                let _ = inner.file.flush();
                inner.buf.clear();
            }
        }
    }
}

/// Log a simple string message.
pub fn log_str(msg: &str) {
    log(format_args!("{}", msg));
}

/// Convenience macro for logging.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log(format_args!($($arg)*))
    };
}

/// Flush buffered log entries to disk.
pub fn flush() {
    if let Ok(mut guard) = LOGGER.lock() {
        if let Some(ref mut inner) = *guard {
            let _ = inner.file.write_all(&inner.buf);
            let _ = inner.file.flush();
            inner.buf.clear();
        }
    }
}
