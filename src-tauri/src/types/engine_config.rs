use crate::types::{DownloadItem, DownloadState, Task};
use std::collections::HashMap;

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
}

impl DownloadItem {
    pub fn to_engine_config(
        &self,
        proxy_url: &str,
        user_agent: &str,
        is_resume: bool,
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
        EngineConfig {
            url: self.url.clone(),
            save_path: self.save_path.clone(),
            id: self.id,
            file_name: self.file_name.clone(),
            is_resume,
            headers: HashMap::new(),
            proxy_url: proxy_url.to_string(),
            proxy_name: self.proxy_name.clone(),
            total_size: self.total_size,
            supports_range: self.resumable.unwrap_or(true),
            rate_limit_bps: 0,
            connections: self.connections,
            max_retries,
            user_agent: user_agent.to_string(),
            resume_tasks: vec![],
            downloaded: self.downloaded,
            part_ranges,
            part_downloaded,
        }
    }
}

impl DownloadState {
    pub fn to_engine_config(
        &self,
        proxy_url: &str,
        user_agent: &str,
        supports_range: bool,
        max_retries: u32,
    ) -> EngineConfig {
        EngineConfig {
            url: self.url.clone(),
            save_path: self.save_path.clone(),
            id: self.id,
            file_name: self.file_name.clone(),
            is_resume: true,
            headers: HashMap::new(),
            proxy_url: proxy_url.to_string(),
            proxy_name: self.proxy_name.clone(),
            total_size: self.total_size,
            supports_range,
            rate_limit_bps: 0,
            connections: self.workers,
            max_retries,
            user_agent: user_agent.to_string(),
            resume_tasks: self.tasks.clone(),
            downloaded: self.downloaded,
            part_ranges: if self.total_size > 0 {
                vec![(0, self.total_size)]
            } else {
                vec![]
            },
            part_downloaded: vec![],
        }
    }
}
