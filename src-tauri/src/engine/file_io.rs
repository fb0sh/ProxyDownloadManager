use std::path::Path;

pub async fn create_output_file(path: &str, total_size: u64) -> Result<std::fs::File, String> {
    let pdm_path = format!("{}.pdm", path);
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
    let pdm_path = format!("{}.pdm", save_path);
    tokio::fs::rename(&pdm_path, save_path)
        .await
        .map_err(|e| format!("Failed to rename file: {}", e))
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
