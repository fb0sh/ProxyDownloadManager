use crate::types::*;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    fn db_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".ProxyDM/state/downloads.db")
    }

    pub fn new() -> Result<Self, String> {
        let path = Self::db_path();
        Self::from_path(&path)
    }

    /// Test helper — create Db at specific path
    fn from_path(path: &std::path::Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
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
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, url, file_name, save_path, total_size, downloaded, status, last_try,
                        created_at, proxy_name, connections, parts, resumable FROM downloads",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let id: u64 = row.get(0)?;
                let url: String = row.get(1)?;
                let file_name: String = row.get(2)?;
                let save_path: String = row.get(3)?;
                let total_size: u64 = row.get(4)?;
                let downloaded: u64 = row.get(5)?;
                let status_str: String = row.get(6)?;
                let last_try: String = row.get(7)?;
                let created_at: String = row.get(8)?;
                let proxy_name: String = row.get(9)?;
                let connections: u32 = row.get(10)?;
                let parts_str: String = row.get(11)?;
                let resumable: Option<i32> = row.get(12)?;

                let parts: Vec<DownloadPart> = serde_json::from_str(&parts_str).unwrap_or_default();
                let status = parse_status(&status_str);

                Ok(DownloadItem {
                    id,
                    url,
                    file_name,
                    save_path,
                    total_size,
                    downloaded,
                    status,
                    parts,
                    proxy_name,
                    connections,
                    resumable: resumable.map(|v| v != 0),
                    merge_progress: 0.0,
                    created_at,
                    last_try,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| e.to_string())?);
        }
        Ok(items)
    }

    pub fn insert_download(&self, item: &DownloadItem) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_download(&self, item: &DownloadItem) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_download(&self, id: u64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM downloads WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn replace_all(&self, items: &[DownloadItem]) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM downloads", [])
            .map_err(|e| e.to_string())?;
        for item in items {
            let parts_str = serde_json::to_string(&item.parts).map_err(|e| e.to_string())?;
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
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn parse_status(s: &str) -> DownloadStatus {
    match s {
        "downloading" => DownloadStatus::Downloading,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        s if s.starts_with("failed") => {
            DownloadStatus::Failed(s[7..].trim_start_matches(':').to_string())
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
            merge_progress: 0.0,
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
    fn test_replace_all() {
        let db = test_db();
        db.insert_download(&sample_item(1)).unwrap();
        let new_items = vec![sample_item(2), sample_item(3)];
        db.replace_all(&new_items).unwrap();
        let items = db.list_downloads().unwrap();
        assert_eq!(items.len(), 2);
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
}
