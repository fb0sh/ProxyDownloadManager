use std::path::Path;

/// The engine's temp-file convention: data streams into `{save_path}.pdm` and
/// is renamed into place on completion. This is the single owner of that
/// knowledge — callers never format the suffix themselves.
pub fn pdm_path(save_path: &str) -> String {
    format!("{}.pdm", save_path)
}

/// Delete a download's files — final and temp — so callers (delete flow)
/// don't need to know the temp convention.
pub fn remove_download_files(save_path: &str) {
    let _ = std::fs::remove_file(pdm_path(save_path));
    let _ = std::fs::remove_file(save_path);
}

pub async fn create_output_file(path: &str, total_size: u64) -> Result<std::fs::File, String> {
    let pdm_path = pdm_path(path);
    if let Some(parent) = Path::new(&pdm_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(&pdm_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    if total_size > 0 {
        let _ = file.set_len(total_size);
    }
    Ok(file)
}

pub async fn finalize_file(save_path: &str) -> Result<(), String> {
    let pdm_path = pdm_path(save_path);
    // Windows: antivirus/indexers briefly hold freshly written files, making
    // the rename fail with a sharing violation — retry with a short backoff.
    // The caller must have dropped its file handle before calling this.
    let attempts = if cfg!(windows) { 10 } else { 1 };
    let mut last_err = String::new();
    for i in 0..attempts {
        match tokio::fs::rename(&pdm_path, save_path).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e.to_string();
                if i + 1 < attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    }
    Err(format!("Failed to rename file: {}", last_err))
}

/// Cross-platform write_at: write to a specific offset without seeking.
#[cfg(unix)]
pub fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    FileExt::write_all_at(file, buf, offset)
}

#[cfg(windows)]
pub fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut written = 0;
    while written < buf.len() {
        let n = FileExt::seek_write(file, &buf[written..], offset + written as u64)?;
        written += n;
    }
    Ok(())
}
