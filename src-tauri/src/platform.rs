use auto_launch::AutoLaunchBuilder;
use tauri::Manager;

pub fn sync_autostart(
    app: &tauri::AppHandle,
    launch_at_startup: bool,
    silent_startup: bool,
) -> Result<(), String> {
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(&app.package_info().name);

    let args = if silent_startup {
        vec![crate::SILENT_START_ARG]
    } else {
        Vec::new()
    };
    builder.set_args(&args);

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    {
        if let Some(appimage) = app.env().appimage.and_then(|p| p.to_str().map(|s| s.to_string())) {
            builder.set_app_path(&appimage);
        } else {
            builder.set_app_path(&current_exe.display().to_string());
        }
    }

    #[cfg(target_os = "macos")]
    {
        builder.set_use_launch_agent(true);
        let exe_path = current_exe
            .canonicalize()
            .map_err(|e| e.to_string())?
            .display()
            .to_string();
        builder.set_app_path(&exe_path);
    }

    #[cfg(target_os = "windows")]
    builder.set_app_path(&current_exe.display().to_string());

    let autostart = builder.build().map_err(|e| e.to_string())?;
    if launch_at_startup {
        autostart.enable().map_err(|e| e.to_string())
    } else {
        autostart.disable().map_err(|e| e.to_string())
    }
}

pub(crate) fn deploy_extensions(app: &tauri::AppHandle) -> Result<String, String> {
    let src_dir = if let Ok(resource_dir) = app.path().resource_dir() {
        let ext_dir = resource_dir.join("extensions");
        if ext_dir.exists() {
            ext_dir
        } else {
            let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(|p| p.join("browsers-extension"));
            match dev {
                Some(p) if p.exists() => p,
                _ => return Err("Extensions source directory not found".to_string()),
            }
        }
    } else {
        return Err("Cannot resolve resource directory".to_string());
    };

    #[cfg(target_os = "macos")]
    {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let target_dir = app_dir.join("extensions");

        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

            for entry in std::fs::read_dir(&src_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let src = entry.path();
                let dst = target_dir.join(&name);

                if src.is_dir() {
                    let _ = std::fs::remove_dir_all(&dst);
                    copy_dir_recursive(&src, &dst)?;
                } else {
                    let _ = std::fs::copy(&src, &dst);
                }
            }
        }

        return Ok(target_dir.to_string_lossy().to_string());
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(src_dir.to_string_lossy().to_string())
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            let _ = std::fs::copy(&src_path, &dst_path);
        }
    }
    Ok(())
}

pub fn resolve_extensions_dir(app: &tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(app_dir) = app.path().app_data_dir() {
            let ext_dir = app_dir.join("extensions");
            if ext_dir.exists() {
                return Ok(ext_dir.to_string_lossy().to_string());
            }
        }
        return deploy_extensions(app);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let ext_dir = resource_dir.join("extensions");
            if ext_dir.exists() {
                return Ok(ext_dir.to_string_lossy().to_string());
            }
        }

        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("browsers-extension"));

        if let Some(path) = dev_path {
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        Err("Extensions directory not found. The browser extensions may not have been bundled. Try reinstalling the application.".to_string())
    }
}
