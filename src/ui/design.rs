use egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Ui};

pub(crate) mod type_size {
    pub(crate) const SECTION_TITLE: f32 = 18.0;
    pub(crate) const LABEL: f32 = 14.0;
    pub(crate) const HELP: f32 = 12.5;
    pub(crate) const STATUS: f32 = 13.0;
    pub(crate) const NAV: f32 = 14.0;
    pub(crate) const METRIC_VALUE: f32 = 28.0;
    pub(crate) const KICKER: f32 = 10.5;
    pub(crate) const HEADING: f32 = 26.0;
}

pub(crate) mod spacing {
    pub(crate) const PAGE_X: f32 = 24.0;
    pub(crate) const PAGE_Y: f32 = 18.0;
    pub(crate) const SECTION_GAP: f32 = 12.0;
    pub(crate) const SECTION_PAD_X: f32 = 16.0;
    pub(crate) const SECTION_PAD_Y: f32 = 14.0;
    pub(crate) const ROW_GAP: f32 = 10.0;
    pub(crate) const NAV_ROW_HEIGHT: f32 = 40.0;
    pub(crate) const NAV_ICON: f32 = 18.0;
}

pub(crate) mod radius {
    pub(crate) const SECTION: f32 = 8.0;
    pub(crate) const CONTROL: f32 = 6.0;
    pub(crate) const PILL: f32 = 999.0;
}

pub(crate) mod color {
    use egui::Color32;

    pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x00, 0xA9, 0xA5);
    pub(crate) const SUCCESS: Color32 = Color32::from_rgb(0x5C, 0xC4, 0x6C);
    pub(crate) const WARNING: Color32 = Color32::from_rgb(0xD2, 0xAA, 0x3C);
    pub(crate) const DANGER: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);
    pub(crate) const DARK_PANEL: Color32 = Color32::from_rgb(20, 24, 30);
    pub(crate) const DARK_SURFACE: Color32 = Color32::from_rgb(34, 40, 50);
    pub(crate) const DARK_INSET: Color32 = Color32::from_rgb(14, 18, 24);
    pub(crate) const DARK_BORDER: Color32 = Color32::from_rgb(76, 86, 101);
    pub(crate) const LIGHT_PANEL: Color32 = Color32::from_rgb(245, 247, 250);
    pub(crate) const LIGHT_SURFACE: Color32 = Color32::from_rgb(232, 237, 243);
    pub(crate) const LIGHT_INSET: Color32 = Color32::WHITE;
    pub(crate) const LIGHT_BORDER: Color32 = Color32::from_rgb(184, 194, 208);
}

#[derive(Clone, Copy)]
pub(crate) enum StatusKind {
    Success,
    Muted,
    Warning,
}

#[derive(Clone, Copy)]
pub(crate) enum NavIcon {
    Dashboard,
    Power,
    Apps,
    Settings,
    History,
}

pub(crate) fn enabled_status_text(enabled: bool) -> &'static str {
    if enabled {
        "On"
    } else {
        "Off"
    }
}

pub(crate) fn registered_status_text(registered: bool) -> &'static str {
    if registered {
        "Registered"
    } else {
        "Not registered"
    }
}

pub(crate) fn section(ui: &mut Ui, title: &str, description: &str, add_contents: impl FnOnce(&mut Ui)) {
    section_with_header_action(ui, title, description, |_| {}, add_contents);
}

pub(crate) fn section_with_header_action(
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
                    if !description.is_empty() {
                        ui.add_space(2.0);
                        ui.label(RichText::new(description).weak().size(type_size::HELP));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), add_action);
            });
            ui.add_space(spacing::ROW_GAP);
            add_contents(ui);
        });
}

pub(crate) fn setting_label(ui: &mut Ui, label: &str, description: &str) {
    ui.label(RichText::new(label).size(type_size::LABEL).strong());
    ui.add_space(2.0);
    ui.label(RichText::new(description).weak().size(type_size::HELP));
}

pub(crate) fn status_badge(ui: &mut Ui, text: &str, kind: StatusKind) -> egui::Response {
    status_badge_sized(ui, text, kind, false)
}

pub(crate) fn compact_status_badge(ui: &mut Ui, text: &str, kind: StatusKind) -> egui::Response {
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

pub(crate) fn subsection_heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(type_size::LABEL).strong());
}

/// Sidebar wordmark: "Power" light + "Planner" strong, per the brand lockup.
pub(crate) fn wordmark(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Power").size(type_size::HEADING));
        ui.label(RichText::new("Planner").size(type_size::HEADING).strong());
    });
}

pub(crate) fn tabs<T: Copy + PartialEq>(ui: &mut Ui, selected: &mut T, labels: &[(T, &str)]) {
    ui.horizontal(|ui| {
        for (value, label) in labels {
            ui.selectable_value(selected, *value, *label);
        }
    });
}

pub(crate) fn nav_item(ui: &mut Ui, label: &str, icon: NavIcon, selected: bool) -> egui::Response {
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

pub(crate) fn enabled_badge_button(ui: &mut Ui, enabled: bool) -> egui::Response {
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

pub(crate) fn command_button(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [96.0, 30.0],
        egui::Button::new(RichText::new(label).size(type_size::STATUS)).rounding(radius::CONTROL),
    )
}

pub(crate) fn accent_command_button(ui: &mut Ui, label: &str) -> egui::Response {
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

pub(crate) fn icon_button(ui: &mut Ui, label: &str, tooltip: &str, accent: Color32) -> egui::Response {
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

/// Maps a coordinate from the design system's 24x24 SVG icon viewBox onto `rect`.
pub(crate) fn svg_point(rect: egui::Rect, x: f32, y: f32) -> egui::Pos2 {
    rect.min + egui::vec2(x, y) * (rect.width() / 24.0)
}

fn draw_nav_icon(painter: &egui::Painter, rect: egui::Rect, icon: NavIcon, color: Color32) {
    let scale = rect.width() / 24.0;
    let stroke = Stroke::new(1.5 * scale, color);
    let pt = |x: f32, y: f32| svg_point(rect, x, y);
    match icon {
        NavIcon::Dashboard => {
            let tile = 7.5 * scale;
            let rx = 2.0 * scale;
            for (x, y) in [(3.0, 3.0), (13.5, 3.0), (3.0, 13.5), (13.5, 13.5)] {
                painter.rect_stroke(
                    egui::Rect::from_min_size(pt(x, y), egui::vec2(tile, tile)),
                    rx,
                    stroke,
                );
            }
        }
        NavIcon::Apps => {
            let body = egui::Rect::from_min_size(pt(3.0, 7.0), egui::vec2(18.0, 13.0) * scale);
            painter.rect_stroke(body, 2.0 * scale, stroke);
            painter.line_segment([pt(9.0, 7.0), pt(10.5, 3.5)], stroke);
            painter.line_segment([pt(15.0, 7.0), pt(13.5, 3.5)], stroke);
        }
        NavIcon::Power => {
            let bolt = [
                pt(14.0, 2.0),
                pt(6.0, 13.0),
                pt(11.0, 13.0),
                pt(9.5, 22.0),
                pt(18.0, 10.0),
                pt(13.0, 10.0),
                pt(14.0, 2.0),
            ];
            painter.add(egui::Shape::line(bolt.to_vec(), stroke));
        }
        NavIcon::Settings => {
            painter.circle_stroke(pt(12.0, 12.0), 5.4 * scale, stroke);
            painter.circle_stroke(pt(12.0, 12.0), 1.8 * scale, stroke);
            for (from, to) in [
                ((12.0, 4.0), (12.0, 6.2)),
                ((12.0, 17.8), (12.0, 20.0)),
                ((4.0, 12.0), (6.2, 12.0)),
                ((17.8, 12.0), (20.0, 12.0)),
                ((6.3, 6.3), (7.9, 7.9)),
                ((16.1, 16.1), (17.7, 17.7)),
                ((17.7, 6.3), (16.1, 7.9)),
                ((7.9, 16.1), (6.3, 17.7)),
            ] {
                painter.line_segment([pt(from.0, from.1), pt(to.0, to.1)], stroke);
            }
        }
        NavIcon::History => {
            painter.circle_stroke(pt(12.0, 12.0), 7.0 * scale, stroke);
            painter.line_segment([pt(12.0, 12.0), pt(12.0, 7.8)], stroke);
            painter.line_segment([pt(12.0, 12.0), pt(16.0, 14.6)], stroke);
            painter.line_segment([pt(3.5, 9.8), pt(6.7, 7.4)], stroke);
        }
    }
}

// ── DS primitives ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CalloutTone {
    Neutral,
    #[allow(dead_code)]
    Accent,
    #[allow(dead_code)]
    Warning,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ChipTone {
    Muted,
    #[allow(dead_code)]
    Accent,
    Warning,
}

fn blend_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let u = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * t + b.r() as f32 * u) as u8,
        (a.g() as f32 * t + b.g() as f32 * u) as u8,
        (a.b() as f32 * t + b.b() as f32 * u) as u8,
    )
}

pub(crate) fn callout(
    ui: &mut Ui,
    title: Option<&str>,
    tone: CalloutTone,
    add_body: impl FnOnce(&mut Ui),
) {
    let (border_color, extreme_bg) = {
        let visuals = ui.visuals();
        let base_border = visuals.noninteractive().bg_stroke.color;
        let border_color = match tone {
            CalloutTone::Neutral => base_border,
            CalloutTone::Accent => blend_color(color::ACCENT, base_border, 0.55),
            CalloutTone::Warning => blend_color(color::WARNING, base_border, 0.55),
        };
        (border_color, visuals.extreme_bg_color)
    };
    let frame = egui::Frame::none()
        .fill(extreme_bg)
        .stroke(egui::Stroke::new(1.0, border_color))
        .rounding(radius::CONTROL)
        .inner_margin(egui::Margin {
            left: 14.0,
            right: 14.0,
            top: 12.0,
            bottom: 12.0,
        });
    frame.show(ui, |ui| {
        if let Some(t) = title {
            ui.label(
                RichText::new(t)
                    .size(type_size::LABEL)
                    .strong()
                    .color(ui.visuals().text_color()),
            );
            ui.add_space(6.0);
        }
        add_body(ui);
    });
}

pub(crate) fn metric_tile(ui: &mut Ui, label: &str, value: &str, accent: bool) {
    let frame = {
        let visuals = ui.visuals();
        egui::Frame::none()
            .fill(visuals.extreme_bg_color)
            .stroke(egui::Stroke::new(1.0, visuals.noninteractive().bg_stroke.color))
            .rounding(radius::CONTROL)
            .inner_margin(egui::Margin {
                left: 14.0,
                right: 14.0,
                top: 12.0,
                bottom: 12.0,
            })
    };
    frame.show(ui, |ui| {
        ui.label(
            RichText::new(label)
                .size(type_size::HELP)
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(6.0);
        let value_color = if accent {
            color::ACCENT
        } else {
            ui.visuals().text_color()
        };
        ui.label(
            RichText::new(value)
                .size(type_size::METRIC_VALUE)
                .color(value_color),
        );
    });
}

pub(crate) fn info_row(ui: &mut Ui, label: &str, value: &str, mono: bool) {
    let value_font = if mono {
        egui::FontId::monospace(type_size::LABEL)
    } else {
        egui::FontId::proportional(type_size::LABEL)
    };
    let row_height = ui.fonts(|f| {
        f.row_height(&egui::FontId::proportional(type_size::LABEL))
            .max(f.row_height(&value_font))
    });
    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(150.0, row_height), |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(label)
                        .size(type_size::LABEL)
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
        ui.add_space(8.0);
        let rt = RichText::new(value).size(type_size::LABEL);
        let rt = if mono { rt.monospace() } else { rt };
        ui.label(rt);
    });
}

pub(crate) fn chip(ui: &mut Ui, text: &str, tone: ChipTone) -> egui::Response {
    let weak_text = ui.visuals().weak_text_color();
    let text_color = match tone {
        ChipTone::Muted => weak_text,
        ChipTone::Accent => color::ACCENT,
        ChipTone::Warning => color::WARNING,
    };
    let extreme_bg = ui.visuals().extreme_bg_color;
    let border_color = ui.visuals().noninteractive().bg_stroke.color;
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(type_size::STATUS),
        text_color,
    );
    let pad_x = 12.0_f32;
    let height = 26.0_f32;
    let desired = egui::vec2(galley.rect.width() + pad_x * 2.0, height);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            egui::Rounding::same(radius::PILL),
            extreme_bg,
            egui::Stroke::new(1.0, border_color),
        );
        ui.painter().galley(
            egui::pos2(
                rect.left() + pad_x,
                rect.center().y - galley.rect.height() / 2.0,
            ),
            galley,
            text_color,
        );
    }
    response
}

pub(crate) fn plan_pill(ui: &mut Ui, plan_name: &str) -> egui::Response {
    let lower = plan_name.to_lowercase();
    let weak_text = ui.visuals().weak_text_color();
    let (text_color, mix): (Color32, f32) = if lower.contains("ultimate") {
        (color::ACCENT, 0.60)
    } else if lower.contains("high performance") {
        (color::SUCCESS, 0.60)
    } else if lower.contains("power saver") || lower.contains("low power") {
        (weak_text, 0.0)
    } else {
        (color::WARNING, 0.60)
    };
    let base_border = ui.visuals().noninteractive().bg_stroke.color;
    let extreme_bg = ui.visuals().extreme_bg_color;
    let border_color = if mix > 0.0 {
        blend_color(text_color, base_border, mix)
    } else {
        base_border
    };
    let galley = ui.painter().layout_no_wrap(
        plan_name.to_string(),
        egui::FontId::proportional(type_size::STATUS),
        text_color,
    );
    let pad_x = 12.0_f32;
    let height = 24.0_f32;
    let desired = egui::vec2(galley.rect.width() + pad_x * 2.0, height);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            egui::Rounding::same(radius::PILL),
            extreme_bg,
            egui::Stroke::new(1.0, border_color),
        );
        ui.painter().galley(
            egui::pos2(
                rect.left() + pad_x,
                rect.center().y - galley.rect.height() / 2.0,
            ),
            galley,
            text_color,
        );
    }
    response
}
