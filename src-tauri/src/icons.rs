// =============================================================================
// icons.rs — System file icon cache (macOS NSWorkspace / fallback)
// Following the pattern from the egui reference implementation.
// =============================================================================

use base64::Engine;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct IconCache {
    cache: Mutex<HashMap<String, IconData>>,
    #[cfg(target_os = "macos")]
    temp_dir: std::path::PathBuf,
}

#[derive(Clone, serde::Serialize)]
pub struct IconData {
    /// Base64-encoded raw RGBA pixel data (not PNG)
    pub rgba: String,
    pub width: u32,
    pub height: u32,
}

impl IconCache {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let temp_dir = {
            let d = std::env::temp_dir().join("proxydm_icons");
            let _ = std::fs::create_dir_all(&d);
            d
        };

        Self {
            cache: Mutex::new(HashMap::new()),
            #[cfg(target_os = "macos")]
            temp_dir,
        }
    }

    pub fn get(&self, file_name: &str) -> IconData {
        let ext = file_name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let ext_key = if ext.is_empty() {
            "generic".to_string()
        } else {
            ext
        };

        // Check cache
        {
            let cache = self.cache.lock().unwrap();
            if let Some(data) = cache.get(&ext_key) {
                return data.clone();
            }
        }

        let data = self.load_icon(&ext_key);

        // Cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(ext_key, data.clone());
        }

        data
    }

    fn load_icon(&self, ext_key: &str) -> IconData {
        #[cfg(target_os = "macos")]
        {
            let temp_file = self.temp_dir.join(format!("icon.{}", ext_key));
            let _ = std::fs::write(&temp_file, b"");

            let result = file_icon_provider::get_file_icon(&temp_file, 32);
            let _ = std::fs::remove_file(&temp_file);

            if let Ok(icon) = result {
                let rgba = base64::engine::general_purpose::STANDARD.encode(&icon.pixels);
                return IconData {
                    rgba,
                    width: icon.width,
                    height: icon.height,
                };
            }
        }

        self.fallback_icon()
    }

    fn fallback_icon(&self) -> IconData {
        // Hand-drawn 32x32 gray document icon (matches reference implementation)
        let size = 32usize;
        let mut pixels = vec![200u8; size * size * 4];
        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) * 4;
                let is_border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
                let folded = x >= size - 8 && y < 8 && (x - (size - 8)) < (8 - y) + 3;
                if is_border || folded {
                    // Gray border/fold: RGB(180, 180, 180)
                    pixels[idx] = 180;
                    pixels[idx + 1] = 180;
                    pixels[idx + 2] = 180;
                    pixels[idx + 3] = 255;
                } else {
                    // Transparent interior
                    pixels[idx + 3] = 0;
                }
            }
        }
        let rgba = base64::engine::general_purpose::STANDARD.encode(&pixels);
        IconData { rgba, width: 32, height: 32 }
    }
}
