use crate::types::*;
use crate::engine::chunk;

/// Plan download chunks based on file size and connection count.
pub struct ChunkPlan {
    pub connections: u32,
    pub parts: Vec<DownloadPart>,
}

pub fn plan_chunks(
    file_size: u64,
    requested_connections: u32,
    supports_range: bool,
    max_connections: u32,
) -> ChunkPlan {
    let connections = compute_connection_count(file_size, requested_connections, max_connections);

    let parts = if supports_range && file_size > 0 {
        let num_conns = if connections > 0 { connections.min(32) } else { 1 };
        let min_chunk = 2u64 * 1024 * 1024;
        let tasks = chunk::compute_chunks(file_size, num_conns, min_chunk);
        tasks.iter().enumerate().map(|(i, t)| DownloadPart {
            index: i as u32,
            start: t.offset,
            end: t.offset + t.length,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }).collect()
    } else {
        vec![DownloadPart {
            index: 0,
            start: 0,
            end: file_size,
            downloaded: 0,
            temp_path: String::new(),
            status: PartStatus::Pending,
            retries: 0,
        }]
    };

    ChunkPlan { connections, parts }
}

pub fn compute_connection_count(file_size: u64, requested: u32, max_connections: u32) -> u32 {
    let max_conns = max_connections.max(1).min(32);

    if requested > 0 {
        requested.min(32)
    } else if file_size == 0 {
        max_conns.min(2)
    } else if file_size < 100 * 1024 * 1024 {
        max_conns.min(2)
    } else if file_size < 1024 * 1024 * 1024 {
        max_conns.min(4)
    } else if file_size < 10u64 * 1024 * 1024 * 1024 {
        max_conns.min(8)
    } else {
        max_conns.min(16)
    }
}

/// Check if there's enough disk space for the download.
pub fn check_disk_space(path: &str, file_size: u64) -> PdmResult<()> {
    if file_size > 0 {
        let pdm_path = format!("{}.pdm", path);
        if let Some(parent) = std::path::Path::new(&pdm_path).parent() {
            if let Ok(available) = fs2::available_space(parent) {
                let needed = file_size + (2u64 * 1024 * 1024);
                if available < needed {
                    return Err(PdmError::Other(format!(
                        "Insufficient disk space: need {}, available {}", needed, available
                    )));
                }
            }
        }
    }
    Ok(())
}
