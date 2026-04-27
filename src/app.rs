use crate::config::{AppearanceMode, Config};
use crate::types::{AppState, MonitorCommand};
use crate::ui::design;
use crate::ui::Nav;
use eframe::egui;
use egui::{Align, Layout};
use std::sync::{mpsc, Arc, RwLock};

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
    last_nav: Nav,
    last_applied_appearance: Option<AppearanceMode>,
    last_system_theme: Option<eframe::Theme>,
}

impl PowerPlannerApp {
    pub fn new(
        state: Arc<RwLock<AppState>>,
        cmd_tx: mpsc::Sender<MonitorCommand>,
        config: Config,
        tray: Option<crate::tray::Tray>,
    ) -> Self {
        Self {
            state,
            cmd_tx,
            config,
            nav: Nav::default(),
            tray,
            bg_texture: None,
            waker_started: false,
            last_tooltip_plan: String::new(),
            last_nav: Nav::default(),
            last_applied_appearance: None,
            last_system_theme: None,
        }
    }
}

impl eframe::App for PowerPlannerApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.apply_appearance_if_needed(ctx, frame);

        if !self.waker_started {
            let ctx2 = ctx.clone();
            let cmd_tx2 = self.cmd_tx.clone();
            let ids = self.tray.as_ref().map(|t| {
                (
                    t.show_item_id.clone(),
                    t.balanced_item_id.clone(),
                    t.perf_item_id.clone(),
                    t.resume_item_id.clone(),
                    t.exit_item_id.clone(),
                )
            });
            let standard_guid = self.config.general.standard_plan_guid.clone();
            let perf_guid = self.config.general.performance_plan_guid.clone();
            std::thread::Builder::new()
                .name("tray-waker".into())
                .spawn(move || tray_event_thread(ctx2, cmd_tx2, ids, standard_guid, perf_guid))
                .ok();
            self.waker_started = true;
        }

        if ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        if self.bg_texture.is_none() {
            if let Ok(img) = image::load_from_memory(LOGO_PNG) {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.bg_texture =
                    Some(ctx.load_texture("bg_logo", color_image, egui::TextureOptions::LINEAR));
            }
        }

        if let Some(ref tray) = self.tray {
            let name = self
                .state
                .read()
                .unwrap()
                .current_plan
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".into());
            if name != self.last_tooltip_plan {
                tray.set_tooltip(&format!("PowerPlanner - {}", name));
                self.last_tooltip_plan = name;
            }
        }

        egui::SidePanel::left("nav").show(ctx, |ui| {
            ui.heading("PowerPlanner");
            ui.separator();
            ui.selectable_value(&mut self.nav, Nav::Dashboard, "Dashboard");
            ui.selectable_value(&mut self.nav, Nav::WatchedApps, "Watched Apps");
            ui.selectable_value(&mut self.nav, Nav::Settings, "Settings");
            ui.selectable_value(&mut self.nav, Nav::History, "Recent Events");

            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.add_space(8.0);
                appearance_cycle_button(ui, &mut self.config, &self.cmd_tx);
            });

            if let Some(ref tex) = self.bg_texture {
                let panel_rect = ui.clip_rect();
                let size = 80.0_f32;
                let margin = 8.0_f32;
                let img_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        panel_rect.right() - size - margin,
                        panel_rect.bottom() - size - margin,
                    ),
                    egui::vec2(size, size),
                );
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    uv,
                    watermark_tint(resolved_theme(
                        self.config.general.appearance_mode,
                        self.last_system_theme,
                    )),
                );
            }
        });

        if self.nav == Nav::Settings && self.last_nav != Nav::Settings {
            let _ = self.cmd_tx.send(MonitorCommand::RefreshPlans);
        }
        self.last_nav = self.nav.clone();

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
                    crate::ui::settings::render(
                        ui,
                        &mut self.config,
                        &self.cmd_tx,
                        &state.available_plans,
                    );
                }
                Nav::History => {
                    crate::ui::history::render(ui, &*state);
                }
            }
        });
    }
}

impl PowerPlannerApp {
    fn apply_appearance_if_needed(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let appearance = self.config.general.appearance_mode;
        let system_theme = frame.info().system_theme;
        let needs_update = self.last_applied_appearance != Some(appearance)
            || (appearance == AppearanceMode::System && self.last_system_theme != system_theme);

        if !needs_update {
            return;
        }

        let resolved_theme = resolved_theme(appearance, system_theme);
        ctx.set_visuals(visuals_for_theme(resolved_theme));
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(system_theme_command(
            appearance,
        )));
        self.last_applied_appearance = Some(appearance);
        self.last_system_theme = system_theme;
    }
}

fn resolved_theme(
    appearance: AppearanceMode,
    system_theme: Option<eframe::Theme>,
) -> eframe::Theme {
    match appearance {
        AppearanceMode::System => system_theme.unwrap_or(eframe::Theme::Dark),
        AppearanceMode::Light => eframe::Theme::Light,
        AppearanceMode::Dark => eframe::Theme::Dark,
    }
}

fn system_theme_command(appearance: AppearanceMode) -> egui::SystemTheme {
    match appearance {
        AppearanceMode::System => egui::SystemTheme::SystemDefault,
        AppearanceMode::Light => egui::SystemTheme::Light,
        AppearanceMode::Dark => egui::SystemTheme::Dark,
    }
}

fn visuals_for_theme(theme: eframe::Theme) -> egui::Visuals {
    let mut visuals = theme.egui_visuals();
    match theme {
        eframe::Theme::Dark => {
            visuals.override_text_color = Some(egui::Color32::from_rgb(230, 235, 242));
            visuals.panel_fill = design::color::DARK_PANEL;
            visuals.faint_bg_color = design::color::DARK_SURFACE;
            visuals.extreme_bg_color = design::color::DARK_INSET;
            visuals.widgets.noninteractive.bg_stroke.color = design::color::DARK_BORDER;
            visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_rgb(214, 221, 232);
            visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(45, 52, 64);
            visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(228, 233, 241);
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(58, 67, 82);
            visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
            visuals.selection.bg_fill = egui::Color32::from_rgb(0, 122, 163);
            visuals.hyperlink_color = egui::Color32::from_rgb(122, 198, 255);
        }
        eframe::Theme::Light => {
            visuals.override_text_color = Some(egui::Color32::from_rgb(49, 58, 72));
            visuals.panel_fill = design::color::LIGHT_PANEL;
            visuals.faint_bg_color = design::color::LIGHT_SURFACE;
            visuals.extreme_bg_color = design::color::LIGHT_INSET;
            visuals.widgets.noninteractive.bg_stroke.color = design::color::LIGHT_BORDER;
            visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_rgb(79, 91, 107);
        }
    }
    visuals
}

fn watermark_tint(theme: eframe::Theme) -> egui::Color32 {
    match theme {
        eframe::Theme::Dark => egui::Color32::from_rgba_unmultiplied(255, 255, 255, 52),
        eframe::Theme::Light => egui::Color32::from_rgba_unmultiplied(86, 108, 138, 160),
    }
}

fn next_appearance_mode(mode: AppearanceMode) -> AppearanceMode {
    match mode {
        AppearanceMode::System => AppearanceMode::Light,
        AppearanceMode::Light => AppearanceMode::Dark,
        AppearanceMode::Dark => AppearanceMode::System,
    }
}

fn appearance_label(mode: AppearanceMode) -> &'static str {
    match mode {
        AppearanceMode::System => "System",
        AppearanceMode::Light => "Light",
        AppearanceMode::Dark => "Dark",
    }
}

fn appearance_tooltip(mode: AppearanceMode) -> String {
    format!(
        "Appearance: {}. Click to switch to {}.",
        appearance_label(mode),
        appearance_label(next_appearance_mode(mode))
    )
}

fn appearance_cycle_button(
    ui: &mut egui::Ui,
    config: &mut Config,
    cmd_tx: &mpsc::Sender<MonitorCommand>,
) {
    let mode = config.general.appearance_mode;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    let response = response.on_hover_text(appearance_tooltip(mode));

    let visuals = ui.visuals();
    ui.painter()
        .rect(rect, 5.0, visuals.panel_fill, egui::Stroke::NONE);

    let icon_rect =
        egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(18.0, 18.0));
    draw_appearance_icon(ui.painter(), icon_rect, mode, visuals.text_color());

    if response.clicked() {
        config.general.appearance_mode = next_appearance_mode(mode);
        let _ = crate::config::save(config);
        let _ = cmd_tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn draw_appearance_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    mode: AppearanceMode,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.5, color);
    match mode {
        AppearanceMode::System => {
            let screen =
                egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 3.0), egui::vec2(14.0, 10.0));
            painter.rect_stroke(screen, 2.0, stroke);
            painter.line_segment(
                [
                    egui::pos2(rect.center().x, screen.bottom()),
                    egui::pos2(rect.center().x, rect.bottom() - 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(rect.center().x - 4.0, rect.bottom() - 2.0),
                    egui::pos2(rect.center().x + 4.0, rect.bottom() - 2.0),
                ],
                stroke,
            );
        }
        AppearanceMode::Light => {
            painter.circle_stroke(rect.center(), 4.3, stroke);
            for angle in [0.0_f32, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
                let radians = angle.to_radians();
                let direction = egui::vec2(radians.cos(), radians.sin());
                painter.line_segment(
                    [
                        rect.center() + direction * 6.6,
                        rect.center() + direction * 8.3,
                    ],
                    stroke,
                );
            }
        }
        AppearanceMode::Dark => {
            painter.circle_filled(rect.center(), 7.0, color);
            painter.circle_filled(
                rect.center() + egui::vec2(3.0, -2.0),
                7.0,
                painter.ctx().style().visuals.panel_fill,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_appearance_prefers_os_theme_when_available() {
        assert_eq!(
            resolved_theme(AppearanceMode::System, Some(eframe::Theme::Light)),
            eframe::Theme::Light
        );
    }

    #[test]
    fn system_appearance_falls_back_to_dark_when_os_theme_unknown() {
        assert_eq!(
            resolved_theme(AppearanceMode::System, None),
            eframe::Theme::Dark
        );
    }

    #[test]
    fn light_mode_uses_light_window_theme_command() {
        assert_eq!(
            system_theme_command(AppearanceMode::Light),
            egui::SystemTheme::Light
        );
    }

    #[test]
    fn appearance_mode_cycles_through_system_light_and_dark() {
        assert_eq!(
            next_appearance_mode(AppearanceMode::System),
            AppearanceMode::Light
        );
        assert_eq!(
            next_appearance_mode(AppearanceMode::Light),
            AppearanceMode::Dark
        );
        assert_eq!(
            next_appearance_mode(AppearanceMode::Dark),
            AppearanceMode::System
        );
    }

    #[test]
    fn light_mode_watermark_uses_darker_tint() {
        let tint = watermark_tint(eframe::Theme::Light);
        assert!(tint.a() > 120);
        assert!(tint.r() < 200);
    }
}

// Runs even when the window is hidden because Win32 ShowWindow bypasses eframe.
fn tray_event_thread(
    ctx: egui::Context,
    cmd_tx: mpsc::Sender<MonitorCommand>,
    ids: Option<(
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
        tray_icon::menu::MenuId,
    )>,
    standard_guid: String,
    perf_guid: String,
) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));

        while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                ..
            } = ev
            {
                win32_show_window();
                // Sync eframe's internal visibility state. win32_show_window bypasses
                // eframe, so without this eframe still thinks Visible=false and may
                // deduplicate the next ViewportCommand::Visible(false) as a no-op.
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.request_repaint();
            }
        }

        if let Some((ref show_id, ref balanced_id, ref perf_id, ref resume_id, ref exit_id)) = ids {
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if ev.id == *show_id {
                    win32_show_window();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.request_repaint();
                } else if ev.id == *balanced_id {
                    let _ = cmd_tx.send(MonitorCommand::ForcePlan(Some(standard_guid.clone())));
                } else if ev.id == *perf_id {
                    let _ = cmd_tx.send(MonitorCommand::ForcePlan(Some(perf_guid.clone())));
                } else if ev.id == *resume_id {
                    let _ = cmd_tx.send(MonitorCommand::ForcePlan(None));
                } else if ev.id == *exit_id {
                    let _ = cmd_tx.send(MonitorCommand::Stop);
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    std::process::exit(0);
                }
            }
        }
    }
}

/// Restore the PowerPlanner window via Win32 when eframe's update loop is not running.
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
