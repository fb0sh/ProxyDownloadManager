// =============================================================================
// icons.rs — System file icon cache (macOS NSWorkspace / fallback)
// =============================================================================

use eframe::egui::{self, ColorImage, TextureHandle};
use std::collections::HashMap;

pub struct IconCache {
    cache: HashMap<String, TextureHandle>,
    #[cfg(target_os = "macos")]
    temp_dir: std::path::PathBuf,
}

impl IconCache {
    pub fn new(_ctx: &egui::Context) -> Self {
        #[cfg(target_os = "macos")]
        let temp_dir = {
            let d = std::env::temp_dir().join("proxydm_icons");
            let _ = std::fs::create_dir_all(&d);
            d
        };

        Self {
            cache: HashMap::new(),
            #[cfg(target_os = "macos")]
            temp_dir,
        }
    }

    pub fn get_icon(&mut self, file_name: &str, ctx: &egui::Context) -> TextureHandle {
        let ext = file_name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let ext_key = if ext.is_empty() { "generic".to_string() } else { ext };

        if !self.cache.contains_key(&ext_key) {
            #[cfg(target_os = "macos")]
            {
                let temp_file = self.temp_dir.join(format!("icon.{}", ext_key));
                if !temp_file.exists() {
                    let _ = std::fs::write(&temp_file, b"");
                }
                let icon = file_icon_provider::get_file_icon(&temp_file, 32);
                if let Ok(fp_icon) = icon {
                    let color_image = ColorImage::from_rgba_unmultiplied(
                        [fp_icon.width as usize, fp_icon.height as usize],
                        &fp_icon.pixels,
                    );
                    let texture = ctx.load_texture(
                        format!("file_icon_{}", ext_key),
                        color_image,
                        egui::TextureOptions::NEAREST,
                    );
                    self.cache.insert(ext_key.clone(), texture);
                } else {
                    let fallback = Self::create_fallback_texture(ctx, &ext_key);
                    self.cache.insert(ext_key.clone(), fallback);
                }
                let _ = std::fs::remove_file(&temp_file);
            }

            #[cfg(not(target_os = "macos"))]
            {
                let fallback = Self::create_fallback_texture(ctx, &ext_key);
                self.cache.insert(ext_key.clone(), fallback);
            }
        }

        self.cache
            .get(&ext_key)
            .cloned()
            .unwrap_or_else(|| Self::create_fallback_texture(ctx, &ext_key))
    }

    fn create_fallback_texture(ctx: &egui::Context, _label: &str) -> TextureHandle {
        let size = 32;
        let mut pixels = vec![200u8; size as usize * size as usize * 4];
        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) as usize * 4;
                let is_border = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
                let folded = x >= size - 8 && y < 8 && (x - (size - 8)) < (8 - y) + 3;
                if is_border || folded {
                    pixels[idx] = 180;
                    pixels[idx + 1] = 180;
                    pixels[idx + 2] = 180;
                    pixels[idx + 3] = 255;
                } else {
                    pixels[idx + 3] = 0;
                }
            }
        }
        let color_image = ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &pixels);
        ctx.load_texture(
            format!("icon_fallback_{}", _label),
            color_image,
            egui::TextureOptions::NEAREST,
        )
    }
}
