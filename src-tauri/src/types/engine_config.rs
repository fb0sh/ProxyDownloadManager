use crate::types::{DownloadItem, Task};
use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

/// Engine-facing download configuration.
/// `proxy_url` is the resolved proxy URL (not a proxy name key).
/// `proxy_name` is the user-facing proxy name key (for saving to gob).
pub struct EngineConfig {
    pub url: String,
    pub save_path: String,
    pub id: u64,
    pub file_name: String,
    pub is_resume: bool,
    pub headers: HashMap<String, String>,
    pub proxy_url: String,
    pub proxy_name: String,
    pub total_size: u64,
    pub supports_range: bool,
    pub rate_limit_bps: u64,
    pub connections: u32,
    pub max_retries: u32,
    pub user_agent: String,
    pub resume_tasks: Vec<Task>,
    pub downloaded: u64,
    /// Fixed Progress Map ranges `(start, end)` planned once for this download.
    pub part_ranges: Vec<(u64, u64)>,
    /// Per-part downloaded bytes (aligned with `part_ranges`) for resume seeding.
    pub part_downloaded: Vec<u64>,
    /// Live worker target. `None` means use `connections`.
    pub desired_connections: Option<Arc<AtomicU32>>,
}

/// Everything the engine needs to continue a download, produced by the
/// progress ledger's `begin_resume`: fixed part ranges, per-part progress,
/// precise remaining tasks and the reconciled total — mutually consistent by
/// construction. The only path to a resume `EngineConfig`.
pub struct ResumePlan {
    pub item: DownloadItem,
    pub downloaded: u64,
    pub part_ranges: Vec<(u64, u64)>,
    pub part_downloaded: Vec<u64>,
    pub tasks: Vec<Task>,
}

impl DownloadItem {
    /// Complete config for a fresh download — no field needs caller patching.
    pub fn to_engine_config(
        &self,
        proxy_url: &str,
        user_agent: &str,
        _rate_limit_bps: u64,
        max_retries: u32,
    ) -> EngineConfig {
        let (part_ranges, part_downloaded) = if self.parts.is_empty() {
            if self.total_size > 0 {
                (vec![(0, self.total_size)], vec![self.downloaded])
            } else {
                (vec![], vec![])
            }
        } else {
            (
                self.parts.iter().map(|p| (p.start, p.end)).collect(),
                self.parts.iter().map(|p| p.downloaded).collect(),
            )
        };
        let headers = self.headers.clone();
        let ua = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| user_agent.to_string());
        EngineConfig {
            url: if self.final_url.is_empty() {
                self.url.clone()
            } else {
                self.final_url.clone()
            },
            save_path: self.save_path.clone(),
            id: self.id,
            file_name: self.file_name.clone(),
            is_resume: false,
            headers,
            proxy_url: proxy_url.to_string(),
            proxy_name: self.proxy_name.clone(),
            total_size: self.total_size,
            supports_range: self.resumable.unwrap_or(true),
            rate_limit_bps: self.rate_limit_bps,
            connections: self.connections,
            max_retries,
            user_agent: ua,
            resume_tasks: vec![],
            downloaded: self.downloaded,
            part_ranges,
            part_downloaded,
            desired_connections: None,
        }
    }
}

impl ResumePlan {
    /// Complete config for a resumed download — no field needs caller patching.
    pub fn into_engine_config(
        self,
        proxy_url: &str,
        user_agent: &str,
        _rate_limit_bps: u64,
        max_retries: u32,
    ) -> EngineConfig {
        let headers = self.item.headers.clone();
        let ua = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| user_agent.to_string());
        EngineConfig {
            url: if self.item.final_url.is_empty() {
                self.item.url
            } else {
                self.item.final_url
            },
            save_path: self.item.save_path,
            id: self.item.id,
            file_name: self.item.file_name,
            is_resume: true,
            headers,
            proxy_url: proxy_url.to_string(),
            proxy_name: self.item.proxy_name,
            total_size: self.item.total_size,
            supports_range: self.item.resumable.unwrap_or(true),
            rate_limit_bps: self.item.rate_limit_bps,
            connections: self.item.connections,
            max_retries,
            user_agent: ua,
            resume_tasks: self.tasks,
            downloaded: self.downloaded,
            part_ranges: self.part_ranges,
            part_downloaded: self.part_downloaded,
            desired_connections: None,
        }
    }
}
