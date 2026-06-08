// =============================================================================
// persist.rs — Persistence: SQLite (downloads) + TOML (settings)
// =============================================================================

use crate::types::*;
use std::fs;

// ─── TOML helpers ─────────────────────────────────────────────────────────────

pub fn load_toml<T: serde::de::DeserializeOwned>(path: &str) -> Option<T> {
    fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok())
}

pub fn save_toml<T: serde::Serialize>(path: &str, value: &T) {
    if let Ok(t) = toml::to_string_pretty(value) {
        let _ = fs::write(path, &t);
    }
}

// ─── Status string conversions (SQLite compat) ────────────────────────────────

pub fn status_to_string(s: &DownloadStatus) -> String {
    match s {
        DownloadStatus::Downloading => "downloading".into(),
        DownloadStatus::Paused => "paused".into(),
        DownloadStatus::Completed => "completed".into(),
        DownloadStatus::Failed(msg) => format!("failed:{}", msg),
        DownloadStatus::Queued => "queued".into(),
    }
}

pub fn status_from_string(s: &str) -> DownloadStatus {
    if let Some(msg) = s.strip_prefix("failed:") {
        DownloadStatus::Failed(msg.to_string())
    } else {
        match s {
            "downloading" => DownloadStatus::Downloading,
            "paused" => DownloadStatus::Paused,
            "completed" => DownloadStatus::Completed,
            "queued" => DownloadStatus::Queued,
            _ => DownloadStatus::Failed(format!("unknown: {}", s)),
        }
    }
}

// ─── SQLite database ──────────────────────────────────────────────────────────

fn init_db(path: &str) -> rusqlite::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL,
            file_name TEXT NOT NULL,
            save_path TEXT NOT NULL,
            total_size INTEGER NOT NULL DEFAULT 0,
            downloaded INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'queued',
            last_try TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            proxy_name TEXT NOT NULL DEFAULT '',
            connections INTEGER NOT NULL DEFAULT 4,
            parts TEXT NOT NULL DEFAULT '[]'
        );
    ")?;

    // Migration: add resumable column if missing
    let has_column: bool = conn.prepare("SELECT resumable FROM downloads LIMIT 0").is_ok();
    if !has_column {
        let _ = conn.execute_batch("ALTER TABLE downloads ADD COLUMN resumable INTEGER;");
    }

    Ok(conn)
}

pub fn load_downloads(path: &str) -> Vec<DownloadItem> {
    let conn = match init_db(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT id, url, file_name, save_path, total_size, downloaded, status, last_try, created_at, proxy_name, connections, parts, resumable FROM downloads"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| {
        let status_str: String = row.get(6)?;
        let parts_str: String = row.get(11)?;
        let parts: Vec<DownloadPart> = serde_json::from_str(&parts_str).unwrap_or_default();
        let resumable: Option<i32> = row.get(12).ok();
        Ok(DownloadItem {
            id: row.get(0)?,
            url: row.get(1)?,
            file_name: row.get(2)?,
            save_path: row.get(3)?,
            total_size: row.get(4)?,
            downloaded: row.get(5)?,
            status: status_from_string(&status_str),
            last_try: row.get(7)?,
            created_at: row.get(8)?,
            proxy_name: row.get(9)?,
            connections: row.get(10)?,
            parts,
            resumable: resumable.map(|v| v != 0),
            merge_progress: 0.0,
        })
    });
    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn save_downloads(path: &str, items: &[DownloadItem]) {
    let conn = match init_db(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Err(e) = conn.execute("DELETE FROM downloads", []) {
        eprintln!("Failed to clear downloads: {}", e);
        return;
    }
    let mut stmt = match conn.prepare(
        "INSERT INTO downloads (id, url, file_name, save_path, total_size, downloaded, status, last_try, created_at, proxy_name, connections, parts, resumable) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
    ) {
        Ok(s) => s,
        Err(e) => { eprintln!("Prepare: {}", e); return; }
    };
    for item in items {
        let status_str = status_to_string(&item.status);
        let parts_str = serde_json::to_string(&item.parts).unwrap_or_else(|_| "[]".to_string());
        let _ = stmt.execute(rusqlite::params![
            item.id as i64,
            &item.url,
            &item.file_name,
            &item.save_path,
            item.total_size as i64,
            item.downloaded as i64,
            &status_str,
            &item.last_try,
            &item.created_at,
            &item.proxy_name,
            item.connections as i64,
            &parts_str,
            item.resumable.map(|r| if r { 1 } else { 0 }),
        ]);
    }
}
