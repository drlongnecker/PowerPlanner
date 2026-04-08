// src/app.rs
use eframe::egui;
use std::sync::{mpsc, Arc, RwLock};
use crate::config::Config;
use crate::types::{AppState, MonitorCommand};
use crate::ui::Nav;

const LOGO_PNG: &[u8] = include_bytes!("../planner.png");

pub struct PowerPlannerApp {
    pub state: Arc<RwLock<AppState>>,
    pub cmd_tx: mpsc::Sender<MonitorCommand>,
    pub config: Config,
    pub nav: Nav,
    pub tray: Option<crate::tray::Tray>,
    bg_texture: Option<egui::TextureHandle>,
    waker_started: bool,
    last_tooltip_plan: String,
}

impl PowerPlannerApp {
    pub fn new(
        state: Arc<RwLock<AppState>>,
        cmd_tx: mpsc::Sender<MonitorCommand>,
        config: Config,
        tray: Option<crate::tray::Tray>,
    ) -> Self {
        Self { state, cmd_tx, config, nav: Nav::default(), tray, bg_texture: None, waker_started: false, last_tooltip_plan: String::new() }
    }
}

impl eframe::App for PowerPlannerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Spawn the tray-event thread once.
        // Root cause: ViewportCommand::Visible(false) stops winit from delivering WM_PAINT,
        // so request_repaint() never triggers update() while the window is hidden.
        // Fix: handle all tray/menu events in this thread using Win32 directly.
        if !self.waker_started {
            let ctx2 = ctx.clone();
            let cmd_tx2 = self.cmd_tx.clone();
            let ids = self.tray.as_ref().map(|t| (
                t.show_item_id.clone(),
                t.balanced_item_id.clone(),
                t.perf_item_id.clone(),
                t.exit_item_id.clone(),
            ));
            let idle_guid = self.config.general.idle_plan_guid.clone();
            let perf_guid = self.config.general.performance_plan_guid.clone();
            std::thread::Builder::new()
                .name("tray-waker".into())
                .spawn(move || tray_event_thread(ctx2, cmd_tx2, ids, idle_guid, perf_guid))
                .ok();
            self.waker_started = true;
        }

        // Minimize to tray: hide the window rather than keeping it in the taskbar
        if ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Lazily decode and upload the logo texture once
        if self.bg_texture.is_none() {
            if let Ok(img) = image::load_from_memory(LOGO_PNG) {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.bg_texture = Some(ctx.load_texture(
                    "bg_logo",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }

        // Update tray tooltip only when the plan name changes
        if let Some(ref tray) = self.tray {
            let name = self.state.read().unwrap().current_plan
                .as_ref().map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".into());
            if name != self.last_tooltip_plan {
                tray.set_tooltip(&format!("PowerPlanner — {}", name));
                self.last_tooltip_plan = name;
            }
        }

        egui::SidePanel::left("nav").show(ctx, |ui| {
            ui.heading("PowerPlanner");
            ui.separator();
            ui.selectable_value(&mut self.nav, Nav::Dashboard, "Dashboard");
            ui.selectable_value(&mut self.nav, Nav::WatchedApps, "Watched Apps");
            ui.selectable_value(&mut self.nav, Nav::Settings, "Settings");
            ui.selectable_value(&mut self.nav, Nav::History, "History");

            // Watermark: paint logo at bottom-left of the nav panel, low opacity
            if let Some(ref tex) = self.bg_texture {
                let panel_rect = ui.clip_rect();
                let size = 80.0_f32;
                let margin = 8.0_f32;
                let img_rect = egui::Rect::from_min_size(
                    egui::pos2(panel_rect.left() + margin, panel_rect.bottom() - size - margin),
                    egui::vec2(size, size),
                );
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    uv,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 45),
                );
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let state = self.state.read().unwrap();
            match self.nav {
                Nav::Dashboard => {
                    crate::ui::dashboard::render(ui, &*state, &mut self.config, &self.cmd_tx);
                }
                Nav::WatchedApps => {
                    crate::ui::watched::render(ui, &*state, &self.cmd_tx, &mut self.config);
                }
                Nav::Settings => {
                    crate::ui::settings::render(ui, &mut self.config, &self.cmd_tx, &state.available_plans);
                }
                Nav::History => {
                    crate::ui::history::render(ui, &*state);
                }
            }
        });
    }
}

// Polls tray icon and menu events at 100 ms intervals.
// Runs even when the window is hidden — Win32 ShowWindow bypasses eframe entirely.
fn tray_event_thread(
    ctx: egui::Context,
    cmd_tx: mpsc::Sender<MonitorCommand>,
    ids: Option<(
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
    )>,
    idle_guid: String,
    perf_guid: String,
) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Left-click on tray icon → restore window
        while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. } = ev {
                win32_show_window();
                // Sync eframe's internal visibility state — win32_show_window bypasses
                // eframe, so without this eframe still thinks Visible=false and will
                // deduplicate the next ViewportCommand::Visible(false) as a no-op.
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.request_repaint();
            }
        }

        // Tray context-menu items
        if let Some((ref show_id, ref balanced_id, ref perf_id, ref exit_id)) = ids {
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if ev.id == *show_id {
                    win32_show_window();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.request_repaint();
                } else if ev.id == *balanced_id {
                    let _ = cmd_tx.send(MonitorCommand::SwitchPlan(idle_guid.clone()));
                } else if ev.id == *perf_id {
                    let _ = cmd_tx.send(MonitorCommand::SwitchPlan(perf_guid.clone()));
                } else if ev.id == *exit_id {
                    let _ = cmd_tx.send(MonitorCommand::Stop);
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    std::process::exit(0);
                }
            }
        }
    }
}

/// Restore the PowerPlanner window via Win32 — works even when the window is
/// hidden and eframe's update() loop is not running.
#[cfg(windows)]
fn win32_show_window() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let title: Vec<u16> = "PowerPlanner\0".encode_utf16().collect();
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn win32_show_window() {}
