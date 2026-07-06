use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Logger {
    file: Mutex<File>,
}

impl Logger {
    pub fn new() -> Result<Self, String> {
        let path = Self::log_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self { file: Mutex::new(file) })
    }

    fn log_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".ProxyDM/logs/proxydm.log")
    }

    pub fn log(&self, level: &str, msg: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let line = format!("[{}] [{}] {}\n", Self::fmt_time(secs), level, msg);
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }

    pub fn info(&self, msg: &str) { self.log("INFO", msg); }
    pub fn warn(&self, msg: &str) { self.log("WARN", msg); }
    pub fn error(&self, msg: &str) { self.log("ERROR", msg); }

    fn fmt_time(secs: u64) -> String {
        let remaining = secs % 86400;
        let hours = remaining / 3600;
        let minutes = (remaining % 3600) / 60;
        let seconds = remaining % 60;
        let days_since_epoch = secs / 86400;

        // Approximate date from Unix epoch using cumulative days per month
        let mut y = 1970i64;
        let mut remaining_days = days_since_epoch as i64;

        loop {
            let days_in_year = if is_leap(y) { 366 } else { 365 };
            if remaining_days < days_in_year { break; }
            remaining_days -= days_in_year;
            y += 1;
        }

        let month_days = if is_leap(y) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

        let mut m = 0;
        for &days_in_m in &month_days {
            if remaining_days < days_in_m { break; }
            remaining_days -= days_in_m;
            m += 1;
        }

        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            y, m + 1, remaining_days + 1, hours, minutes, seconds)
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn log_path_str() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".ProxyDM/logs/proxydm.log").to_string_lossy().to_string()
}

pub fn read_logs(max_lines: usize) -> Result<Vec<String>, String> {
    let path = log_path_str();
    if !std::path::Path::new(&path).exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let lines: Vec<String> = content.lines().rev().take(max_lines).map(|l| l.to_string()).collect();
    Ok(lines) // newest first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_can_construct() {
        // Logger::new() creates the file at ~/.ProxyDM/logs/proxydm.log
        // This is a smoke test - in isolation it should succeed
        let logger = Logger::new();
        assert!(logger.is_ok());
    }

    #[test]
    fn test_read_logs_no_error() {
        let result = read_logs(100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_path_ends_correctly() {
        let path = log_path_str();
        assert!(path.ends_with("proxydm.log"));
        assert!(path.contains(".ProxyDM"));
    }
}
