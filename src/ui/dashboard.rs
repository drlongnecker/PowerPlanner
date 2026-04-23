// src/ui/dashboard.rs
use crate::config::Config;
use crate::types::{AppState, CpuHistoryPoint, MonitorCommand};
use egui::{self, Align, Align2, Color32, Layout, Pos2, RichText, Sense, Shape, Stroke, Ui};
use std::collections::BTreeMap;
use std::sync::mpsc;

const CPU_GRAPH_MAX_WIDTH: f32 = 600.0;
const CPU_GRAPH_HEIGHT: f32 = 300.0;
const CPU_GRAPH_WINDOW_MINUTES: i64 = 15;
const DASHBOARD_TILE_SPACING: f32 = 10.0;
const DASHBOARD_CONTENT_INSET: f32 = 10.0;
const CPU_GRAPH_Y_MAX: f32 = 100.0;

#[derive(Clone, Copy)]
enum DashboardTileWidth {
    Half,
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CpuHistoryPlanKind, CpuHistoryPoint};
    use chrono::{Duration, Local};

    #[test]
    fn dashboard_copy_uses_standard_plan_label() {
        const BUTTON_LABEL: &str = "Set as Standard Plan";
        assert_eq!(BUTTON_LABEL, "Set as Standard Plan");
    }

    #[test]
    fn plan_time_breakdown_collapses_single_plan() {
        let now = Local::now();
        let mut history = std::collections::VecDeque::new();
        history.push_back(CpuHistoryPoint {
            ts: now,
            average_percent: 10.0,
            plan_kind: CpuHistoryPlanKind::Standard,
            plan_name: "Balanced".into(),
            trigger: "startup".into(),
        });
        history.push_back(CpuHistoryPoint {
            ts: now + Duration::minutes(5),
            average_percent: 12.0,
            plan_kind: CpuHistoryPlanKind::Standard,
            plan_name: "Balanced".into(),
            trigger: "startup".into(),
        });

        let breakdown = build_plan_time_breakdown(&history);

        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0].name, "Balanced");
        assert_eq!(breakdown[0].seconds, 300.0);
    }
}

pub fn render(
    ui: &mut Ui,
    state: &AppState,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
) {
    let plan_name = state
        .current_plan
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown");
    ui.heading(format!("Current Plan: {}", plan_name));
    ui.add_space(10.0);

    dashboard_content(ui, |ui| {
        dashboard_tile(ui, "Overview", DashboardTileWidth::Full, |ui| {
            if let Some(ref forced) = state.forced_plan {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::YELLOW, format!("Forced: {}", forced.name));
                    if ui.button("Resume Auto").clicked() {
                        let _ = tx.send(MonitorCommand::ForcePlan(None));
                    }
                });
                ui.add_space(6.0);
            }

            if let Some(ref err) = state.last_error {
                ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
                ui.add_space(6.0);
            }

            egui::Grid::new("dashboard_overview_grid")
                .num_columns(2)
                .spacing([18.0, 10.0])
                .min_col_width(140.0)
                .show(ui, |ui| {
                    summary_row(ui, "Power Source", power_source_text(state).as_str());
                    summary_row(ui, "Monitor", monitor_status_text(state));

                    if !state.matched_processes.is_empty() {
                        summary_row(ui, "Active Triggers", &state.matched_processes.join(", "));
                    }

                    if let Some(r) = state.hold_remaining_secs.filter(|r| *r > 0.0) {
                        summary_row(ui, "Hold Timer", &format!("{:.0}s remaining", r));
                    }

                    if let Some(idle_for_secs) = state.idle_for_secs {
                        summary_row(
                            ui,
                            "Idle",
                            &format!(
                                "{:.0}s / {}s",
                                idle_for_secs, config.general.idle_wait_seconds
                            ),
                        );
                    }

                    let cpu_text = if let Some(cpu_average_percent) = state.cpu_average_percent {
                        format!(
                            "{:.1}% / {}%",
                            cpu_average_percent, config.general.low_power_cpu_threshold_percent
                        )
                    } else {
                        format!(
                            "Gathering samples ({}s window)",
                            config.general.low_power_cpu_quiet_window_seconds
                        )
                    };
                    summary_row(ui, "CPU Quiet Window Avg", &cpu_text);

                    summary_row(
                        ui,
                        "Low Power Gates",
                        &format!(
                            "input={}  cpu={}",
                            if state.low_power_ready_input {
                                "ready"
                            } else {
                                "waiting"
                            },
                            if state.low_power_ready_cpu {
                                "ready"
                            } else {
                                "waiting"
                            }
                        ),
                    );
                });
        });

        ui.add_space(10.0);
        dashboard_two_up(
            ui,
            |ui| {
                dashboard_tile(ui, "Usage Trend", DashboardTileWidth::Full, |ui| {
                    ui.label(
                        RichText::new("Quiet-window CPU average over the last 15 minutes")
                            .weak()
                            .size(13.0),
                    );
                    ui.add_space(8.0);
                    render_cpu_history_chart(ui, state, config);
                });
            },
            |ui| {
                dashboard_tile(ui, "Plan Time", DashboardTileWidth::Full, |ui| {
                    ui.label(
                        RichText::new("Share of sampled time by active plan")
                            .weak()
                            .size(13.0),
                    );
                    ui.add_space(8.0);
                    render_plan_time_pie(ui, state);
                });
            },
        );
    });

    ui.add_space(10.0);
}

fn dashboard_content(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let content_width = (ui.available_width() - DASHBOARD_CONTENT_INSET * 2.0).max(320.0);
    ui.horizontal(|ui| {
        ui.add_space(DASHBOARD_CONTENT_INSET);
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, 0.0),
            Layout::top_down(Align::Min),
            add_contents,
        );
        ui.add_space(DASHBOARD_CONTENT_INSET);
    });
}

fn dashboard_tile(
    ui: &mut Ui,
    title: &str,
    width: DashboardTileWidth,
    add_contents: impl FnOnce(&mut Ui),
) {
    let tile_width = tile_width_for_available(ui.available_width(), width);
    show_dashboard_tile(ui, title, tile_width, add_contents);
}

fn show_dashboard_tile(
    ui: &mut Ui,
    title: &str,
    tile_width: f32,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(tile_width, 0.0),
        Layout::top_down(Align::Min),
        |ui| {
            egui::Frame::none()
                .fill(ui.visuals().faint_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                .show(ui, |ui| {
                    ui.set_width(tile_width - 28.0);
                    ui.heading(title);
                    ui.add_space(10.0);
                    add_contents(ui);
                });
        },
    );
}

fn tile_width_for_available(available: f32, width: DashboardTileWidth) -> f32 {
    match width {
        DashboardTileWidth::Full => available,
        DashboardTileWidth::Half => ((available - DASHBOARD_TILE_SPACING).max(200.0)) / 2.0,
    }
}

fn dashboard_two_up(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let row_width = ui.available_width();
    let tile_width = tile_width_for_available(row_width, DashboardTileWidth::Half);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(tile_width, 0.0),
            Layout::top_down(Align::Min),
            left,
        );
        ui.add_space(DASHBOARD_TILE_SPACING);
        ui.allocate_ui_with_layout(
            egui::vec2(tile_width, 0.0),
            Layout::top_down(Align::Min),
            right,
        );
    });
}

fn summary_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).weak().size(13.0));
    ui.label(value);
    ui.end_row();
}

fn power_source_text(state: &AppState) -> String {
    let bat = &state.battery;
    if bat.percent.is_none() {
        "Desktop (no battery)".to_string()
    } else if bat.on_battery {
        let pct = bat.percent.unwrap_or(0);
        format!(
            "On Battery - {}%{}",
            pct,
            if bat.charging { " (charging)" } else { "" }
        )
    } else {
        "AC Connected".to_string()
    }
}

fn monitor_status_text(state: &AppState) -> &'static str {
    if state.monitor_running {
        "Running"
    } else {
        "Stopped"
    }
}

fn render_cpu_history_chart(ui: &mut Ui, state: &AppState, config: &Config) {
    let desired_width = ui.available_width().min(CPU_GRAPH_MAX_WIDTH).max(160.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(desired_width, CPU_GRAPH_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 6.0, visuals.extreme_bg_color);
    painter.rect_stroke(rect, 6.0, visuals.widgets.noninteractive.bg_stroke);

    if state.cpu_history.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!(
                "Gathering CPU quiet-window samples ({}s window)",
                config.general.low_power_cpu_quiet_window_seconds
            ),
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        return;
    }

    let plot_rect = egui::Rect::from_min_max(
        Pos2::new(rect.left() + 38.0, rect.top() + 12.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 12.0),
    );
    let threshold = config.general.low_power_cpu_threshold_percent as f32;
    let history: Vec<_> = state.cpu_history.iter().cloned().collect();
    let y_max = CPU_GRAPH_Y_MAX;

    let latest = history.last().unwrap().ts;
    let window_start = latest - chrono::Duration::minutes(CPU_GRAPH_WINDOW_MINUTES);
    let total_millis = (latest - window_start).num_milliseconds().max(1) as f32;

    let to_x = |ts: chrono::DateTime<chrono::Local>| {
        let elapsed = (ts - window_start)
            .num_milliseconds()
            .clamp(0, total_millis as i64) as f32;
        plot_rect.left() + (elapsed / total_millis) * plot_rect.width()
    };
    let to_y =
        |value: f32| plot_rect.bottom() - (value / y_max).clamp(0.0, 1.0) * plot_rect.height();

    for percent in [0.0_f32, 20.0, 40.0, 60.0, 80.0, 100.0] {
        let y = to_y(percent);
        painter.line_segment(
            [
                Pos2::new(plot_rect.left(), y),
                Pos2::new(plot_rect.right(), y),
            ],
            Stroke::new(1.0, Color32::from_gray(50)),
        );
        painter.text(
            Pos2::new(plot_rect.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.0}", percent),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
    }

    let threshold_y = to_y(threshold);
    painter.line_segment(
        [
            Pos2::new(plot_rect.left(), threshold_y),
            Pos2::new(plot_rect.right(), threshold_y),
        ],
        Stroke::new(1.0, Color32::from_gray(120)),
    );
    painter.text(
        Pos2::new(plot_rect.left(), threshold_y - 4.0),
        Align2::LEFT_BOTTOM,
        format!("{}%", config.general.low_power_cpu_threshold_percent),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );

    if history.len() == 1 {
        let point = &history[0];
        let x = to_x(point.ts);
        let y = to_y(point.average_percent);
        painter.line_segment(
            [Pos2::new(x, plot_rect.bottom()), Pos2::new(x, y)],
            Stroke::new(2.0, point.plan_kind.color().gamma_multiply(0.85)),
        );
        painter.circle_filled(Pos2::new(x, y), 3.5, visuals.widgets.active.fg_stroke.color);
        return;
    }

    for segment in history.windows(2) {
        let left = &segment[0];
        let right = &segment[1];
        let x1 = to_x(left.ts);
        let x2 = to_x(right.ts);
        let y1 = to_y(left.average_percent);
        let y2 = to_y(right.average_percent);
        let fill = left.plan_kind.color().gamma_multiply(0.6);
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(x1, plot_rect.bottom()),
                Pos2::new(x1, y1),
                Pos2::new(x2, y2),
                Pos2::new(x2, plot_rect.bottom()),
            ],
            fill,
            Stroke::NONE,
        ));
    }

    let line_points: Vec<Pos2> = history
        .iter()
        .map(|point| Pos2::new(to_x(point.ts), to_y(point.average_percent)))
        .collect();
    painter.add(Shape::line(
        line_points.clone(),
        Stroke::new(2.0, visuals.widgets.active.fg_stroke.color),
    ));

    if let Some(pointer_pos) = response.hover_pos().filter(|pos| plot_rect.contains(*pos)) {
        if let Some((point, position)) =
            history
                .iter()
                .zip(line_points.iter())
                .min_by(|(_, left_pos), (_, right_pos)| {
                    (left_pos.x - pointer_pos.x)
                        .abs()
                        .total_cmp(&(right_pos.x - pointer_pos.x).abs())
                })
        {
            painter.line_segment(
                [
                    Pos2::new(position.x, plot_rect.top()),
                    Pos2::new(position.x, plot_rect.bottom()),
                ],
                Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color),
            );
            painter.circle_filled(*position, 4.5, Color32::WHITE);
            response.on_hover_ui_at_pointer(|ui| {
                ui.label(&point.plan_name);
                ui.label(format!("CPU: {:.1}%", point.average_percent));
                ui.label(format!("Trigger: {}", point.trigger));
                ui.label(point.ts.format("%Y-%m-%d %H:%M:%S").to_string());
            });
        }
    }
}

fn render_plan_time_pie(ui: &mut Ui, state: &AppState) {
    let desired_width = ui.available_width().min(CPU_GRAPH_MAX_WIDTH).max(160.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(desired_width, CPU_GRAPH_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 6.0, visuals.extreme_bg_color);
    painter.rect_stroke(rect, 6.0, visuals.widgets.noninteractive.bg_stroke);

    let breakdown = build_plan_time_breakdown(&state.cpu_history);
    if breakdown.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Waiting for enough chart history to summarize plan time",
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        return;
    }

    let center = Pos2::new(rect.left() + rect.width() * 0.33, rect.center().y);
    let radius = rect.height().min(rect.width() * 0.45) * 0.28;
    let total_seconds: f32 = breakdown.iter().map(|slice| slice.seconds).sum();

    if breakdown.len() == 1 {
        painter.circle_filled(center, radius, breakdown[0].color);
        painter.circle_filled(center, radius * 0.52, visuals.extreme_bg_color);
        render_plan_time_legend(
            &painter,
            rect,
            center,
            radius,
            visuals,
            &breakdown,
            total_seconds,
            ui,
        );
        return;
    }

    let mut start_angle = -std::f32::consts::FRAC_PI_2;

    for slice in &breakdown {
        let fraction = slice.seconds / total_seconds;
        let end_angle = start_angle + fraction * std::f32::consts::TAU;
        let mut points = vec![center];
        let segments = ((fraction * 48.0).ceil() as usize).max(3);
        for step in 0..=segments {
            let t = start_angle + (end_angle - start_angle) * (step as f32 / segments as f32);
            points.push(Pos2::new(
                center.x + radius * t.cos(),
                center.y + radius * t.sin(),
            ));
        }
        painter.add(Shape::convex_polygon(points, slice.color, Stroke::NONE));
        start_angle = end_angle;
    }

    painter.circle_filled(center, radius * 0.52, visuals.extreme_bg_color);

    render_plan_time_legend(
        &painter,
        rect,
        center,
        radius,
        visuals,
        &breakdown,
        total_seconds,
        ui,
    );
}

fn render_plan_time_legend(
    painter: &egui::Painter,
    rect: egui::Rect,
    center: Pos2,
    radius: f32,
    visuals: &egui::Visuals,
    breakdown: &[PlanTimeSlice],
    total_seconds: f32,
    ui: &Ui,
) {
    let legend_x = center.x + radius + 28.0;
    let mut legend_y = rect.top() + 28.0;
    for slice in breakdown {
        painter.rect_filled(
            egui::Rect::from_min_size(Pos2::new(legend_x, legend_y + 2.0), egui::vec2(10.0, 10.0)),
            2.0,
            slice.color,
        );
        painter.text(
            Pos2::new(legend_x + 18.0, legend_y),
            Align2::LEFT_TOP,
            format!(
                "{} - {} ({:.0}%)",
                slice.name,
                format_duration(slice.seconds),
                (slice.seconds / total_seconds) * 100.0
            ),
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.text_color(),
        );
        legend_y += 24.0;
    }
}

#[derive(Clone)]
struct PlanTimeSlice {
    name: String,
    color: Color32,
    seconds: f32,
}

fn build_plan_time_breakdown(
    history: &std::collections::VecDeque<CpuHistoryPoint>,
) -> Vec<PlanTimeSlice> {
    let mut totals: BTreeMap<String, PlanTimeSlice> = BTreeMap::new();
    let history_vec: Vec<_> = history.iter().cloned().collect();
    for pair in history_vec.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        let seconds = ((right.ts - left.ts).num_milliseconds().max(0) as f32) / 1000.0;
        if seconds <= 0.0 {
            continue;
        }
        totals
            .entry(left.plan_name.clone())
            .and_modify(|slice| {
                slice.seconds += seconds;
            })
            .or_insert_with(|| PlanTimeSlice {
                name: left.plan_name.clone(),
                color: left.plan_kind.color(),
                seconds,
            });
    }

    let mut slices: Vec<_> = totals.into_values().collect();
    slices.sort_by(|left, right| right.seconds.total_cmp(&left.seconds));
    slices
}

fn format_duration(seconds: f32) -> String {
    let total_seconds = seconds.round() as i64;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
