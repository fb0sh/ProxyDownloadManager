// =============================================================================
// ui.rs — egui UI rendering for ProxyDownloadManager
//
// Implements eframe::App by organizing the ~1450-line ui() method into
// well-named private helpers for each major UI section.
// =============================================================================

use crate::app::ProxyDownloadManager;
use crate::types::*;
use crate::icons::IconCache;

use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, Margin, Vec2, RichText, ScrollArea};
use std::path::PathBuf;
use std::fs;

impl eframe::App for ProxyDownloadManager {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ── Initialize icon cache (once) ──────────────────────────────────────
        if self.icon_cache.is_none() {
            self.icon_cache = Some(IconCache::new(ui.ctx()));
        }

        // ── Sync from shared state ────────────────────────────────────────────
        if let Ok(shared) = self.shared.lock() {
            self.downloads = shared.clone();
        }

        // ── Update speed trackers for downloading items ───────────────────────
        for item in &self.downloads {
            if matches!(item.status, DownloadStatus::Downloading) {
                let tracker = self.speed_trackers.entry(item.id).or_insert_with(SpeedTracker::new);
                tracker.update(item.downloaded);
            } else {
                self.speed_trackers.remove(&item.id);
            }
        }

        // ── Clipboard URL auto-detect ─────────────────────────────────────
        self.clipboard_poll_counter += 1;
        if self.clipboard_poll_counter >= 30 && !self.show_new_dialog {
            self.clipboard_poll_counter = 0;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    let t = text.trim().to_string();
                    if t != self.last_clipboard_text
                        && (t.starts_with("http://") || t.starts_with("https://") || t.starts_with("ftp://"))
                        && looks_like_download_url(&t)
                    {
                        crate::log_info!("Auto-detected download URL: {}", t);
                        self.last_clipboard_text.clone_from(&t);
                        self.show_new_dialog = true;
                        self.new_url = t;
                        self.new_filename = ProxyDownloadManager::file_name_from_url(&self.new_url);
                        self.new_proxy_name = self.settings.default_proxy.clone();
                        self.new_connections = 0;
                        self.clipboard_checked = true;
                        self.prev_url_for_name.clone_from(&self.new_url);
                    }
                }
            }
        }

        // ── Time-based status message fading ──────────────────────────────────
        if self.status_message.is_some() {
            self.status_message_timer -= ui.input(|i| i.unstable_dt);
            if self.status_message_timer <= 0.0 {
                self.status_message = None;
            }
        }

        // ── Pre-compute immutable values ──────────────────────────────────────
        let all_count = self.downloads.len();
        let completed_count = self.downloads.iter().filter(|d| d.status == DownloadStatus::Completed).count();
        let incompleted_count = all_count - completed_count;
        let sel_ids = &self.selected_ids;
        let any_selected = !sel_ids.is_empty();
        let can_resume = if any_selected {
            self.downloads.iter().any(|d| sel_ids.contains(&d.id) && matches!(d.status, DownloadStatus::Paused | DownloadStatus::Failed(_)))
        } else {
            self.downloads.iter().any(|d| matches!(d.status, DownloadStatus::Paused | DownloadStatus::Failed(_)))
        };
        let can_stop = if any_selected {
            self.downloads.iter().any(|d| sel_ids.contains(&d.id) && matches!(d.status, DownloadStatus::Downloading))
        } else {
            self.downloads.iter().any(|d| matches!(d.status, DownloadStatus::Downloading))
        };
        let has_active = !self.active_downloads.is_empty();
        let status_msg = self.status_message.clone();
        let filter = self.filter.clone();

        // ── Local action signals for closures ─────────────────────────────────
        let mut tb_new = false;
        let mut tb_resume = false;
        let mut tb_stop = false;
        let mut tb_delete = false;
        let mut tb_quit = false;
        let mut tb_settings = false;
        let mut tb_about = false;
        let mut sb_filter: Option<TreeFilter> = None;

        // ── Top Toolbar ───────────────────────────────────────────────────────
        self.render_toolbar(ui, &mut tb_new, &mut tb_resume, &mut tb_stop, &mut tb_delete,
                            &mut tb_quit, &mut tb_settings, &mut tb_about, can_resume, can_stop);

        // ── Handle toolbar actions ────────────────────────────────────────────
        if tb_new {
            self.show_new_dialog = true;
            self.new_url.clear();
            self.new_filename.clear();
            self.new_proxy_name = self.settings.default_proxy.clone();
            self.new_connections = 0;
            self.clipboard_checked = false;
            self.prev_url_for_name.clear();
        }
        if tb_resume {
            let ids: Vec<u64> = if self.selected_ids.is_empty() {
                self.downloads.iter().filter(|d| matches!(d.status, DownloadStatus::Paused | DownloadStatus::Failed(_))).map(|d| d.id).collect()
            } else {
                self.selected_ids.iter().copied().collect()
            };
            for id in &ids { self.resume_download(*id); }
            // Batch resume: no detail window; single resume: show detail
            if ids.len() > 1 {
                for id in &ids { self.closed_detail_windows.insert(*id); }
            } else if ids.len() == 1 {
                self.manual_detail_ids.insert(ids[0]);
            }
        }
        if tb_stop {
            let ids: Vec<u64> = if self.selected_ids.is_empty() {
                self.downloads.iter().filter(|d| matches!(d.status, DownloadStatus::Downloading)).map(|d| d.id).collect()
            } else {
                self.selected_ids.iter().copied().collect()
            };
            for id in ids { self.stop_download(id); }
        }
        if tb_delete {
            let ids: Vec<u64> = if self.selected_ids.is_empty() {
                self.downloads.iter().map(|d| d.id).collect()
            } else {
                self.selected_ids.iter().copied().collect()
            };
            if !ids.is_empty() { self.pending_delete_ids = ids; }
        }
        if tb_quit {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if tb_settings {
            self.show_settings = true;
        }
        if tb_about {
            self.show_about = true;
        }

        // ── Sidebar (Tree View) ──────────────────────────────────────────────
        self.render_sidebar(ui, all_count, completed_count, incompleted_count,
                            &filter, &status_msg, &mut sb_filter);

        // ── Handle sidebar filter changes ─────────────────────────────────────
        if let Some(f) = sb_filter {
            self.filter = f;
        }

        // ── Central Table View ───────────────────────────────────────────────
        self.render_table(ui, &filter);

        // ── Dialogs ──────────────────────────────────────────────────────────
        self.render_delete_dialog(ui);
        self.render_edit_dialog(ui);
        self.render_properties_popup(ui);
        self.render_new_download_dialog(ui);
        self.render_detail_windows(ui);
        self.render_settings_window(ui);
        self.render_about_window(ui);

        // ── Auto-save (every ~60 frames) ─────────────────────────────────────
        self.save_counter += 1;
        if self.save_counter >= 60 {
            self.save_counter = 0;
            if let Ok(items) = self.shared.lock() {
                let dl_path = crate::types::downloads_path();
                let _ = std::fs::create_dir_all(dl_path.parent().unwrap());
                crate::persist::save_downloads(&dl_path.to_string_lossy().to_string(), &items);
            }
        }

        // ── Process detail window actions ─────────────────────────────────────
        let queued_actions: Vec<(u64, &'static str)> = {
            let mut actions = self.detail_actions.lock().unwrap();
            actions.drain(..).collect()
        };
        for (item_id, action) in queued_actions {
            match action {
                "stop" => { self.stop_download(item_id); },
                "resume" => { self.resume_download(item_id); },
                "delete" => { self.pending_delete_ids = vec![item_id]; },
                "close" => { self.manual_detail_ids.remove(&item_id); },
                _ => {},
            }
        }

        // ── Request repaint while downloading ────────────────────────────────
        if has_active || self.downloads.iter().any(|d| matches!(d.status, DownloadStatus::Downloading)) {
            ui.ctx().request_repaint();
        }
    }
}

// ─── Private rendering helpers ─────────────────────────────────────────────────

impl ProxyDownloadManager {
    fn render_toolbar(&mut self, ui: &mut egui::Ui,
                      tb_new: &mut bool, tb_resume: &mut bool, tb_stop: &mut bool,
                      tb_delete: &mut bool, tb_quit: &mut bool,
                      tb_settings: &mut bool, tb_about: &mut bool,
                      can_resume: bool, can_stop: bool)
    {
        let gap = 6.0;
        let btn_h = 30.0;

        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);

                if ui.add_sized(Vec2::new(140.0, btn_h),
                    egui::Button::new(RichText::new("📥 New Download").size(14.0)),
                ).clicked() { *tb_new = true; }
                ui.add_space(gap);

                let resume_btn = ui.add_sized(Vec2::new(100.0, btn_h),
                    egui::Button::new(RichText::new("▶ Resume").size(14.0)));
                if resume_btn.clicked() && can_resume { *tb_resume = true; }
                ui.add_space(gap);

                let stop_btn = ui.add_sized(Vec2::new(100.0, btn_h),
                    egui::Button::new(RichText::new("⏹ Stop").size(14.0)));
                if stop_btn.clicked() && can_stop { *tb_stop = true; }
                ui.add_space(gap);

                if ui.add_sized(Vec2::new(100.0, btn_h),
                    egui::Button::new(RichText::new("🗑 Delete").size(14.0)),
                ).clicked() { *tb_delete = true; }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(gap);
                    if ui.add_sized(Vec2::new(80.0, btn_h),
                        egui::Button::new(RichText::new("❌ Quit").size(14.0)),
                    ).clicked() { *tb_quit = true; }
                    ui.add_space(gap);
                    if ui.add_sized(Vec2::new(90.0, btn_h),
                        egui::Button::new(RichText::new("ℹ About").size(14.0)),
                    ).clicked() { *tb_about = true; }
                    ui.add_space(gap);
                    if ui.add_sized(Vec2::new(110.0, btn_h),
                        egui::Button::new(RichText::new("⚙ Settings").size(14.0)),
                    ).clicked() { *tb_settings = true; }
                    ui.add_space(8.0);
                });
                ui.add_space(8.0);
            });
            ui.add_space(4.0);
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui,
                      all_count: usize, completed_count: usize, incompleted_count: usize,
                      filter: &TreeFilter, status_msg: &Option<String>,
                      sb_filter: &mut Option<TreeFilter>)
    {
        egui::Panel::left("sidebar")
            .resizable(false)
            .default_size(130.0)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("📂 Downloads");
                ui.separator();
                ui.add_space(4.0);

                if ui.add(egui::Button::new(format!("📁 All ({})", all_count))
                    .selected(filter == &TreeFilter::All)
                    .min_size(Vec2::new(110.0, 28.0))
                ).clicked() { *sb_filter = Some(TreeFilter::All); }

                if ui.add(egui::Button::new(format!("✅ Completed ({})", completed_count))
                    .selected(filter == &TreeFilter::Completed)
                    .min_size(Vec2::new(110.0, 28.0))
                ).clicked() { *sb_filter = Some(TreeFilter::Completed); }

                if ui.add(egui::Button::new(format!("⏳ Incomplete ({})", incompleted_count))
                    .selected(filter == &TreeFilter::Incompleted)
                    .min_size(Vec2::new(110.0, 28.0))
                ).clicked() { *sb_filter = Some(TreeFilter::Incompleted); }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(4.0);

                ui.label(RichText::new(format!("Total: {}", all_count))
                    .size(12.0).color(Color32::GRAY));

                if let Some(msg) = status_msg {
                    ui.add_space(8.0);
                    ui.label(RichText::new(msg.as_str()).size(12.0).color(Color32::from_rgb(0, 180, 0)));
                }
            });
    }

    fn render_table(&mut self, ui: &mut egui::Ui, filter: &TreeFilter) {
        let cloned_items: Vec<DownloadItem> = self.downloads.clone();

        // ── Context menu action flags ────────────────────────────────────────
        let mut ctx_resume: Option<u64> = None;
        let mut ctx_stop: Option<u64> = None;
        let mut ctx_show_delete_dialog: Option<u64> = None;
        let mut ctx_redownload: Option<(String, String)> = None;
        let mut ctx_double_click: Option<u64> = None;
        let mut ctx_edit: Option<u64> = None;
        let mut ctx_properties: Option<u64> = None;
        let ctx_toggle_select: Option<u64> = None;
        let mut ctx_select_all: Option<bool> = None;
        let mut selected_ids = self.selected_ids.clone();

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Sort by last_try descending (newest first), empty last_try at bottom
            let mut filtered_items: Vec<&DownloadItem> = match filter {
                TreeFilter::All => cloned_items.iter().collect(),
                TreeFilter::Completed => cloned_items.iter().filter(|d| d.status == DownloadStatus::Completed).collect(),
                TreeFilter::Incompleted => cloned_items.iter().filter(|d| d.status != DownloadStatus::Completed).collect(),
            };
            filtered_items.sort_by(|a, b| {
                if a.last_try.is_empty() && b.last_try.is_empty() { std::cmp::Ordering::Equal }
                else if a.last_try.is_empty() { std::cmp::Ordering::Greater }
                else if b.last_try.is_empty() { std::cmp::Ordering::Less }
                else { b.last_try.cmp(&a.last_try) }
            });

            // ── Header row ───────────────────────────────────────────────────
            ui.horizontal(|ui| {
                let label = match filter {
                    TreeFilter::All => "All Downloads",
                    TreeFilter::Completed => "Completed Downloads",
                    TreeFilter::Incompleted => "Incomplete Downloads",
                };
                let sel_count = self.selected_ids.len();
                ui.label(RichText::new(label).size(14.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if sel_count > 0 {
                        let sel_item_count = filtered_items.iter().filter(|d| self.selected_ids.contains(&d.id)).count();
                        if sel_item_count > 0 {
                            ui.label(RichText::new(format!("{} selected", sel_item_count))
                                .size(12.0).color(Color32::from_rgb(0, 120, 215)));
                            ui.add_space(8.0);
                        }
                    }
                    ui.label(RichText::new(format!("{} items", filtered_items.len()))
                        .size(12.0).color(Color32::GRAY));
                });
            });
            ui.separator();

            // ── Column headers ───────────────────────────────────────────────
            let header_height = 26.0;
            Frame::NONE.inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let all_selected = !filtered_items.is_empty()
                        && filtered_items.iter().all(|d| selected_ids.contains(&d.id));
                    let cb_label = if all_selected { "☑" } else { "☐" };
                    if ui.add_sized(Vec2::new(28.0, header_height),
                        egui::Button::new(RichText::new(cb_label).strong().size(11.0))
                    ).clicked() { ctx_select_all = Some(!all_selected); }

                    ui.add_sized(Vec2::new(192.0, header_height),
                        egui::Label::new(RichText::new("File Name").strong().size(12.0)));
                    ui.add_sized(Vec2::new(75.0, header_height),
                        egui::Label::new(RichText::new("Size").strong().size(12.0)));
                    ui.add_sized(Vec2::new(120.0, header_height),
                        egui::Label::new(RichText::new("Status").strong().size(12.0)));
                    ui.add_sized(Vec2::new(80.0, header_height),
                        egui::Label::new(RichText::new("Speed").strong().size(12.0)));
                    ui.add_sized(Vec2::new(80.0, header_height),
                        egui::Label::new(RichText::new("Remain").strong().size(12.0)));
                    ui.add_sized(Vec2::new(55.0, header_height),
                        egui::Label::new(RichText::new("Resume").strong().size(12.0)));
                    ui.add_sized(Vec2::new(55.0, header_height),
                        egui::Label::new(RichText::new("Proxy").strong().size(12.0)));
                    ui.add_sized(Vec2::new(75.0, header_height),
                        egui::Label::new(RichText::new("Last Try").strong().size(12.0)));
                });
            });
            ui.add_space(2.0);

            // ── Data rows ────────────────────────────────────────────────────
            ScrollArea::both()
                .id_salt("download_table")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let row_height = 32.0;
                    let btn_bg = ui.style().visuals.widgets.inactive.bg_fill;
                    let row_color_normal = btn_bg;
                    let row_color_selected = Color32::from_rgb(
                        (btn_bg.r() as u16 + 30).min(255) as u8,
                        (btn_bg.g() as u16 + 40).min(255) as u8,
                        (btn_bg.b() as u16 + 60).min(255) as u8,
                    );

                    let icon_cache = &mut self.icon_cache;
                    let speed_trackers = &self.speed_trackers;

                    for item in filtered_items.iter() {
                        let is_selected = selected_ids.contains(&item.id);
                        let bg = if is_selected { row_color_selected } else { row_color_normal };

                        let icon_texture = icon_cache
                            .as_mut()
                            .map(|cache| cache.get_icon(&item.file_name, ui.ctx()));

                        let frame = Frame::NONE
                            .fill(bg)
                            .corner_radius(CornerRadius::same(2))
                            .inner_margin(Margin::symmetric(8, 2));

                        let response = frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Checkbox
                                let cb_label = if is_selected { "☑" } else { "☐" };
                                ui.add_sized(Vec2::new(28.0, row_height),
                                    egui::Label::new(RichText::new(cb_label).size(12.0).color(Color32::BLACK)));

                                // File Name
                                ui.add_sized(Vec2::new(192.0, row_height), |ui: &mut egui::Ui| {
                                    ui.horizontal(|ui| {
                                        if let Some(tex) = &icon_texture {
                                            ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::new(18.0, 18.0)));
                                        }
                                        ui.add(egui::Label::new(
                                            RichText::new(&item.file_name).size(12.0).color(Color32::BLACK)
                                        ).truncate());
                                    }).response
                                });

                                // Size
                                ui.add_sized(Vec2::new(75.0, row_height),
                                    egui::Label::new(RichText::new(format_size(item.total_size)).size(12.0).color(Color32::BLACK)));

                                // Status
                                let (status_text, status_color) = status_icon_and_text(&item.status);
                                let is_merging = item.merge_progress > 0.0;
                                let merge_pct = (item.merge_progress * 100.0) as u32;
                                let status_display = if is_merging {
                                    format!("Merging ({}%)", merge_pct.min(100))
                                } else {
                                    let pct = if item.total_size > 0 {
                                        ((item.downloaded as f64 / item.total_size as f64) * 100.0) as u32
                                    } else { 0 };
                                    match &item.status {
                                        DownloadStatus::Failed(msg) if !msg.is_empty() => {
                                            let truncated = if msg.len() > 40 { format!("{}...", &msg[..37]) } else { msg.clone() };
                                            format!("{}: {}", status_text, truncated)
                                        },
                                        DownloadStatus::Downloading if item.total_size > 0 => format!("{} ({}%)", status_text, pct.min(100)),
                                        DownloadStatus::Paused if item.downloaded > 0 && item.total_size > 0 => format!("{} ({}%)", status_text, pct.min(100)),
                                        _ => status_text.to_string(),
                                    }
                                };
                                let merge_color = Color32::from_rgb(255, 170, 0); // amber
                                ui.add_sized(Vec2::new(120.0, row_height),
                                    egui::Label::new(RichText::new(&status_display).size(12.0).color(if is_merging { merge_color } else { status_color })));

                                // Speed + Remain
                                let (speed_str, remain_str) = if matches!(item.status, DownloadStatus::Downloading) {
                                    if let Some(tracker) = speed_trackers.get(&item.id) {
                                        let spd = tracker.speed();
                                        let rem = item.total_size.saturating_sub(item.downloaded);
                                        (format_speed(spd), tracker.eta(rem))
                                    } else { ("-".to_string(), "-".to_string()) }
                                } else { ("-".to_string(), "-".to_string()) };
                                ui.add_sized(Vec2::new(80.0, row_height),
                                    egui::Label::new(RichText::new(speed_str).size(11.0).color(Color32::BLACK)));
                                ui.add_sized(Vec2::new(80.0, row_height),
                                    egui::Label::new(RichText::new(remain_str).size(11.0).color(Color32::BLACK)));

                                // Resume badge
                                let resume_display = match item.resumable {
                                    Some(true) => "✅".to_string(),
                                    Some(false) => "❌".to_string(),
                                    None => "-".to_string(),
                                };
                                ui.add_sized(Vec2::new(55.0, row_height),
                                    egui::Label::new(RichText::new(resume_display).size(11.0).color(Color32::BLACK)));

                                // Proxy
                                let proxy_display = if item.proxy_name.is_empty() {
                                    "-".to_string()
                                } else { format!("🔌 {}", item.proxy_name) };
                                ui.add_sized(Vec2::new(55.0, row_height),
                                    egui::Label::new(RichText::new(proxy_display).size(11.0).color(Color32::BLACK)));

                                // Last Try
                                let last_try_display = if item.last_try.is_empty() { "-".to_string() } else { item.last_try.clone() };
                                ui.add_sized(Vec2::new(75.0, row_height),
                                    egui::Label::new(RichText::new(last_try_display).size(11.0).color(Color32::BLACK)));
                            }).response
                        });

                        // ── Row interaction ──────────────────────────────────
                        let row_rect = response.response.rect;
                        let row_response = ui.interact(row_rect, egui::Id::new(("row", item.id)), egui::Sense::click());

                        if row_response.clicked() {
                            if selected_ids.contains(&item.id) {
                                selected_ids.remove(&item.id);
                            } else {
                                selected_ids.insert(item.id);
                            }
                        }
                        if row_response.double_clicked() {
                            if !selected_ids.contains(&item.id) {
                                selected_ids.clear();
                                selected_ids.insert(item.id);
                            }
                            ctx_double_click = Some(item.id);
                        }

                        // ── Context menu ─────────────────────────────────────
                        let item_clone = item.clone();
                        row_response.context_menu(|ui| {
                            let file_exists = PathBuf::from(&item_clone.save_path).exists();

                            if matches!(item_clone.status, DownloadStatus::Paused | DownloadStatus::Failed(_)) {
                                if ui.button("▶ Continue").clicked() {
                                    ctx_resume = Some(item_clone.id);
                                    ui.close();
                                }
                                if ui.button("✏️ Edit").clicked() {
                                    ctx_edit = Some(item_clone.id);
                                    ui.close();
                                }
                            }
                            if matches!(item_clone.status, DownloadStatus::Downloading) {
                                if ui.button("⏹ Stop").clicked() {
                                    ctx_stop = Some(item_clone.id);
                                    ui.close();
                                }
                            }
                            if matches!(item_clone.status, DownloadStatus::Completed | DownloadStatus::Failed(_) | DownloadStatus::Paused) {
                                if ui.button("🔄 Redownload").clicked() {
                                    ctx_redownload = Some((item_clone.url.clone(), item_clone.file_name.clone()));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if file_exists {
                                if ui.button("📂 Open").clicked() {
                                    #[cfg(target_os = "macos")]
                                    let _ = std::process::Command::new("open").arg(&item_clone.save_path).spawn();
                                    #[cfg(target_os = "windows")]
                                    let _ = std::process::Command::new("explorer").arg(&item_clone.save_path).spawn();
                                    #[cfg(target_os = "linux")]
                                    let _ = std::process::Command::new("xdg-open").arg(&item_clone.save_path).spawn();
                                    ui.close();
                                }
                                if ui.button("📁 Show in Folder").clicked() {
                                    #[cfg(target_os = "macos")]
                                    let _ = std::process::Command::new("open").arg("-R").arg(&item_clone.save_path).spawn();
                                    #[cfg(target_os = "windows")]
                                    let _ = std::process::Command::new("explorer").arg("/select,").arg(&item_clone.save_path).spawn();
                                    #[cfg(target_os = "linux")]
                                    if let Some(parent) = std::path::Path::new(&item_clone.save_path).parent() {
                                        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                                    }
                                    ui.close();
                                }
                            }
                            if ui.button(format!("🗑 Delete{}",
                                if selected_ids.len() > 1 { format!(" ({} selected)", selected_ids.len()) } else { String::new() })
                            ).clicked() {
                                ctx_show_delete_dialog = Some(item_clone.id);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("ℹ Properties").clicked() {
                                ctx_properties = Some(item_clone.id);
                                ui.close();
                            }
                        });
                    }

                    if filtered_items.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("📭 No downloads yet").size(18.0).color(Color32::GRAY));
                            ui.add_space(8.0);
                            ui.label(RichText::new("Click 'New Download' to get started").size(14.0).color(Color32::DARK_GRAY));
                        });
                    }
                });
        });

        // ── Handle context menu actions ──────────────────────────────────────
        if let Some(id) = ctx_resume {
            self.resume_download(id);
            self.manual_detail_ids.insert(id);
        }
        if let Some(id) = ctx_stop {
            self.stop_download(id);
        }
        if let Some((url, name)) = ctx_redownload {
            let item_id = self.downloads.iter().find(|d| d.url == url && d.file_name == name).map(|d| d.id);
            if let Some(id) = item_id { self.delete_download(id); }
            self.add_new_download(&url, Some(&name));
        }
        if let Some(id) = ctx_show_delete_dialog {
            let ids: Vec<u64> = if self.selected_ids.is_empty() { vec![id] } else { self.selected_ids.iter().copied().collect() };
            self.pending_delete_ids = ids;
        }
        if let Some(id) = ctx_edit {
            self.edit_item_id = Some(id);
            self.edit_url = self.downloads.iter().find(|d| d.id == id).map(|d| d.url.clone()).unwrap_or_default();
            self.edit_filename = self.downloads.iter().find(|d| d.id == id).map(|d| d.file_name.clone()).unwrap_or_default();
            self.edit_proxy_name = self.downloads.iter().find(|d| d.id == id).map(|d| d.proxy_name.clone()).unwrap_or_default();
            self.edit_connections = self.downloads.iter().find(|d| d.id == id).map(|d| d.connections).unwrap_or(0);
        }
        if let Some(id) = ctx_properties {
            self.show_properties = Some(id);
        }
        if let Some(id) = ctx_toggle_select {
            if selected_ids.contains(&id) { selected_ids.remove(&id); } else { selected_ids.insert(id); }
        }
        if let Some(select) = ctx_select_all {
            for item in &cloned_items {
                let matches_filter = match filter {
                    TreeFilter::All => true,
                    TreeFilter::Completed => matches!(item.status, DownloadStatus::Completed),
                    TreeFilter::Incompleted => !matches!(item.status, DownloadStatus::Completed),
                };
                if matches_filter {
                    if select { selected_ids.insert(item.id); } else { selected_ids.remove(&item.id); }
                }
            }
        }
        if let Some(id) = ctx_double_click {
            self.manual_detail_ids.insert(id);
        }

        self.selected_ids = selected_ids;
    }

    // ── Dialog helpers ──────────────────────────────────────────────────────

    fn render_delete_dialog(&mut self, ui: &mut egui::Ui) {
        if self.pending_delete_ids.is_empty() { return; }
        let del_count = self.pending_delete_ids.len();
        let first_name = self.pending_delete_ids.first()
            .and_then(|id| self.downloads.iter().find(|d| d.id == *id))
            .map(|d| d.file_name.clone());
        let display_name = match &first_name {
            Some(name) if del_count == 1 => format!("\"{}\"", name),
            Some(name) => format!("\"{}\" and {} more", name, del_count - 1),
            None => format!("{} items", del_count),
        };

        egui::Window::new("🗑 Confirm Delete")
            .id(egui::Id::new("delete_confirm"))
            .collapsible(false).resizable(false)
            .default_size(Vec2::new(380.0, 180.0))
            .show(ui.ctx(), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(display_name).size(14.0).strong());
                ui.add_space(4.0);
                ui.label("What would you like to do?");
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add_sized(Vec2::new(180.0, 30.0),
                        egui::Button::new(RichText::new("🗑 Delete Record Only").size(13.0)))
                        .on_hover_text("Remove from list, keep file").clicked()
                    {
                        let ids = std::mem::take(&mut self.pending_delete_ids);
                        let mut items = self.shared.lock().unwrap();
                        for id in &ids { items.retain(|d| d.id != *id); self.selected_ids.remove(id); }
                    }
                    if ui.add_sized(Vec2::new(180.0, 30.0),
                        egui::Button::new(RichText::new("🗑 Delete File & Record").size(13.0)))
                        .on_hover_text("Remove from list and delete file").clicked()
                    {
                        let ids = std::mem::take(&mut self.pending_delete_ids);
                        for id in &ids { self.delete_download(*id); }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized(Vec2::new(120.0, 28.0), egui::Button::new("Cancel")).clicked() {
                            self.pending_delete_ids.clear();
                        }
                    });
                });
            });
    }

    fn render_edit_dialog(&mut self, ui: &mut egui::Ui) {
        let edit_id = match self.edit_item_id { Some(id) => id, None => return };
        if self.downloads.iter().find(|d| d.id == edit_id).is_none() {
            self.edit_item_id = None;
            return;
        }

        egui::Window::new("✏️ Edit Download")
            .id(egui::Id::new("edit_window"))
            .collapsible(false).resizable(false)
            .default_size(Vec2::new(460.0, 220.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("URL:").strong());
                    ui.add_sized(Vec2::new(350.0, 24.0),
                        egui::TextEdit::singleline(&mut self.edit_url).hint_text("URL"));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Name:").strong());
                    ui.add_sized(Vec2::new(350.0, 24.0),
                        egui::TextEdit::singleline(&mut self.edit_filename).hint_text("Filename"));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Proxy:").strong());
                    let names: Vec<String> = std::iter::once(String::new())
                        .chain(self.settings.proxies.iter().map(|p| p.name.clone())).collect();
                    let current_idx = names.iter().position(|n| *n == self.edit_proxy_name).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("edit_proxy")
                        .selected_text(if self.edit_proxy_name.is_empty() { "No Proxy".to_string() } else { self.edit_proxy_name.clone() })
                        .show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                let display = if name.is_empty() { "No Proxy".to_string() } else { name.clone() };
                                if ui.selectable_label(sel == i, &display).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.edit_proxy_name = if sel == 0 { String::new() } else { names[sel].clone() }; }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Threads:").strong());
                    let conn_options = [("Global (default)", 0u32), ("8", 8), ("16", 16), ("32", 32), ("64", 64)];
                    let current_idx = conn_options.iter().position(|(_, v)| *v == self.edit_connections).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("edit_connections")
                        .selected_text(if self.edit_connections == 0 { "Global (default)".to_string() } else { self.edit_connections.to_string() })
                        .show_ui(ui, |ui| {
                            for (i, (label, _)) in conn_options.iter().enumerate() {
                                if ui.selectable_label(sel == i, *label).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.edit_connections = conn_options[sel].1; }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized(Vec2::new(100.0, 28.0), egui::Button::new("💾 Save")).clicked() {
                            let mut items = self.shared.lock().unwrap();
                            if let Some(item) = items.iter_mut().find(|d| d.id == edit_id) {
                                item.url = std::mem::take(&mut self.edit_url);
                                item.file_name = std::mem::take(&mut self.edit_filename);
                                item.proxy_name = std::mem::take(&mut self.edit_proxy_name);
                                if self.edit_connections > 0 { item.connections = std::mem::replace(&mut self.edit_connections, 0); }
                            }
                            self.edit_item_id = None;
                        }
                        ui.add_space(8.0);
                        if ui.add_sized(Vec2::new(80.0, 28.0), egui::Button::new("Cancel")).clicked() {
                            self.edit_item_id = None;
                        }
                    });
                });
            });
    }

    fn render_properties_popup(&mut self, ui: &mut egui::Ui) {
        let prop_id = match self.show_properties { Some(id) => id, None => return };
        if let Some(item) = self.downloads.iter().find(|d| d.id == prop_id) {
            let file_exists = PathBuf::from(&item.save_path).exists();
            let file_size = if file_exists {
                fs::metadata(&item.save_path).ok().map(|m| m.len()).unwrap_or(0)
            } else { 0 };

            egui::Window::new("📋 Properties")
                .id(egui::Id::new("properties_window"))
                .collapsible(false).resizable(false)
                .default_size(Vec2::new(420.0, 280.0))
                .show(ui.ctx(), |ui| {
                    ui.vertical(|ui| {
                        let mut row = |label: &str, value: &str| {
                            ui.horizontal(|ui| {
                                ui.add_sized(Vec2::new(140.0, 20.0),
                                    egui::Label::new(RichText::new(label).strong().size(13.0)));
                                ui.label(RichText::new(value).size(13.0).color(Color32::BLACK));
                            });
                            ui.add_space(2.0);
                        };

                        let status_str = match &item.status {
                            DownloadStatus::Downloading => "Downloading".into(),
                            DownloadStatus::Paused => "Paused".into(),
                            DownloadStatus::Completed => "Completed".into(),
                            DownloadStatus::Failed(msg) => format!("Failed: {}", msg),
                            DownloadStatus::Queued => "Queued".into(),
                        };
                        row("File Name:", &item.file_name);
                        row("URL:", &item.url);
                        row("Save Path:", &item.save_path);
                        row("Size:", &format_size(item.total_size));
                        row("Downloaded:", &format_size(item.downloaded));
                        row("On Disk:", &format_size(file_size));
                        row("Status:", &status_str);
                        row("Proxy:", if item.proxy_name.is_empty() { "None" } else { &item.proxy_name });
                        row("Connections:", &item.connections.to_string());
                        row("Parts:", &item.parts.len().to_string());
                        row("Last Try:", if item.last_try.is_empty() { "-" } else { &item.last_try });
                        row("Created:", &item.created_at);

                        if file_exists {
                            if let Ok(md) = fs::metadata(&item.save_path) {
                                if let Ok(modified) = md.modified() {
                                    use std::time::UNIX_EPOCH;
                                    if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                        let secs = duration.as_secs();
                                        let days = secs / 86400;
                                        let hours = (secs % 86400) / 3600;
                                        let mins = (secs % 3600) / 60;
                                        row("Modified:", &format!("{}d {}h {}m ago", days, hours, mins));
                                    }
                                }
                            }
                        }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_sized(Vec2::new(80.0, 28.0), egui::Button::new("Close")).clicked() {
                                self.show_properties = None;
                            }
                        });
                    });
                });
        } else {
            self.show_properties = None;
        }
    }

    fn render_new_download_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.show_new_dialog { return; }

        // Auto-detect clipboard URL on first open
        if !self.clipboard_checked {
            self.clipboard_checked = true;
            if self.new_url.trim().is_empty() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let t = text.trim().to_string();
                        if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("ftp://") {
                            self.new_url = t;
                        }
                    }
                }
            }
        }

        // Auto-fill filename from URL when URL changes
        if self.new_url != self.prev_url_for_name {
            self.new_filename = ProxyDownloadManager::file_name_from_url(&self.new_url);
            self.prev_url_for_name = self.new_url.clone();
        }

        egui::Window::new("New Download")
            .id(egui::Id::new("new_download_window"))
            .collapsible(false).resizable(false)
            .default_size(Vec2::new(520.0, 310.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("URL:").strong());
                    ui.add_sized(Vec2::new(400.0, 24.0),
                        egui::TextEdit::singleline(&mut self.new_url)
                            .hint_text("https://example.com/file.zip")
                            .cursor_at_end(false));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Name:").strong());
                    ui.add_sized(Vec2::new(400.0, 24.0),
                        egui::TextEdit::singleline(&mut self.new_filename)
                            .hint_text("Auto-detected from URL"));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Proxy:").strong());
                    let names: Vec<String> = std::iter::once(String::new())
                        .chain(self.settings.proxies.iter().map(|p| p.name.clone())).collect();
                    let current_idx = names.iter().position(|n| *n == self.new_proxy_name).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("download_proxy")
                        .selected_text(if self.new_proxy_name.is_empty() { "No Proxy".to_string() } else { self.new_proxy_name.clone() })
                        .show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                let display = if name.is_empty() { "No Proxy".to_string() } else { name.clone() };
                                if ui.selectable_label(sel == i, &display).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.new_proxy_name = if sel == 0 { String::new() } else { names[sel].clone() }; }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Threads:").strong());
                    let conn_options = [("Global (default)", 0u32), ("8", 8), ("16", 16), ("32", 32), ("64", 64)];
                    let current_idx = conn_options.iter().position(|(_, v)| *v == self.new_connections).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("download_connections")
                        .selected_text(if self.new_connections == 0 { format!("Global ({})", self.settings.max_connections) } else { self.new_connections.to_string() })
                        .show_ui(ui, |ui| {
                            for (i, (label, _)) in conn_options.iter().enumerate() {
                                if ui.selectable_label(sel == i, *label).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.new_connections = conn_options[sel].1; }
                    ui.add_space(8.0);
                    ui.label(RichText::new("(per-file concurrent parts)").size(11.0).color(Color32::GRAY));
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let url_ok = !self.new_url.trim().is_empty();
                        if ui.add_sized(Vec2::new(120.0, 28.0),
                            egui::Button::new(RichText::new("📥 Download").size(14.0)),
                        ).clicked() && url_ok {
                            let url = self.new_url.trim().to_string();
                            let filename = if self.new_filename.trim().is_empty() { None }
                                else { Some(self.new_filename.trim().to_string()) };
                            self.new_proxy_name = self.new_proxy_name.trim().to_string();
                            self.add_new_download(&url, filename.as_deref());
                            self.show_new_dialog = false;
                            if let Ok(items) = self.shared.lock() {
                                let dl_path = crate::types::downloads_path();
                                let _ = std::fs::create_dir_all(dl_path.parent().unwrap());
                                crate::persist::save_downloads(&dl_path.to_string_lossy().to_string(), &items);
                            }
                        }
                        ui.add_space(8.0);
                        if ui.add_sized(Vec2::new(120.0, 28.0), egui::Button::new("Cancel")).clicked() {
                            self.show_new_dialog = false;
                        }
                    });
                });
            });
    }

    fn render_detail_windows(&mut self, ui: &mut egui::Ui) {
        let mut to_close_manual: Vec<u64> = Vec::new();

        for item in &self.downloads {
            let is_auto = matches!(item.status, DownloadStatus::Downloading | DownloadStatus::Queued);
            let is_manual = self.manual_detail_ids.contains(&item.id);
            if !is_auto && !is_manual { continue; }

            let fname = item.file_name.clone();
            let overall_pct = if item.total_size > 0 {
                item.downloaded as f64 / item.total_size as f64
            } else { 0.0 };
            let (spd_str, eta_str) = if let Some(tracker) = self.speed_trackers.get(&item.id) {
                (format_speed(tracker.speed()),
                 tracker.eta(item.total_size.saturating_sub(item.downloaded)))
            } else { ("-".to_string(), "-".to_string()) };
            let proxy_str = if item.proxy_name.is_empty() {
                "No Proxy".to_string()
            } else { format!("🔌 {}", item.proxy_name) };
            let resume_str = match item.resumable {
                Some(true) => "✅ Resumable".to_string(),
                Some(false) => "❌ Non-Resumable".to_string(),
                None => String::new(),
            };
            let parts = item.parts.clone();
            let has_parts = parts.len() > 1;
            let is_merging = item.merge_progress > 0.0
                || (has_parts
                    && parts.iter().all(|p| p.status == PartStatus::Completed)
                    && item.status == DownloadStatus::Downloading);
            let item_id = item.id;
            let actions = self.detail_actions.clone();

            let mut open = (is_auto && !self.closed_detail_windows.contains(&item.id))
                || self.manual_detail_ids.contains(&item.id);

            let icon_texture = self.icon_cache
                .as_mut()
                .map(|cache| cache.get_icon(&item.file_name, ui.ctx()));

            egui::Window::new(fname)
                .id(egui::Id::new(("detail", item_id)))
                .open(&mut open)
                .collapsible(true).resizable(false)
                .default_size(Vec2::new(480.0, 360.0))
                .show(ui.ctx(), |ui| {
                    // ── Progress bar ──
                    ui.add_space(2.0);
                    let (bar_pct, bar_text) = if item.merge_progress > 0.0 {
                        let mp = item.merge_progress.clamp(0.0, 1.0) as f32;
                        let mpct = (mp * 100.0) as u32;
                        (mp, format!("Merging ({}%)", mpct.min(100)))
                    } else {
                        let pct = overall_pct.clamp(0.0, 1.0) as f32;
                        (pct, format!("{:.1}% — {:.1} MB / {:.1} MB",
                            pct * 100.0,
                            item.downloaded as f64 / 1_048_576.0,
                            item.total_size.max(item.downloaded) as f64 / 1_048_576.0))
                    };
                    ui.add(egui::ProgressBar::new(bar_pct)
                        .desired_width(ui.available_width())
                        .text(bar_text)
                        );

                    // ── URL ──
                    ui.add_space(2.0);
                    ui.hyperlink_to(
                        RichText::new(&item.url).size(11.0).color(Color32::from_rgb(0, 80, 180)),
                        &item.url,
                    );

                    // ── Info Card ──
                    ui.add_space(6.0);
                    Frame::NONE
                        .fill(Color32::from_rgb(245, 245, 248))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            // Fixed-width card: use available width but cap visual width
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                // Left: field:value pairs
                                ui.vertical(|ui| {
                                    let field_color = Color32::BLACK;
                                    let row_height = 18.0;

                                    // Helper: render one field:value row
                                    let label_width = 70.0;
                                    let val_width = 200.0;
                                    let mut row = |field: &str, value: &str, val_color: Color32| {
                                        ui.horizontal(|ui| {
                                            ui.add_sized(Vec2::new(label_width, row_height),
                                                egui::Label::new(RichText::new(field).size(12.0).color(field_color)));
                                            ui.add_sized(Vec2::new(val_width, row_height),
                                                egui::Label::new(RichText::new(value).size(12.0).color(val_color)));
                                        });
                                    };

                                    row("Size:", &format_size(item.total_size), Color32::BLACK);
                                    row("Speed:", &spd_str, Color32::BLACK);
                                    row("ETA:", &eta_str, Color32::BLACK);
                                    let proxy_color = if item.proxy_name.is_empty() { Color32::BLACK } else { Color32::from_rgb(0, 150, 0) };
                                    row("Proxy:", &proxy_str, proxy_color);
                                    if !resume_str.is_empty() {
                                        row("Resume:", &resume_str, Color32::from_rgb(0, 150, 0));
                                    }
                                    row("Created:", &item.created_at, Color32::BLACK);

                                    // Status with dynamic color
                                    let (status_text, status_color) = status_icon_and_text(&item.status);
                                    let pct = if item.total_size > 0 {
                                        ((item.downloaded as f64 / item.total_size as f64) * 100.0) as u32
                                    } else { 0 };
                                    let status_display = match &item.status {
                                        DownloadStatus::Failed(msg) if !msg.is_empty() => {
                                            let truncated = if msg.len() > 40 { format!("{}...", &msg[..37]) } else { msg.clone() };
                                            format!("{}: {}", status_text, truncated)
                                        },
                                        DownloadStatus::Downloading if item.total_size > 0 => format!("{} ({}%)", status_text, pct.min(100)),
                                        DownloadStatus::Paused if item.downloaded > 0 && item.total_size > 0 => format!("{} ({}%)", status_text, pct.min(100)),
                                        _ => status_text.to_string(),
                                    };
                                    row("Status:", &status_display, status_color);
                                    // Show retry count if any part has been retried
                                    let total_retries: u32 = item.parts.iter().map(|p| p.retries).sum();
                                    if total_retries > 0 {
                                        row("Retries:", &total_retries.to_string(), Color32::from_rgb(200, 100, 0));
                                    }

                                    drop(row); // release borrow before UA
                                    let ua_trunc = if self.settings.user_agent.len() > 20 {
                                        format!("{}.....", &self.settings.user_agent[..20])
                                    } else {
                                        self.settings.user_agent.clone()
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_sized(Vec2::new(label_width, row_height),
                                            egui::Label::new(RichText::new("UA:").size(12.0).color(field_color)));
                                        ui.add(egui::Label::new(
                                            RichText::new(&ua_trunc).size(11.0).color(Color32::BLACK)
                                        ));
                                    });
                                });

                                // Right: icon + filename (centered both axes)
                                ui.with_layout(
                                    Layout::top_down(Align::Center).with_main_align(Align::Center),
                                    |ui| {
                                    if let Some(tex) = &icon_texture {
                                        ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::new(48.0, 48.0)));
                                    } else {
                                        ui.add_space(48.0);
                                    }
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&item.file_name).size(11.0).color(Color32::BLACK).strong());
                                });
                            });
                        });

                    // ── Action buttons ──
                    ui.add_space(6.0);
                    let btn_size = Vec2::new(100.0, 26.0);
                    ui.horizontal(|ui| {
                        if matches!(item.status, DownloadStatus::Downloading) {
                            if ui.add_sized(btn_size, egui::Button::new("⏹ Stop")).clicked() {
                                actions.lock().unwrap().push((item_id, "stop"));
                            }
                        }
                        if matches!(item.status, DownloadStatus::Paused | DownloadStatus::Failed(_)) {
                            if ui.add_sized(btn_size, egui::Button::new("▶ Resume")).clicked() {
                                actions.lock().unwrap().push((item_id, "resume"));
                            }
                        }
                        if matches!(item.status, DownloadStatus::Completed) {
                            let sv = item.save_path.clone();
                            if ui.add_sized(btn_size, egui::Button::new("📂 Open Folder")).clicked() {
                                let p = sv;
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg("-R").arg(&p).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("explorer").arg("/select,").arg(&p).spawn();
                                #[cfg(target_os = "linux")]
                                if let Some(parent) = std::path::Path::new(&p).parent() {
                                    let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                                }
                            }
                        }
                        if ui.add_sized(btn_size, egui::Button::new("🗑 Delete")).clicked() {
                            actions.lock().unwrap().push((item_id, "delete"));
                        }
                    });

                    if has_parts {
                        ui.add_space(4.0);
                        ui.separator();
                        let part_done = parts.iter().filter(|p| p.status == PartStatus::Completed).count();
                        let incomplete_parts: Vec<&DownloadPart> = parts.iter().filter(|p| p.status != PartStatus::Completed).collect();
                        let showing = if part_done == parts.len() { "All completed" } else { "Incomplete:" };
                        ui.label(RichText::new(format!("Parts: {}/{} — {}", part_done, parts.len(), showing)).size(12.0).strong());
                        if !incomplete_parts.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt(("parts_list", item_id))
                                .max_height(90.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    for part in &incomplete_parts {
                                    let p_range = if part.end > part.start { part.end - part.start + 1 } else { 1 };
                                    let p_pct = (part.downloaded as f64 / p_range as f64).clamp(0.0, 1.0) as f32;
                                    let icon = match &part.status {
                                        PartStatus::Completed => "✅",
                                        PartStatus::Downloading => "⬇",
                                        PartStatus::Pending => "⏳",
                                        PartStatus::Failed(_) => "❌",
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("{} #{}", icon, part.index)).size(11.0).color(Color32::BLACK));
                                        ui.add_sized(Vec2::new(220.0, 14.0),
                                            egui::ProgressBar::new(p_pct).desired_width(220.0).text(format!("{:.1}%", p_pct * 100.0)));
                                    });
                                }
                            });
                        }

                        if is_merging {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Merging...").size(12.0).color(Color32::from_rgb(255, 170, 0)));
                        }
                    }
                });

            if !open {
                if is_manual { to_close_manual.push(item.id); }
                else { self.closed_detail_windows.insert(item.id); }
            }
        }

        for id in to_close_manual {
            self.manual_detail_ids.remove(&id);
        }
    }

    fn render_settings_window(&mut self, ui: &mut egui::Ui) {
        if !self.show_settings { return; }

        egui::Window::new("⚙ Settings")
            .id(egui::Id::new("settings_window"))
            .collapsible(false).resizable(true)
            .default_size(Vec2::new(520.0, 420.0))
            .show(ui.ctx(), |ui| {
                ui.label(RichText::new("Download Settings").strong().size(15.0));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Download Directory:");
                    let resp = ui.add_sized(Vec2::new(310.0, 24.0),
                        egui::TextEdit::singleline(&mut self.settings.download_dir));
                    if ui.add_sized(Vec2::new(80.0, 24.0), egui::Button::new("📂 Browse"))
                        .on_hover_text("Choose download folder").clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title("Choose Download Directory").pick_folder()
                        {
                            if let Some(path) = folder.to_str() {
                                self.settings.download_dir = path.to_string();
                            }
                        }
                    }
                    if !self.settings.download_dir.is_empty() {
                        resp.on_hover_text(&self.settings.download_dir);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Max Threads (per file):");
                    let conn_options = [("8", 8u32), ("16", 16), ("32", 32), ("64", 64)];
                    let current_idx = conn_options.iter().position(|(_, v)| *v == self.settings.max_connections).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("global_connections")
                        .selected_text(self.settings.max_connections.to_string())
                        .show_ui(ui, |ui| {
                            for (i, (label, _val)) in conn_options.iter().enumerate() {
                                if ui.selectable_label(sel == i, *label).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.settings.max_connections = conn_options[sel].1; }
                    ui.add_space(8.0);
                    ui.label(RichText::new("(8-64 concurrent connections)").size(11.0).color(Color32::GRAY));
                });

                ui.horizontal(|ui| {
                    ui.label("Max Retries:");
                    let retry_options = [("3", 3u32), ("5", 5), ("10", 10), ("20", 20), ("50", 50)];
                    let current_idx = retry_options.iter().position(|(_, v)| *v == self.settings.max_retries).unwrap_or(2);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("max_retries")
                        .selected_text(self.settings.max_retries.to_string())
                        .show_ui(ui, |ui| {
                            for (i, (label, _)) in retry_options.iter().enumerate() {
                                if ui.selectable_label(sel == i, *label).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.settings.max_retries = retry_options[sel].1; }
                    ui.add_space(8.0);
                    ui.label(RichText::new("(retry failed parts automatically)").size(11.0).color(Color32::GRAY));
                });

                ui.horizontal(|ui| {
                    ui.label("User-Agent:");
                    ui.add_sized(Vec2::new(310.0, 24.0),
                        egui::TextEdit::singleline(&mut self.settings.user_agent).hint_text("ProxyDM/0.1"));
                });

                // Cache section
                if self.cached_cache_size.is_none() {
                    let parts_dir = pdm_dir().join("parts");
                    self.cached_cache_size = if parts_dir.exists() {
                        Some(fs::read_dir(&parts_dir)
                            .map(|entries| entries.filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
                            .unwrap_or(0))
                    } else { Some(0) };
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Cache").strong().size(15.0));
                ui.separator();
                {
                    let parts_dir = pdm_dir().join("parts");
                    let cache_size = self.cached_cache_size.unwrap_or(0);
                    ui.horizontal(|ui| {
                        ui.label("Parts Cache:");
                        let size_str = format_size(cache_size);
                        if cache_size > 0 {
                            ui.label(RichText::new(&size_str).size(13.0).color(Color32::BLACK));
                            if ui.add_sized(Vec2::new(140.0, 28.0),
                                egui::Button::new(format!("🗑 Clear ({})", size_str)))
                                .on_hover_text("Delete all cached download part files").clicked()
                            {
                                let _ = fs::remove_dir_all(&parts_dir);
                                let _ = fs::create_dir_all(&parts_dir);
                                self.cached_cache_size = Some(0);
                            }
                        } else {
                            ui.label(RichText::new("Empty").size(13.0).color(Color32::GRAY));
                        }
                    });
                }

                // Home directory
                {
                    let home_dir = crate::types::pdm_dir();
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label("Home Dir:");
                        ui.add(egui::Label::new(
                            RichText::new(home_dir.to_string_lossy().to_string()).size(12.0).color(Color32::BLACK)
                        ).truncate());
                        if ui.add_sized(Vec2::new(110.0, 24.0),
                            egui::Button::new("📂 Open"))
                            .on_hover_text("Open .pdm directory in file manager")
                            .clicked()
                        {
                            let path = home_dir.to_string_lossy().to_string();
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(&path).spawn();
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("explorer").arg(&path).spawn();
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                        }
                    });
                }

                // Proxy Lists
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(RichText::new("Proxy Lists").strong().size(15.0));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Default Proxy:");
                    let names: Vec<String> = std::iter::once(String::new())
                        .chain(self.settings.proxies.iter().map(|p| p.name.clone())).collect();
                    let current_idx = names.iter().position(|n| *n == self.settings.default_proxy).unwrap_or(0);
                    let mut sel = current_idx;
                    egui::ComboBox::from_id_salt("default_proxy")
                        .selected_text(if self.settings.default_proxy.is_empty() { "None".to_string() } else { self.settings.default_proxy.clone() })
                        .show_ui(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                let display = if name.is_empty() { "None".to_string() } else { name.clone() };
                                if ui.selectable_label(sel == i, &display).clicked() { sel = i; }
                            }
                        });
                    if sel != current_idx { self.settings.default_proxy = if sel == 0 { String::new() } else { names[sel].clone() }; }
                });
                ui.add_space(4.0);

                // Proxy list
                Frame::NONE.inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(Vec2::new(120.0, 20.0), egui::Label::new(RichText::new("Name").strong().size(12.0)));
                        ui.add_sized(Vec2::new(60.0, 20.0), egui::Label::new(RichText::new("Type").strong().size(12.0)));
                        ui.add_sized(Vec2::new(130.0, 20.0), egui::Label::new(RichText::new("Host:Port").strong().size(12.0)));
                        ui.add_sized(Vec2::new(80.0, 20.0), egui::Label::new(RichText::new("").strong().size(12.0)));
                    });
                });

                let mut to_delete: Option<usize> = None;
                let mut to_edit: Option<usize> = None;
                ScrollArea::vertical().id_salt("proxy_list").max_height(140.0).show(ui, |ui| {
                    let btn_bg = ui.style().visuals.widgets.inactive.bg_fill;
                    for (i, proxy) in self.settings.proxies.iter().enumerate() {
                        let row_bg = if i % 2 == 0 { btn_bg } else {
                            Color32::from_rgb(
                                (btn_bg.r() as u16 + 5).min(255) as u8,
                                (btn_bg.g() as u16 + 5).min(255) as u8,
                                (btn_bg.b() as u16 + 5).min(255) as u8,
                            )
                        };
                        Frame::NONE.fill(row_bg).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized(Vec2::new(120.0, 20.0), egui::Label::new(RichText::new(&proxy.name).size(12.0)));
                                let proto = match proxy.protocol { ProxyProtocol::Http => "HTTP", ProxyProtocol::Socks5 => "SOCKS5" };
                                ui.add_sized(Vec2::new(60.0, 20.0), egui::Label::new(RichText::new(proto).size(12.0).color(Color32::BLACK)));
                                ui.add_sized(Vec2::new(130.0, 20.0), egui::Label::new(RichText::new(format!("{}:{}", proxy.host, proxy.port)).size(12.0).color(Color32::BLACK)));
                                if ui.add_sized(Vec2::new(40.0, 20.0), egui::Button::new("✏️")).clicked() { to_edit = Some(i); }
                                if ui.add_sized(Vec2::new(40.0, 20.0), egui::Button::new("🗑")).clicked() { to_delete = Some(i); }
                            });
                        });
                    }
                    if self.settings.proxies.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("No proxies configured. Click 'Add Proxy' to create one.").size(12.0).color(Color32::GRAY));
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("➕ Add Proxy").clicked() {
                        self.edit_proxy = Some(ProxyEntry {
                            name: String::new(), protocol: ProxyProtocol::Http,
                            host: String::new(), port: 8080,
                            username: String::new(), password: String::new(),
                        });
                        self.edit_proxy_index = None;
                        self.show_proxy_editor = true;
                    }
                });

                if let Some(idx) = to_delete {
                    if self.settings.default_proxy == self.settings.proxies[idx].name {
                        self.settings.default_proxy = String::new();
                    }
                    self.settings.proxies.remove(idx);
                }
                if let Some(idx) = to_edit {
                    self.edit_proxy = Some(self.settings.proxies[idx].clone());
                    self.edit_proxy_index = Some(idx);
                    self.show_proxy_editor = true;
                }

                // Proxy editor dialog
                if self.show_proxy_editor {
                    if let Some(ref mut proxy) = self.edit_proxy {
                        egui::Window::new(if self.edit_proxy_index.is_some() { "Edit Proxy" } else { "Add Proxy" })
                            .id(egui::Id::new("proxy_editor"))
                            .collapsible(false).resizable(false)
                            .default_size(Vec2::new(400.0, 260.0))
                            .show(ui.ctx(), |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Name:");
                                    ui.add_sized(Vec2::new(200.0, 24.0),
                                        egui::TextEdit::singleline(&mut proxy.name).hint_text("my-proxy"));
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Protocol:");
                                    let protos = ["HTTP", "SOCKS5"];
                                    let mut sel = if proxy.protocol == ProxyProtocol::Socks5 { 1 } else { 0 };
                                    egui::ComboBox::from_id_salt("proxy_proto")
                                        .selected_text(if sel == 0 { "HTTP" } else { "SOCKS5" })
                                        .show_ui(ui, |ui| {
                                            for (i, p) in protos.iter().enumerate() {
                                                if ui.selectable_label(sel == i, *p).clicked() { sel = i; }
                                            }
                                        });
                                    proxy.protocol = if sel == 0 { ProxyProtocol::Http } else { ProxyProtocol::Socks5 };
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Host:");
                                    ui.add_sized(Vec2::new(200.0, 24.0),
                                        egui::TextEdit::singleline(&mut proxy.host).hint_text("127.0.0.1"));
                                    ui.label("Port:");
                                    let mut port_str = proxy.port.to_string();
                                    if ui.add_sized(Vec2::new(60.0, 24.0),
                                        egui::TextEdit::singleline(&mut port_str).hint_text("8080")).changed() {
                                        proxy.port = port_str.parse().unwrap_or(8080);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Username:");
                                    ui.add_sized(Vec2::new(150.0, 24.0),
                                        egui::TextEdit::singleline(&mut proxy.username));
                                    ui.label("Password:");
                                    ui.add_sized(Vec2::new(150.0, 24.0),
                                        egui::TextEdit::singleline(&mut proxy.password).password(true));
                                });
                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.button("Save").clicked() && !proxy.name.is_empty() {
                                            if let Some(idx) = self.edit_proxy_index {
                                                self.settings.proxies[idx] = proxy.clone();
                                            } else {
                                                self.settings.proxies.push(proxy.clone());
                                            }
                                            self.show_proxy_editor = false;
                                        }
                                        ui.add_space(8.0);
                                        if ui.button("Cancel").clicked() { self.show_proxy_editor = false; }
                                    });
                                });
                            });
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_sized(Vec2::new(120.0, 28.0), egui::Button::new("Save & Close")).clicked() {
                            {
                                let set_path = crate::types::settings_path();
                                let _ = std::fs::create_dir_all(set_path.parent().unwrap());
                                crate::persist::save_toml(&set_path.to_string_lossy().to_string(), &self.settings);
                            }
                            self.set_status("Settings saved".to_string());
                            self.cached_cache_size = None;
                            self.show_settings = false;
                        }
                        ui.add_space(8.0);
                        if ui.add_sized(Vec2::new(80.0, 28.0), egui::Button::new("Cancel")).clicked() {
                            let set_path = crate::types::settings_path().to_string_lossy().to_string();
                            if let Some(s) = crate::persist::load_toml(&set_path) { self.settings = s; }
                            self.cached_cache_size = None;
                            self.show_settings = false;
                        }
                    });
                });
            });
    }

    fn render_about_window(&mut self, ui: &mut egui::Ui) {
        if !self.show_about { return; }

        egui::Window::new("ℹ About ProxyDM")
            .id(egui::Id::new("about_window"))
            .collapsible(false).resizable(false)
            .default_size(Vec2::new(350.0, 220.0))
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("ProxyDM").size(24.0).strong());
                    ui.label(RichText::new("Version 0.1.0").size(14.0).color(Color32::GRAY));
                    ui.add_space(12.0);
                    ui.label("A download manager built with Rust and egui");
                    ui.label("Supports HTTP/HTTPS downloads with pause/resume");
                    ui.label("and proxy configuration.");
                    ui.add_space(8.0);
                    ui.label(RichText::new("🔧 Proxy Download Manager").size(12.0).color(Color32::DARK_GRAY));
                    ui.add_space(6.0);
                    ui.hyperlink_to(
                        RichText::new("github.com/fb0sh/ProxyDownloadManager").size(12.0).color(Color32::from_rgb(0, 80, 180)),
                        "https://github.com/fb0sh/ProxyDownloadManager"
                    );
                    ui.add_space(2.0);
                    ui.label(RichText::new("fb0sh@outlook.com").size(12.0).color(Color32::from_rgb(0, 80, 180)));
                    ui.add_space(16.0);
                    if ui.add_sized(Vec2::new(100.0, 28.0), egui::Button::new("Close")).clicked() {
                        self.show_about = false;
                    }
                });
            });
    }
}

// ─── Download-URL heuristics ─────────────────────────────────────────────────

/// Check whether `url` looks like a downloadable file (archive, image, video,
/// installer, document, package, etc.) rather than a webpage.
fn looks_like_download_url(url: &str) -> bool {
    // Strip query string and fragment before checking path extension
    let path = url.split(|c| c == '?' || c == '#').next().unwrap_or(url);
    let path_lower = path.to_lowercase();

    // Multi-part extensions (e.g. .tar.gz, .tar.xz)
    let multi_extensions = [
        ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.lz",
        ".tgz", ".txz", ".tbz2", ".tzst",
    ];
    for ext in &multi_extensions {
        if path_lower.ends_with(ext) {
            return true;
        }
    }

    // Single-file extensions for downloadable content
    let single_extensions = [
        // Archives
        ".zip", ".rar", ".7z", ".tar", ".gz", ".xz", ".bz2", ".zst", ".lz", ".lzma",
        ".z", ".arj", ".cab", ".iso", ".img", ".vhd", ".vmdk",
        // Installers / executables
        ".exe", ".msi", ".dmg", ".pkg", ".apk", ".deb", ".rpm",
        ".AppImage", ".flatpak", ".snap", ".run", ".sh", ".bin",
        // Documents
        ".pdf", ".epub", ".mobi", ".djvu", ".chm",
        // Video
        ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm",
        ".m4v", ".ts", ".mts", ".3gp",
        // Audio
        ".mp3", ".flac", ".wav", ".aac", ".ogg", ".opus", ".m4a", ".wma",
        // Images (large/raw formats)
        ".psd", ".tiff", ".raw", ".cr2", ".nef", ".arw",
        ".bmp", ".pcx", ".tga",
        // Disk images / firmware
        ".ddf", ".hdi", ".sparsebundle",
        // Fonts
        ".ttf", ".otf", ".woff", ".woff2",
        // Code / libraries
        ".jar", ".war", ".ear", ".nupkg", ".whl", ".gem", ".crate",
        ".vsix", ".crx", ".xpi",
        // Virtual machines
        ".ova", ".ovf", ".vbox",
        // Other common download formats
        ".csv", ".json", ".xml", ".sql", ".db", ".sqlite",
        ".dmp", ".core",
    ];
    for ext in &single_extensions {
        if path_lower.ends_with(ext) {
            return true;
        }
    }

    false
}
