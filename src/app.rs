use crate::config::{AppearanceMode, Config};
use crate::types::{AppState, MonitorCommand};
use crate::ui::design;
use crate::ui::Nav;
use eframe::egui;
use egui::{Align, Layout};
use std::sync::{mpsc, Arc, RwLock};

const WATERMARK_PNG: &[u8] = include_bytes!("../assets/logo-mark-mono.png");

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
    show_close_confirmation: bool,
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
            show_close_confirmation: false,
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
            std::thread::Builder::new()
                .name("tray-waker".into())
                .spawn(move || tray_event_thread(ctx2, cmd_tx2, ids))
                .ok();
            self.waker_started = true;
        }

        let (minimized, close_requested) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                viewport.minimized.unwrap_or(false),
                viewport.close_requested(),
            )
        });
        match window_lifecycle_action(self.tray.is_some(), minimized, close_requested) {
            WindowLifecycleAction::None => {}
            WindowLifecycleAction::Hide => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            WindowLifecycleAction::CancelCloseAndPrompt => {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_close_confirmation = true;
            }
        }

        if self.bg_texture.is_none() {
            if let Ok(img) = image::load_from_memory(WATERMARK_PNG) {
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
            design::wordmark(ui);
            ui.separator();
            ui.add_space(4.0);
            if design::nav_item(
                ui,
                "Dashboard",
                design::NavIcon::Dashboard,
                self.nav == Nav::Dashboard,
            )
            .clicked()
            {
                self.nav = Nav::Dashboard;
            }
            if design::nav_item(
                ui,
                "Watched Apps",
                design::NavIcon::Apps,
                self.nav == Nav::WatchedApps,
            )
            .clicked()
            {
                self.nav = Nav::WatchedApps;
            }
            if design::nav_item(
                ui,
                "Power Usage",
                design::NavIcon::Power,
                self.nav == Nav::PowerUsage,
            )
            .clicked()
            {
                self.nav = Nav::PowerUsage;
            }
            if design::nav_item(
                ui,
                "Settings",
                design::NavIcon::Settings,
                self.nav == Nav::Settings,
            )
            .clicked()
            {
                self.nav = Nav::Settings;
            }
            if design::nav_item(
                ui,
                "Recent Events",
                design::NavIcon::History,
                self.nav == Nav::History,
            )
            .clicked()
            {
                self.nav = Nav::History;
            }

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
                Nav::PowerUsage => {
                    crate::ui::power_usage::render(ui, &mut self.config, &self.cmd_tx);
                }
                Nav::WatchedApps => {
                    crate::ui::watched::render(ui, &*state, &self.cmd_tx, &mut self.config);
                }
                Nav::Settings => {
                    crate::ui::settings::render(ui, &mut self.config, &self.cmd_tx, &*state);
                }
                Nav::History => {
                    crate::ui::history::render(ui, &*state);
                }
            }
        });

        if self.show_close_confirmation {
            if let Some(choice) = show_close_confirmation_dialog(ctx, self.tray.is_some()) {
                match close_dialog_action(choice, self.tray.is_some()) {
                    CloseDialogAction::Dismiss => {
                        self.show_close_confirmation = false;
                    }
                    CloseDialogAction::DismissAndHide => {
                        self.show_close_confirmation = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }
                    CloseDialogAction::Exit => {
                        let _ = self.cmd_tx.send(MonitorCommand::Stop);
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        std::process::exit(0);
                    }
                }
            }
        }
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
    let scale = rect.width() / 24.0;
    let stroke = egui::Stroke::new(1.5 * scale, color);
    let pt = |x: f32, y: f32| design::svg_point(rect, x, y);
    match mode {
        AppearanceMode::System => {
            let screen = egui::Rect::from_min_size(pt(4.0, 5.0), egui::vec2(16.0, 11.0) * scale);
            painter.rect_stroke(screen, 2.0 * scale, stroke);
            painter.line_segment([pt(12.0, 16.0), pt(12.0, 20.0)], stroke);
            painter.line_segment([pt(8.0, 20.0), pt(16.0, 20.0)], stroke);
            painter.circle_filled(pt(12.0, 10.5), 2.4 * scale, color);
        }
        AppearanceMode::Light => {
            painter.circle_stroke(pt(12.0, 12.0), 4.3 * scale, stroke);
            for (from, to) in [
                ((12.0, 3.0), (12.0, 5.0)),
                ((12.0, 19.0), (12.0, 21.0)),
                ((3.0, 12.0), (5.0, 12.0)),
                ((19.0, 12.0), (21.0, 12.0)),
                ((5.6, 5.6), (7.0, 7.0)),
                ((17.0, 17.0), (18.4, 18.4)),
                ((18.4, 5.6), (17.0, 7.0)),
                ((7.0, 17.0), (5.6, 18.4)),
            ] {
                painter.line_segment([pt(from.0, from.1), pt(to.0, to.1)], stroke);
            }
        }
        AppearanceMode::Dark => {
            painter.circle_filled(rect.center(), 8.0 * scale, color);
            painter.circle_filled(
                rect.center() + egui::vec2(3.5, -2.5) * scale,
                6.2 * scale,
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

    #[test]
    fn minimized_without_tray_keeps_window_available() {
        assert_eq!(
            window_lifecycle_action(false, true, false),
            WindowLifecycleAction::None
        );
    }

    #[test]
    fn minimized_with_tray_hides_to_tray() {
        assert_eq!(
            window_lifecycle_action(true, true, false),
            WindowLifecycleAction::Hide
        );
    }

    #[test]
    fn close_with_tray_cancels_close_and_prompts() {
        assert_eq!(
            window_lifecycle_action(true, false, true),
            WindowLifecycleAction::CancelCloseAndPrompt
        );
    }

    #[test]
    fn close_without_tray_cancels_close_and_prompts() {
        assert_eq!(
            window_lifecycle_action(false, false, true),
            WindowLifecycleAction::CancelCloseAndPrompt
        );
    }

    #[test]
    fn close_dialog_cancel_keeps_window_open() {
        assert_eq!(
            close_dialog_action(CloseDialogChoice::Cancel, true),
            CloseDialogAction::Dismiss
        );
    }

    #[test]
    fn close_dialog_close_exits_app() {
        assert_eq!(
            close_dialog_action(CloseDialogChoice::Close, true),
            CloseDialogAction::Exit
        );
    }

    #[test]
    fn close_dialog_minimize_to_tray_hides_when_tray_exists() {
        assert_eq!(
            close_dialog_action(CloseDialogChoice::MinimizeToTray, true),
            CloseDialogAction::DismissAndHide
        );
    }

    #[test]
    fn close_dialog_minimize_to_tray_is_unavailable_without_tray() {
        assert_eq!(
            close_dialog_action(CloseDialogChoice::MinimizeToTray, false),
            CloseDialogAction::Dismiss
        );
    }

    #[test]
    fn close_dialog_enter_uses_default_minimize_to_tray() {
        assert_eq!(
            close_dialog_keyboard_choice(true, false, true),
            Some(CloseDialogChoice::MinimizeToTray)
        );
    }

    #[test]
    fn close_dialog_enter_does_nothing_when_tray_is_unavailable() {
        assert_eq!(close_dialog_keyboard_choice(true, false, false), None);
    }

    #[test]
    fn close_dialog_escape_cancels() {
        assert_eq!(
            close_dialog_keyboard_choice(false, true, true),
            Some(CloseDialogChoice::Cancel)
        );
    }

    #[test]
    fn close_dialog_layout_is_compact() {
        let layout = close_dialog_layout();
        assert_eq!(layout.size, egui::vec2(448.0, 196.0));
        assert_eq!(layout.action_height, 64.0);
    }

    #[test]
    fn close_dialog_layout_has_room_for_action_buttons() {
        let layout = close_dialog_layout();
        let button_width = 128.0;
        let button_gap = 8.0;
        assert!(layout.size.x >= button_width * 3.0 + button_gap * 2.0 + 32.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowLifecycleAction {
    None,
    Hide,
    CancelCloseAndPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDialogChoice {
    Cancel,
    Close,
    MinimizeToTray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDialogAction {
    Dismiss,
    DismissAndHide,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CloseDialogLayout {
    size: egui::Vec2,
    action_height: f32,
}

fn window_lifecycle_action(
    has_tray: bool,
    minimized: bool,
    close_requested: bool,
) -> WindowLifecycleAction {
    if close_requested {
        WindowLifecycleAction::CancelCloseAndPrompt
    } else if minimized {
        if has_tray {
            WindowLifecycleAction::Hide
        } else {
            WindowLifecycleAction::None
        }
    } else {
        WindowLifecycleAction::None
    }
}

fn close_dialog_action(choice: CloseDialogChoice, has_tray: bool) -> CloseDialogAction {
    match choice {
        CloseDialogChoice::Cancel => CloseDialogAction::Dismiss,
        CloseDialogChoice::Close => CloseDialogAction::Exit,
        CloseDialogChoice::MinimizeToTray if has_tray => CloseDialogAction::DismissAndHide,
        CloseDialogChoice::MinimizeToTray => CloseDialogAction::Dismiss,
    }
}

fn show_close_confirmation_dialog(
    ctx: &egui::Context,
    has_tray: bool,
) -> Option<CloseDialogChoice> {
    let (enter_pressed, escape_pressed) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let mut choice = close_dialog_keyboard_choice(enter_pressed, escape_pressed, has_tray);
    let layout = close_dialog_layout();

    egui::Area::new("close_confirmation_scrim".into())
        .order(egui::Order::Foreground)
        .fixed_pos(egui::Pos2::ZERO)
        .interactable(false)
        .show(ctx, |ui| {
            let screen_rect = ctx.screen_rect();
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(18, 24, 33, 96),
            );
        });

    egui::Area::new("close_confirmation_dialog".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(ui.visuals().window_fill())
                .stroke(egui::Stroke::new(
                    1.0,
                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                ))
                .rounding(8.0)
                .shadow(egui::epaint::Shadow {
                    offset: egui::vec2(0.0, 18.0),
                    blur: 32.0,
                    spread: 0.0,
                    color: egui::Color32::from_black_alpha(56),
                })
                .show(ui, |ui| {
                    ui.set_min_size(layout.size);
                    ui.set_max_size(layout.size);

                    ui.vertical(|ui| {
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.add_space(22.0);
                            ui.vertical(|ui| {
                                ui.set_width(layout.size.x - 44.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Are you sure you want to close PowerPlanner?",
                                    )
                                    .size(20.0)
                                    .strong(),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Doing so will disable the ability to monitor power usage and elevate tasks.",
                                    )
                                    .size(design::type_size::LABEL),
                                );
                            });
                        });

                        ui.add_space(24.0);

                        egui::Frame::none()
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(egui::Margin::symmetric(16.0, 14.0))
                            .show(ui, |ui| {
                                ui.set_width(layout.size.x);
                                ui.set_height(layout.action_height);

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if close_dialog_button(
                                            ui,
                                            "Minimize to Tray",
                                            true,
                                            has_tray,
                                        )
                                        .on_disabled_hover_text("Tray is unavailable.")
                                        .clicked()
                                        {
                                            choice = Some(CloseDialogChoice::MinimizeToTray);
                                        }

                                        ui.add_space(8.0);

                                        if close_dialog_button(ui, "Close", false, true).clicked() {
                                            choice = Some(CloseDialogChoice::Close);
                                        }

                                        ui.add_space(8.0);

                                        if close_dialog_button(ui, "Cancel", false, true).clicked()
                                        {
                                            choice = Some(CloseDialogChoice::Cancel);
                                        }
                                    },
                                );
                            });
                    });
                });
        });

    choice
}

fn close_dialog_layout() -> CloseDialogLayout {
    CloseDialogLayout {
        size: egui::vec2(448.0, 196.0),
        action_height: 64.0,
    }
}

fn close_dialog_button(
    ui: &mut egui::Ui,
    label: &str,
    default: bool,
    enabled: bool,
) -> egui::Response {
    let text = if default {
        egui::RichText::new(label)
            .size(design::type_size::STATUS)
            .strong()
            .color(egui::Color32::WHITE)
    } else {
        egui::RichText::new(label).size(design::type_size::STATUS)
    };

    let mut button = egui::Button::new(text).rounding(4.0);
    if default {
        button = button
            .fill(ui.visuals().selection.bg_fill)
            .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color));
    } else {
        button = button.fill(ui.visuals().widgets.inactive.bg_fill);
    }

    ui.add_enabled_ui(enabled, |ui| ui.add_sized([128.0, 32.0], button))
        .inner
}

fn close_dialog_keyboard_choice(
    enter_pressed: bool,
    escape_pressed: bool,
    has_tray: bool,
) -> Option<CloseDialogChoice> {
    if escape_pressed {
        Some(CloseDialogChoice::Cancel)
    } else if enter_pressed && has_tray {
        Some(CloseDialogChoice::MinimizeToTray)
    } else {
        None
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
                    let _ = cmd_tx.send(MonitorCommand::ForceConfiguredStandard);
                } else if ev.id == *perf_id {
                    let _ = cmd_tx.send(MonitorCommand::ForceConfiguredPerformance);
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
