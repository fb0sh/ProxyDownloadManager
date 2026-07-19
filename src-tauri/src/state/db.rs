use crate::types::*;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

/// All columns in the downloads table, in order. Used for SELECT queries and row mapping.
const COLUMNS: &str = "id, url, file_name, save_path, total_size, downloaded, status, last_try, \
                        created_at, proxy_name, connections, parts, resumable";

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<DownloadItem> {
    let parts_str: String = row.get("parts")?;
    let status_str: String = row.get("status")?;
    let resumable: Option<i32> = row.get("resumable")?;

    Ok(DownloadItem {
        id: row.get("id")?,
        url: row.get("url")?,
        file_name: row.get("file_name")?,
        save_path: row.get("save_path")?,
        total_size: row.get("total_size")?,
        downloaded: row.get("downloaded")?,
        status: parse_status(&status_str),
        parts: serde_json::from_str(&parts_str).unwrap_or_default(),
        proxy_name: row.get("proxy_name")?,
        connections: row.get("connections")?,
        resumable: resumable.map(|v| v != 0),
        created_at: row.get("created_at")?,
        last_try: row.get("last_try")?,
    })
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    fn db_path() -> PathBuf {
        let home = crate::state::gob::state_dir().parent()
            .unwrap_or(&std::path::PathBuf::from("."))
            .to_path_buf();
        home.join("state/downloads.db")
    }

    pub fn new() -> PdmResult<Self> {
        let path = Self::db_path();
        Self::from_path(&path)
    }

    /// Test helper — create Db at specific path
    pub(crate) fn from_path(path: &std::path::Path) -> PdmResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(PdmError::from)?;
        }
        let conn = Connection::open(path).map_err(PdmError::from)?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> PdmResult<()> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
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
                parts TEXT NOT NULL DEFAULT '[]',
                resumable INTEGER
            );",
        )
        .map_err(PdmError::from)?;
        Ok(())
    }

    /// Return the maximum download ID in the database, or 0 if empty.
    /// Used to initialize the worker pool's ID counter across restarts.
    pub fn max_id(&self) -> PdmResult<u64> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        let max: u64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM downloads", [], |row| row.get(0))
            .map_err(PdmError::from)?;
        Ok(max)
    }

    pub fn list_downloads(&self) -> PdmResult<Vec<DownloadItem>> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM downloads", COLUMNS))
            .map_err(PdmError::from)?;

        let rows = stmt
            .query_map([], |row| row_to_item(row))
            .map_err(PdmError::from)?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(PdmError::from)?);
        }
        Ok(items)
    }

    pub fn insert_download(&self, item: &DownloadItem) -> PdmResult<()> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        let parts_str = serde_json::to_string(&item.parts).map_err(PdmError::from)?;
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, save_path, total_size, downloaded, status, last_try,
                                    created_at, proxy_name, connections, parts, resumable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item.id,
                item.url,
                item.file_name,
                item.save_path,
                item.total_size,
                item.downloaded,
                status_to_string(&item.status),
                item.last_try,
                item.created_at,
                item.proxy_name,
                item.connections,
                parts_str,
                item.resumable.map(|v| if v { 1 } else { 0 }),
            ],
        )
        .map_err(PdmError::from)?;
        Ok(())
    }

    pub fn get_by_id(&self, id: u64) -> PdmResult<Option<DownloadItem>> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM downloads WHERE id=?1", COLUMNS))
            .map_err(PdmError::from)?;

        let mut rows = stmt.query_map(params![id], |row| row_to_item(row)).map_err(PdmError::from)?;

        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(e)) => Err(PdmError::from(e)),
            None => Ok(None),
        }
    }

    /// Lightweight: only update the `downloaded` field (used for progress flushes).
    pub fn update_download_progress(&self, id: u64, downloaded: u64) -> PdmResult<()> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        conn.execute(
            "UPDATE downloads SET downloaded=?1 WHERE id=?2",
            params![downloaded, id],
        )
        .map_err(PdmError::from)?;
        Ok(())
    }

    pub fn update_download(&self, item: &DownloadItem) -> PdmResult<()> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        let parts_str = serde_json::to_string(&item.parts).map_err(PdmError::from)?;
        conn.execute(
            "UPDATE downloads SET url=?1, file_name=?2, save_path=?3, total_size=?4, downloaded=?5,
                                   status=?6, last_try=?7, created_at=?8, proxy_name=?9,
                                   connections=?10, parts=?11, resumable=?12
             WHERE id=?13",
            params![
                item.url,
                item.file_name,
                item.save_path,
                item.total_size,
                item.downloaded,
                status_to_string(&item.status),
                item.last_try,
                item.created_at,
                item.proxy_name,
                item.connections,
                parts_str,
                item.resumable.map(|v| if v { 1 } else { 0 }),
                item.id,
            ],
        )
        .map_err(PdmError::from)?;
        Ok(())
    }

    pub fn delete_download(&self, id: u64) -> PdmResult<()> {
        let conn = self.conn.lock().map_err(PdmError::from)?;
        conn.execute("DELETE FROM downloads WHERE id=?1", params![id])
            .map_err(PdmError::from)?;
        Ok(())
    }
}

fn parse_status(s: &str) -> DownloadStatus {
    match s {
        "downloading" => DownloadStatus::Downloading,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        s if s.starts_with("failed") => {
            let msg = s.strip_prefix("failed:").unwrap_or("");
            DownloadStatus::Failed(msg.to_string())
        }
        _ => DownloadStatus::Queued,
    }
}

fn status_to_string(s: &DownloadStatus) -> String {
    match s {
        DownloadStatus::Downloading => "downloading".to_string(),
        DownloadStatus::Paused => "paused".to_string(),
        DownloadStatus::Completed => "completed".to_string(),
        DownloadStatus::Failed(msg) => format!("failed:{}", msg),
        DownloadStatus::Queued => "queued".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_db() -> Db {
        let dir = std::env::temp_dir().join(format!("pdm_test_{}_{}", std::process::id(), rand()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.db");
        Db::from_path(&path).expect("Failed to create test DB")
    }

    fn rand() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    fn sample_item(id: u64) -> DownloadItem {
        DownloadItem {
            id,
            url: format!("https://example.com/file{}.zip", id),
            file_name: format!("file{}.zip", id),
            save_path: format!("/tmp/file{}.zip", id),
            total_size: 1000,
            downloaded: 0,
            status: DownloadStatus::Queued,
            parts: vec![],
            proxy_name: "".to_string(),
            connections: 4,
            resumable: Some(true),
            created_at: "2026-01-01".to_string(),
            last_try: "".to_string(),
        }
    }

    #[test]
    fn test_insert_and_list() {
        let db = test_db();
        let item = sample_item(1);
        db.insert_download(&item).unwrap();
        let items = db.list_downloads().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].file_name, "file1.zip");
    }

    #[test]
    fn test_update() {
        let db = test_db();
        let mut item = sample_item(1);
        db.insert_download(&item).unwrap();
        item.downloaded = 500;
        item.status = DownloadStatus::Downloading;
        db.update_download(&item).unwrap();
        let items = db.list_downloads().unwrap();
        assert_eq!(items[0].downloaded, 500);
    }

    #[test]
    fn test_delete() {
        let db = test_db();
        db.insert_download(&sample_item(1)).unwrap();
        db.insert_download(&sample_item(2)).unwrap();
        db.delete_download(1).unwrap();
        let items = db.list_downloads().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 2);
    }

    #[test]
    fn test_status_persistence() {
        let db = test_db();
        let mut item = sample_item(1);
        item.status = DownloadStatus::Failed("timeout".to_string());
        db.insert_download(&item).unwrap();
        let items = db.list_downloads().unwrap();
        assert_eq!(format!("{:?}", items[0].status), "Failed(\"timeout\")");
    }

    #[test]
    fn test_empty_list() {
        let db = test_db();
        let items = db.list_downloads().unwrap();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_delete_nonexistent() {
        let db = test_db();
        db.delete_download(999).unwrap();
    }

    #[test]
    fn test_status_roundtrip_all_variants() {
        let statuses = vec![
            DownloadStatus::Downloading,
            DownloadStatus::Paused,
            DownloadStatus::Completed,
            DownloadStatus::Queued,
            DownloadStatus::Failed("connection refused".to_string()),
            DownloadStatus::Failed("".to_string()),
        ];
        for status in &statuses {
            let db = test_db();
            let mut item = sample_item(1);
            item.status = status.clone();
            db.insert_download(&item).unwrap();
            let items = db.list_downloads().unwrap();
            assert!(format!("{:?}", items[0].status) == format!("{:?}", status),
                "Roundtrip failed for {:?}", status);
        }
    }

    #[test]
    fn test_parse_status_handles_edge_cases() {
        assert!(matches!(parse_status("downloading"), DownloadStatus::Downloading));
        assert!(matches!(parse_status("paused"), DownloadStatus::Paused));
        assert!(matches!(parse_status("completed"), DownloadStatus::Completed));
        assert!(matches!(parse_status("queued"), DownloadStatus::Queued));
        assert!(matches!(parse_status("failed:timeout"), DownloadStatus::Failed(msg) if msg == "timeout"));
        assert!(matches!(parse_status("failed:"), DownloadStatus::Failed(msg) if msg == ""));
        assert!(matches!(parse_status("failed"), DownloadStatus::Failed(msg) if msg == ""));
        assert!(matches!(parse_status("unknown"), DownloadStatus::Queued));
    }
}
