// =============================================================================
// startup.rs — Cross-platform launch-at-login helpers
// =============================================================================

use auto_launch::AutoLaunchBuilder;

fn build_auto_launch() -> anyhow::Result<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe()?;
    let exe_path = exe.to_string_lossy().to_string();

    let auto = AutoLaunchBuilder::new()
        .set_app_name(crate::types::APP_NAME)
        .set_app_path(&exe_path)
        .set_use_launch_agent(true)
        .build()?;

    Ok(auto)
}

pub fn is_enabled() -> bool {
    build_auto_launch()
        .and_then(|auto| auto.is_enabled().map_err(Into::into))
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let auto = build_auto_launch()?;
    if enabled {
        auto.enable()?;
    } else {
        auto.disable()?;
    }
    Ok(())
}
