use egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Ui};

pub mod type_size {
    pub const PAGE_TITLE: f32 = 24.0;
    pub const SECTION_TITLE: f32 = 18.0;
    pub const LABEL: f32 = 14.0;
    pub const HELP: f32 = 12.5;
    pub const STATUS: f32 = 13.0;
    pub const NAV: f32 = 14.0;
}

pub mod spacing {
    pub const PAGE_X: f32 = 24.0;
    pub const PAGE_Y: f32 = 18.0;
    pub const SECTION_GAP: f32 = 12.0;
    pub const SECTION_PAD_X: f32 = 16.0;
    pub const SECTION_PAD_Y: f32 = 14.0;
    pub const ROW_GAP: f32 = 10.0;
    pub const NAV_ROW_HEIGHT: f32 = 40.0;
    pub const NAV_ICON: f32 = 18.0;
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

#[derive(Clone, Copy)]
pub enum NavIcon {
    Dashboard,
    Power,
    Apps,
    Settings,
    History,
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
    status_badge_sized(ui, text, kind, false)
}

pub fn compact_status_badge(ui: &mut Ui, text: &str, kind: StatusKind) -> egui::Response {
    status_badge_sized(ui, text, kind, true)
}

fn status_badge_sized(ui: &mut Ui, text: &str, kind: StatusKind, compact: bool) -> egui::Response {
    let accent = match kind {
        StatusKind::Success => color::SUCCESS,
        StatusKind::Muted => ui.visuals().weak_text_color(),
        StatusKind::Warning => color::WARNING,
    };
    let text_color = match kind {
        StatusKind::Muted => ui.visuals().text_color(),
        _ => accent,
    };
    let text_style = if compact {
        egui::TextStyle::Small
    } else {
        egui::TextStyle::Body
    };
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), text_style.resolve(ui.style()), text_color);
    let desired = if compact {
        egui::vec2((galley.size().x + 34.0).max(78.0), 23.0)
    } else {
        egui::vec2((galley.size().x + 48.0).max(92.0), 28.0)
    };
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let fill = ui.visuals().extreme_bg_color;
    let stroke = Stroke::new(1.0, accent.gamma_multiply(0.65));
    ui.painter().rect(rect, radius::PILL, fill, stroke);

    let dot_radius = if compact { 5.0 } else { 6.0 };
    let dot_center = egui::pos2(
        rect.left() + if compact { 12.0 } else { 14.0 },
        rect.center().y,
    );
    ui.painter().circle_filled(dot_center, dot_radius, accent);
    if matches!(kind, StatusKind::Success) {
        draw_checkmark(ui, dot_center);
    }
    ui.painter().galley(
        egui::pos2(
            rect.left() + if compact { 24.0 } else { 30.0 },
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );
    response
}

pub fn subsection_heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(type_size::LABEL).strong());
}

pub fn tabs<T: Copy + PartialEq>(ui: &mut Ui, selected: &mut T, labels: &[(T, &str)]) {
    ui.horizontal(|ui| {
        for (value, label) in labels {
            ui.selectable_value(selected, *value, *label);
        }
    });
}

pub fn nav_item(ui: &mut Ui, label: &str, icon: NavIcon, selected: bool) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), spacing::NAV_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let visuals = ui.visuals();

    let fill = if selected {
        color::ACCENT
    } else if response.hovered() {
        visuals.faint_bg_color
    } else {
        visuals.panel_fill
    };
    let stroke = if selected {
        Stroke::new(1.0, color::ACCENT)
    } else if response.hovered() {
        visuals.widgets.hovered.bg_stroke
    } else {
        Stroke::NONE
    };
    ui.painter().rect(rect, radius::CONTROL, fill, stroke);

    let content_color = if selected {
        Color32::WHITE
    } else {
        visuals.text_color()
    };
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + 12.0,
            rect.center().y - spacing::NAV_ICON / 2.0,
        ),
        egui::vec2(spacing::NAV_ICON, spacing::NAV_ICON),
    );
    draw_nav_icon(ui.painter(), icon_rect, icon, content_color);

    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(type_size::NAV),
        content_color,
    );
    ui.painter().galley(
        egui::pos2(
            icon_rect.right() + 10.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        content_color,
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

fn draw_nav_icon(painter: &egui::Painter, rect: egui::Rect, icon: NavIcon, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    match icon {
        NavIcon::Dashboard => {
            let gap = 2.0;
            let tile = (rect.width() - gap) / 2.0;
            for row in 0..2 {
                for col in 0..2 {
                    let min =
                        rect.min + egui::vec2(col as f32 * (tile + gap), row as f32 * (tile + gap));
                    painter.rect_stroke(
                        egui::Rect::from_min_size(min, egui::vec2(tile, tile)),
                        2.0,
                        stroke,
                    );
                }
            }
        }
        NavIcon::Apps => {
            let body = egui::Rect::from_min_size(
                rect.min + egui::vec2(2.0, 5.0),
                egui::vec2(rect.width() - 4.0, rect.height() - 7.0),
            );
            painter.rect_stroke(body, 2.0, stroke);
            painter.line_segment(
                [
                    egui::pos2(body.left() + 4.0, body.top()),
                    egui::pos2(body.left() + 6.0, rect.top() + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(body.right() - 4.0, body.top()),
                    egui::pos2(body.right() - 6.0, rect.top() + 2.0),
                ],
                stroke,
            );
        }
        NavIcon::Power => {
            let bolt = [
                rect.left_top() + egui::vec2(10.0, 1.5),
                rect.left_top() + egui::vec2(4.5, 9.5),
                rect.left_top() + egui::vec2(9.0, 9.5),
                rect.left_top() + egui::vec2(7.0, 16.5),
                rect.left_top() + egui::vec2(14.0, 7.5),
                rect.left_top() + egui::vec2(9.5, 7.5),
                rect.left_top() + egui::vec2(10.0, 1.5),
            ];
            painter.add(egui::Shape::line(bolt.to_vec(), stroke));
        }
        NavIcon::Settings => {
            painter.circle_stroke(rect.center(), 5.4, stroke);
            painter.circle_stroke(rect.center(), 1.8, stroke);
            for angle in [0.0_f32, 60.0, 120.0, 180.0, 240.0, 300.0] {
                let radians = angle.to_radians();
                let direction = egui::vec2(radians.cos(), radians.sin());
                painter.line_segment(
                    [
                        rect.center() + direction * 7.0,
                        rect.center() + direction * 8.8,
                    ],
                    stroke,
                );
            }
        }
        NavIcon::History => {
            painter.circle_stroke(rect.center(), 7.0, stroke);
            painter.line_segment(
                [rect.center(), rect.center() + egui::vec2(0.0, -4.2)],
                stroke,
            );
            painter.line_segment(
                [rect.center(), rect.center() + egui::vec2(4.0, 2.6)],
                stroke,
            );
            painter.line_segment(
                [
                    rect.left_top() + egui::vec2(1.5, 5.8),
                    rect.left_top() + egui::vec2(4.7, 3.4),
                ],
                stroke,
            );
        }
    }
}
