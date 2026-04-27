use egui::{self, Align, Color32, Layout, RichText, Stroke, Ui};

pub mod type_size {
    pub const PAGE_TITLE: f32 = 24.0;
    pub const SECTION_TITLE: f32 = 18.0;
    pub const LABEL: f32 = 14.0;
    pub const HELP: f32 = 12.5;
    pub const STATUS: f32 = 13.0;
}

pub mod spacing {
    pub const PAGE_X: f32 = 24.0;
    pub const PAGE_Y: f32 = 18.0;
    pub const SECTION_GAP: f32 = 12.0;
    pub const SECTION_PAD_X: f32 = 16.0;
    pub const SECTION_PAD_Y: f32 = 14.0;
    pub const ROW_GAP: f32 = 10.0;
}

pub mod radius {
    pub const SECTION: f32 = 8.0;
    pub const CONTROL: f32 = 6.0;
    pub const PILL: f32 = 999.0;
}

pub mod color {
    use egui::Color32;

    pub const ACCENT: Color32 = Color32::from_rgb(0x00, 0xA9, 0xA5);
    pub const SUCCESS: Color32 = Color32::from_rgb(0x5C, 0xC4, 0x6C);
    pub const WARNING: Color32 = Color32::from_rgb(0xD2, 0xAA, 0x3C);
    pub const DANGER: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);
    pub const DARK_PANEL: Color32 = Color32::from_rgb(20, 24, 30);
    pub const DARK_SURFACE: Color32 = Color32::from_rgb(34, 40, 50);
    pub const DARK_INSET: Color32 = Color32::from_rgb(14, 18, 24);
    pub const DARK_BORDER: Color32 = Color32::from_rgb(76, 86, 101);
    pub const LIGHT_PANEL: Color32 = Color32::from_rgb(245, 247, 250);
    pub const LIGHT_SURFACE: Color32 = Color32::from_rgb(232, 237, 243);
    pub const LIGHT_INSET: Color32 = Color32::WHITE;
    pub const LIGHT_BORDER: Color32 = Color32::from_rgb(184, 194, 208);
}

#[derive(Clone, Copy)]
pub enum StatusKind {
    Success,
    Muted,
    Warning,
}

pub fn enabled_status_text(enabled: bool) -> &'static str {
    if enabled {
        "On"
    } else {
        "Off"
    }
}

pub fn registered_status_text(registered: bool) -> &'static str {
    if registered {
        "Registered"
    } else {
        "Not registered"
    }
}

pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(type_size::PAGE_TITLE).strong());
    ui.add_space(4.0);
    ui.label(RichText::new(subtitle).weak().size(type_size::LABEL));
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(spacing::SECTION_GAP);
}

pub fn section(ui: &mut Ui, title: &str, description: &str, add_contents: impl FnOnce(&mut Ui)) {
    section_with_header_action(ui, title, description, |_| {}, add_contents);
}

pub fn section_with_header_action(
    ui: &mut Ui,
    title: &str,
    description: &str,
    add_action: impl FnOnce(&mut Ui),
    add_contents: impl FnOnce(&mut Ui),
) {
    let section_width = ui.available_width();
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(radius::SECTION)
        .inner_margin(egui::Margin::symmetric(
            spacing::SECTION_PAD_X,
            spacing::SECTION_PAD_Y,
        ))
        .show(ui, |ui| {
            let inner_width = section_width - spacing::SECTION_PAD_X * 2.0;
            ui.set_width(inner_width);
            ui.set_max_width(inner_width);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(type_size::SECTION_TITLE).strong());
                    ui.add_space(2.0);
                    ui.label(RichText::new(description).weak().size(type_size::HELP));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), add_action);
            });
            ui.add_space(spacing::ROW_GAP);
            add_contents(ui);
        });
}

pub fn setting_label(ui: &mut Ui, label: &str, description: &str) {
    ui.label(RichText::new(label).size(type_size::LABEL).strong());
    ui.add_space(2.0);
    ui.label(RichText::new(description).weak().size(type_size::HELP));
}

pub fn status_badge(ui: &mut Ui, text: &str, kind: StatusKind) -> egui::Response {
    let accent = match kind {
        StatusKind::Success => color::SUCCESS,
        StatusKind::Muted => ui.visuals().weak_text_color(),
        StatusKind::Warning => color::WARNING,
    };
    let text_color = match kind {
        StatusKind::Muted => ui.visuals().text_color(),
        _ => accent,
    };
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        text_color,
    );
    let desired = egui::vec2((galley.size().x + 38.0).max(70.0), 26.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let fill = ui.visuals().extreme_bg_color;
    let stroke = Stroke::new(1.0, accent.gamma_multiply(0.65));
    ui.painter().rect(rect, radius::PILL, fill, stroke);

    let dot_center = egui::pos2(rect.left() + 14.0, rect.center().y);
    ui.painter().circle_filled(dot_center, 6.0, accent);
    if matches!(kind, StatusKind::Success) {
        draw_checkmark(ui, dot_center);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + 27.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    response
}

pub fn enabled_badge_button(ui: &mut Ui, enabled: bool) -> egui::Response {
    let text = enabled_status_text(enabled);
    let accent = if enabled {
        color::SUCCESS
    } else {
        ui.visuals().weak_text_color()
    };
    let text_color = if enabled {
        color::SUCCESS
    } else {
        ui.visuals().text_color()
    };
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        text_color,
    );
    let desired = egui::vec2((galley.size().x + 42.0).max(84.0), 30.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    let fill = if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().extreme_bg_color
    };
    ui.painter()
        .rect(rect, radius::PILL, fill, Stroke::new(1.0, accent));

    let dot_center = egui::pos2(rect.left() + 15.0, rect.center().y);
    ui.painter().circle_filled(dot_center, 6.0, accent);
    if enabled {
        draw_checkmark(ui, dot_center);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + 29.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    response
}

pub fn command_button(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [96.0, 30.0],
        egui::Button::new(RichText::new(label).size(type_size::STATUS)).rounding(radius::CONTROL),
    )
}

pub fn accent_command_button(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [76.0, 30.0],
        egui::Button::new(
            RichText::new(label)
                .size(type_size::STATUS)
                .color(color::ACCENT)
                .strong(),
        )
        .rounding(radius::CONTROL),
    )
}

pub fn icon_button(ui: &mut Ui, label: &str, tooltip: &str, accent: Color32) -> egui::Response {
    let button = egui::Button::new(
        RichText::new(label)
            .size(type_size::LABEL)
            .color(accent)
            .strong(),
    )
    .rounding(radius::PILL);
    ui.add_sized([28.0, 28.0], button).on_hover_text(tooltip)
}

fn draw_checkmark(ui: &Ui, center: egui::Pos2) {
    let check_stroke = Stroke::new(1.6, Color32::WHITE);
    ui.painter().line_segment(
        [
            center + egui::vec2(-3.0, 0.0),
            center + egui::vec2(-0.8, 2.4),
        ],
        check_stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-0.8, 2.4),
            center + egui::vec2(3.4, -3.0),
        ],
        check_stroke,
    );
}
