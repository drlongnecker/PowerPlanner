use crate::config::{Config, PlanTimeRangeMode};
use crate::db;
use crate::types::{AppState, CpuHistoryPoint, MonitorCommand};
use crate::ui::design;
use egui::{self, Align, Align2, Color32, Layout, Mesh, Pos2, RichText, Sense, Shape, Stroke, Ui};
use std::collections::BTreeMap;
use std::sync::mpsc;

const CPU_GRAPH_HEIGHT: f32 = 300.0;
const CPU_GRAPH_Y_MAX: f32 = 100.0;
const CPU_GATE_COLOR: Color32 = design::color::DANGER;
const TOP_TILE_HEIGHT: f32 = 280.0;

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
    fn plan_time_breakdown_collapses_single_plan() {
        let now = Local::now();
        let history = vec![
            CpuHistoryPoint {
                ts: now,
                average_percent: 10.0,
                plan_kind: CpuHistoryPlanKind::Standard,
                plan_name: "Balanced".into(),
                trigger: "startup".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(5),
                average_percent: 12.0,
                plan_kind: CpuHistoryPlanKind::Standard,
                plan_name: "Balanced".into(),
                trigger: "startup".into(),
            },
        ];

        let breakdown = build_plan_time_breakdown(&history);

        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0].name, "Balanced");
        assert_eq!(breakdown[0].seconds, 300.0);
    }

    #[test]
    fn plan_time_segments_group_by_plan_time_share() {
        let now = Local::now();
        let history = vec![
            CpuHistoryPoint {
                ts: now,
                average_percent: 10.0,
                plan_kind: CpuHistoryPlanKind::Standard,
                plan_name: "Balanced".into(),
                trigger: "startup".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(5),
                average_percent: 12.0,
                plan_kind: CpuHistoryPlanKind::Standard,
                plan_name: "Balanced".into(),
                trigger: "startup".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(10),
                average_percent: 8.0,
                plan_kind: CpuHistoryPlanKind::LowPower,
                plan_name: "Power saver".into(),
                trigger: "idle".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(12),
                average_percent: 12.0,
                plan_kind: CpuHistoryPlanKind::Standard,
                plan_name: "Balanced".into(),
                trigger: "startup".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(15),
                average_percent: 16.0,
                plan_kind: CpuHistoryPlanKind::Performance,
                plan_name: "High Performance".into(),
                trigger: "rustc.exe".into(),
            },
            CpuHistoryPoint {
                ts: now + Duration::minutes(20),
                average_percent: 15.0,
                plan_kind: CpuHistoryPlanKind::Performance,
                plan_name: "High Performance".into(),
                trigger: "rustc.exe".into(),
            },
        ];

        let segments = build_plan_time_segments(&history);

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].seconds, 780.0);
        assert_eq!(segments[1].seconds, 300.0);
        assert_eq!(segments[2].seconds, 120.0);
    }

    #[test]
    fn format_duration_uses_hours_for_large_values() {
        assert_eq!(format_duration(12_052.0), "3h 20m");
    }

    #[test]
    fn format_duration_uses_days_for_very_large_values() {
        assert_eq!(format_duration(200_000.0), "2d 7h");
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
    let (usage_history, plan_time_history) = load_dashboard_histories(config);
    let mut dashboard_preferences_changed = false;
    let mut usage_window_minutes = config.general.usage_trend_window_minutes;
    let mut plan_time_range_mode = config.general.plan_time_range_mode;

    crate::ui::padded_page(ui, |ui| {
        let top_row_width = ui.available_width();
        let top_tile_width = tile_width_for_available(top_row_width, DashboardTileWidth::Half);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            show_dashboard_tile(
                ui,
                "Overview",
                top_tile_width,
                Some(TOP_TILE_HEIGHT),
                |_| {},
                |ui| {
                    render_overview_tile(ui, state, config, tx, plan_name);
                },
            );
            ui.add_space(design::spacing::SECTION_GAP);
            show_dashboard_tile(
                ui,
                "Current State",
                top_tile_width,
                Some(TOP_TILE_HEIGHT),
                |_| {},
                |ui| {
                    render_current_state_tile(ui, state, config);
                },
            );
        });

        ui.add_space(design::spacing::SECTION_GAP);
        let row_width = ui.available_width();
        let tile_width = tile_width_for_available(row_width, DashboardTileWidth::Half);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let usage_window_label = usage_window_minutes;
            dashboard_tile(
                ui,
                "Usage Trend",
                DashboardTileWidth::Half,
                |ui| {
                    usage_trend_window_selector(
                        ui,
                        &mut usage_window_minutes,
                        &mut dashboard_preferences_changed,
                    )
                },
                |ui| {
                    ui.label(
                        RichText::new(format!(
                            "CPU average over the last {} minutes",
                            usage_window_label
                        ))
                        .weak()
                        .size(design::type_size::HELP),
                    );
                    ui.add_space(8.0);
                    render_cpu_history_chart(ui, &usage_history, config);
                },
            );
            ui.add_space(design::spacing::SECTION_GAP);
            ui.allocate_ui_with_layout(
                egui::vec2(tile_width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    let plan_time_subtitle_text = plan_time_subtitle(plan_time_range_mode);
                    dashboard_tile(
                        ui,
                        "Plan Time",
                        DashboardTileWidth::Full,
                        |ui| {
                            plan_time_range_selector(
                                ui,
                                &mut plan_time_range_mode,
                                &mut dashboard_preferences_changed,
                            )
                        },
                        |ui| {
                            ui.label(
                                RichText::new(plan_time_subtitle_text)
                                    .weak()
                                    .size(design::type_size::HELP),
                            );
                            ui.add_space(8.0);
                            render_plan_time_timeline(ui, &plan_time_history);
                        },
                    );
                },
            );
        });
    });

    if dashboard_preferences_changed {
        config.general.usage_trend_window_minutes = usage_window_minutes;
        config.general.plan_time_range_mode = plan_time_range_mode;
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn load_dashboard_histories(config: &Config) -> (Vec<CpuHistoryPoint>, Vec<CpuHistoryPoint>) {
    let Ok(conn) = db::open() else {
        return (vec![], vec![]);
    };

    let usage =
        db::query_dashboard_samples_recent(&conn, config.general.usage_trend_window_minutes as i64)
            .unwrap_or_default();
    let plan_time = match config.general.plan_time_range_mode {
        PlanTimeRangeMode::MatchUsageTrend => usage.clone(),
        PlanTimeRangeMode::AllRetained => {
            db::query_all_dashboard_samples(&conn).unwrap_or_default()
        }
    };
    (usage, plan_time)
}

fn plan_time_subtitle(plan_time_range_mode: PlanTimeRangeMode) -> &'static str {
    match plan_time_range_mode {
        PlanTimeRangeMode::MatchUsageTrend => {
            "Share of sampled time by active plan for the selected usage range"
        }
        PlanTimeRangeMode::AllRetained => {
            "Share of sampled time by active plan across all retained dashboard data"
        }
    }
}

fn usage_trend_window_selector(ui: &mut Ui, usage_window_minutes: &mut u64, changed: &mut bool) {
    egui::ComboBox::from_id_source("usage_trend_window_combo")
        .selected_text(format!("{}m", *usage_window_minutes))
        .width(96.0)
        .show_ui(ui, |ui| {
            for minutes in [15_u64, 30, 60, 90, 120] {
                if ui
                    .selectable_value(usage_window_minutes, minutes, format!("{}m", minutes))
                    .changed()
                {
                    *changed = true;
                }
            }
        });
}

fn plan_time_range_selector(
    ui: &mut Ui,
    plan_time_range_mode: &mut PlanTimeRangeMode,
    changed: &mut bool,
) {
    egui::ComboBox::from_id_source("plan_time_range_combo")
        .selected_text(match *plan_time_range_mode {
            PlanTimeRangeMode::MatchUsageTrend => "Match Usage Trend",
            PlanTimeRangeMode::AllRetained => "All retained",
        })
        .width(150.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_value(
                    plan_time_range_mode,
                    PlanTimeRangeMode::MatchUsageTrend,
                    "Match Usage Trend",
                )
                .changed()
            {
                *changed = true;
            }
            if ui
                .selectable_value(
                    plan_time_range_mode,
                    PlanTimeRangeMode::AllRetained,
                    "All retained",
                )
                .changed()
            {
                *changed = true;
            }
        });
}

fn dashboard_tile(
    ui: &mut Ui,
    title: &str,
    width: DashboardTileWidth,
    add_actions: impl FnOnce(&mut Ui),
    add_contents: impl FnOnce(&mut Ui),
) {
    let tile_width = tile_width_for_available(ui.available_width(), width);
    show_dashboard_tile(ui, title, tile_width, None, add_actions, add_contents);
}

fn show_dashboard_tile(
    ui: &mut Ui,
    title: &str,
    tile_width: f32,
    tile_height: Option<f32>,
    add_actions: impl FnOnce(&mut Ui),
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(tile_width, tile_height.unwrap_or(0.0)),
        Layout::top_down(Align::Min),
        |ui| {
            egui::Frame::none()
                .fill(ui.visuals().faint_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .rounding(design::radius::SECTION)
                .inner_margin(egui::Margin::symmetric(
                    design::spacing::SECTION_PAD_X,
                    design::spacing::SECTION_PAD_Y,
                ))
                .show(ui, |ui| {
                    ui.set_width(tile_width - design::spacing::SECTION_PAD_X * 2.0);
                    if let Some(tile_height) = tile_height {
                        ui.set_min_height(tile_height - design::spacing::SECTION_PAD_Y * 2.0);
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(title)
                                .size(design::type_size::SECTION_TITLE)
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            add_actions(ui);
                        });
                    });
                    ui.add_space(design::spacing::ROW_GAP);
                    add_contents(ui);
                });
        },
    );
}

fn tile_width_for_available(available: f32, width: DashboardTileWidth) -> f32 {
    match width {
        DashboardTileWidth::Full => available,
        DashboardTileWidth::Half => ((available - design::spacing::SECTION_GAP).max(200.0)) / 2.0,
    }
}

fn summary_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).weak().size(design::type_size::STATUS));
    ui.label(value);
    ui.end_row();
}

fn render_overview_tile(
    ui: &mut Ui,
    state: &AppState,
    config: &Config,
    tx: &mpsc::Sender<MonitorCommand>,
    plan_name: &str,
) {
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
            summary_row(ui, "Current Plan", plan_name);
            summary_row(ui, "Monitor", monitor_status_text(state));
            summary_row(
                ui,
                "Idle Wait",
                &format!("{}s", config.general.idle_wait_seconds),
            );
            summary_row(
                ui,
                "CPU Avg Window",
                &format!("{}s", config.general.cpu_average_window_seconds),
            );
            summary_row(
                ui,
                "Turbo Rescue",
                if config.general.turbo_rescue_enabled {
                    "enabled"
                } else {
                    "disabled"
                },
            );
            summary_row(
                ui,
                "Turbo Trigger",
                &format!(
                    ">{}%, {}s above base",
                    config.general.turbo_rescue_cpu_threshold_percent,
                    config.general.turbo_rescue_window_seconds
                ),
            );

            if !state.matched_processes.is_empty() {
                summary_row(ui, "Active Triggers", &state.matched_processes.join(", "));
            }

            if let Some(r) = state.hold_remaining_secs.filter(|r| *r > 0.0) {
                summary_row(ui, "Hold Timer", &format!("{:.0}s remaining", r));
            }
        });
}

fn render_current_state_tile(ui: &mut Ui, state: &AppState, config: &Config) {
    egui::Grid::new("dashboard_current_state_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .min_col_width(140.0)
        .show(ui, |ui| {
            summary_row(ui, "Power Source", power_source_text(state).as_str());

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

            if let Some(cpu_info) = &state.cpu_info {
                let cpu_name = if cpu_info.brand.is_empty() {
                    "Unknown CPU"
                } else {
                    cpu_info.brand.as_str()
                };
                summary_row(ui, "CPU", cpu_name);
                summary_row(
                    ui,
                    "Base Speed",
                    &cpu_info
                        .base_mhz
                        .map(format_mhz)
                        .unwrap_or_else(|| "Unavailable".to_string()),
                );
            } else {
                summary_row(ui, "CPU", "Unavailable");
            }

            summary_row(
                ui,
                "Current CPU Speed",
                &state
                    .cpu_frequency
                    .max_mhz
                    .map(format_mhz)
                    .unwrap_or_else(|| "Unavailable".to_string()),
            );

            let cpu_text = if let Some(cpu_average_percent) = state.cpu_average_percent {
                format!(
                    "{:.1}% / {}%",
                    cpu_average_percent, config.general.cpu_average_threshold_percent
                )
            } else {
                format!(
                    "Gathering samples ({}s window)",
                    config.general.cpu_average_window_seconds
                )
            };
            summary_row(
                ui,
                &format!(
                    "CPU Average ({}s)",
                    config.general.cpu_average_window_seconds
                ),
                &cpu_text,
            );
            summary_row(ui, "Turbo Rescue State", &state.turbo_rescue_state);

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

fn format_mhz(mhz: u32) -> String {
    if mhz >= 1000 {
        format!("{:.2} GHz", mhz as f32 / 1000.0)
    } else {
        format!("{} MHz", mhz)
    }
}

fn render_cpu_history_chart(ui: &mut Ui, history: &[CpuHistoryPoint], config: &Config) {
    let desired_width = ui.available_width().max(160.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(desired_width, CPU_GRAPH_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, design::radius::CONTROL, visuals.extreme_bg_color);
    painter.rect_stroke(
        rect,
        design::radius::CONTROL,
        visuals.widgets.noninteractive.bg_stroke,
    );

    if history.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!(
                "Gathering CPU quiet-window samples ({}s window)",
                config.general.cpu_average_window_seconds
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
    let threshold = config.general.cpu_average_threshold_percent as f32;
    let y_max = CPU_GRAPH_Y_MAX;
    let latest = chrono::Local::now();
    let window_start =
        latest - chrono::Duration::minutes(config.general.usage_trend_window_minutes as i64);
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
        Stroke::new(1.5, CPU_GATE_COLOR),
    );
    painter.text(
        Pos2::new(plot_rect.left(), threshold_y - 4.0),
        Align2::LEFT_BOTTOM,
        format!("{}%", config.general.cpu_average_threshold_percent),
        egui::TextStyle::Small.resolve(ui.style()),
        CPU_GATE_COLOR,
    );

    if history.len() == 1 {
        let point = &history[0];
        let x = to_x(point.ts);
        let y = to_y(point.average_percent);
        painter.line_segment(
            [Pos2::new(x, plot_rect.bottom()), Pos2::new(x, y)],
            Stroke::new(2.0, visuals.text_color()),
        );
        painter.circle_filled(Pos2::new(x, y), 3.5, visuals.text_color());
        return;
    }

    render_cpu_plan_fill(&painter, history, plot_rect, &to_x, &to_y);

    let line_points: Vec<Pos2> = history
        .iter()
        .map(|point| Pos2::new(to_x(point.ts), to_y(point.average_percent)))
        .collect();
    painter.add(Shape::line(
        line_points.clone(),
        Stroke::new(4.0, visuals.extreme_bg_color.gamma_multiply(0.85)),
    ));
    painter.add(Shape::line(
        line_points.clone(),
        Stroke::new(2.0, visuals.text_color()),
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
            painter.circle_filled(*position, 4.5, visuals.text_color());
            response.on_hover_ui_at_pointer(|ui| {
                ui.label(&point.plan_name);
                ui.label(format!("CPU: {:.1}%", point.average_percent));
                ui.label(format!("Trigger: {}", point.trigger));
                ui.label(point.ts.format("%Y-%m-%d %H:%M:%S").to_string());
            });
        }
    }
}

fn render_cpu_plan_fill(
    painter: &egui::Painter,
    history: &[CpuHistoryPoint],
    plot_rect: egui::Rect,
    to_x: &impl Fn(chrono::DateTime<chrono::Local>) -> f32,
    to_y: &impl Fn(f32) -> f32,
) {
    let mut mesh = Mesh::default();
    for pair in history.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        let fill = left.plan_kind.color().gamma_multiply(0.9);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(Pos2::new(to_x(left.ts), plot_rect.bottom()), fill);
        mesh.colored_vertex(Pos2::new(to_x(left.ts), to_y(left.average_percent)), fill);
        mesh.colored_vertex(Pos2::new(to_x(right.ts), plot_rect.bottom()), fill);
        mesh.colored_vertex(Pos2::new(to_x(right.ts), to_y(right.average_percent)), fill);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 2, base + 1, base + 3);
    }

    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn render_plan_time_timeline(ui: &mut Ui, history: &[CpuHistoryPoint]) {
    let desired_width = ui.available_width().max(160.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(desired_width, CPU_GRAPH_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, design::radius::CONTROL, visuals.extreme_bg_color);
    painter.rect_stroke(
        rect,
        design::radius::CONTROL,
        visuals.widgets.noninteractive.bg_stroke,
    );

    let segments = build_plan_time_segments(history);
    let breakdown = build_plan_time_breakdown(history);
    if breakdown.is_empty() || segments.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Waiting for enough chart history to summarize plan time",
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        return;
    }

    let timeline_rect = egui::Rect::from_min_max(
        Pos2::new(rect.left() + 20.0, rect.top() + 26.0),
        Pos2::new(rect.right() - 20.0, rect.top() + 92.0),
    );
    let total_seconds: f32 = breakdown.iter().map(|slice| slice.seconds).sum();
    let mut x = timeline_rect.left();
    for (index, segment) in segments.iter().enumerate() {
        let width = timeline_rect.width() * (segment.seconds / total_seconds);
        let segment_rect = egui::Rect::from_min_max(
            Pos2::new(x, timeline_rect.top()),
            Pos2::new(
                (x + width).min(timeline_rect.right()),
                timeline_rect.bottom(),
            ),
        );
        painter.rect_filled(
            segment_rect,
            segment_rounding(index, segments.len()),
            segment.color,
        );
        x += width;
    }
    painter.rect_stroke(timeline_rect, 6.0, visuals.widgets.noninteractive.bg_stroke);

    if let (Some(first), Some(last)) = (history.first(), history.last()) {
        painter.text(
            Pos2::new(timeline_rect.left(), timeline_rect.bottom() + 8.0),
            Align2::LEFT_TOP,
            first.ts.format("%Y-%m-%d %H:%M").to_string(),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        painter.text(
            Pos2::new(timeline_rect.right(), timeline_rect.bottom() + 8.0),
            Align2::RIGHT_TOP,
            last.ts.format("%Y-%m-%d %H:%M").to_string(),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
    }

    render_plan_time_legend(&painter, rect, visuals, &breakdown, total_seconds, ui);
}

fn render_plan_time_legend(
    painter: &egui::Painter,
    rect: egui::Rect,
    visuals: &egui::Visuals,
    breakdown: &[PlanTimeSlice],
    total_seconds: f32,
    ui: &Ui,
) {
    let legend_x = rect.left() + 22.0;
    let mut legend_y = rect.top() + 126.0;
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

#[derive(Clone)]
struct PlanTimeSegment {
    color: Color32,
    seconds: f32,
}

fn build_plan_time_segments(history: &[CpuHistoryPoint]) -> Vec<PlanTimeSegment> {
    build_plan_time_breakdown(history)
        .into_iter()
        .map(|slice| PlanTimeSegment {
            color: slice.color,
            seconds: slice.seconds,
        })
        .collect()
}

fn build_plan_time_breakdown(history: &[CpuHistoryPoint]) -> Vec<PlanTimeSlice> {
    let mut totals: BTreeMap<String, PlanTimeSlice> = BTreeMap::new();
    for pair in history.windows(2) {
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
    let total_minutes = total_seconds / 60;
    let total_hours = total_minutes / 60;
    let total_days = total_hours / 24;
    if total_days > 0 {
        let hours = total_hours % 24;
        if hours > 0 {
            return format!("{}d {}h", total_days, hours);
        }
        return format!("{}d", total_days);
    }
    if total_hours > 0 {
        let minutes = total_minutes % 60;
        if minutes > 0 {
            return format!("{}h {}m", total_hours, minutes);
        }
        return format!("{}h", total_hours);
    }
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn segment_rounding(index: usize, total_segments: usize) -> egui::Rounding {
    if total_segments <= 1 {
        return egui::Rounding::same(6.0);
    }

    let mut rounding = egui::Rounding::same(0.0);
    if index == 0 {
        rounding.nw = 6.0;
        rounding.sw = 6.0;
    }
    if index + 1 == total_segments {
        rounding.ne = 6.0;
        rounding.se = 6.0;
    }
    rounding
}
