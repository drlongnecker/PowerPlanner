use crate::config::{Config, PowerUsageRangeMode};
use crate::db;
use crate::types::{CpuHistoryPlanKind, CpuHistoryPoint, MonitorCommand};
use crate::ui::design;
use egui::{self, Align2, Color32, Pos2, RichText, Sense, Shape, Stroke, Ui};
use std::sync::mpsc;

const POWER_GRAPH_HEIGHT: f32 = 360.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct PowerUsageSummary {
    sample_count: usize,
    latest_watts: f64,
    average_watts: f64,
    peak_watts: f64,
    estimated_cost_usd: f64,
    estimated_savings_usd: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CpuHistoryEnergyEstimate, CpuHistoryPlanKind, CpuHistoryPoint};
    use chrono::{Duration, Local};

    #[test]
    fn power_usage_summary_totals_estimated_cost_and_savings() {
        let now = Local::now();
        let history = vec![
            point(now, 10.0, 900, 18.0, 0.0000225, 0.00013375),
            point(
                now + Duration::seconds(30),
                15.0,
                3500,
                32.0,
                0.00004,
                0.00011625,
            ),
        ];

        let summary = build_power_usage_summary(&history).unwrap();

        assert_eq!(summary.sample_count, 2);
        assert_eq!(summary.latest_watts, 32.0);
        assert_eq!(summary.average_watts, 25.0);
        assert_eq!(summary.peak_watts, 32.0);
        assert!((summary.estimated_cost_usd - 0.0000625).abs() < 0.0000001);
        assert!((summary.estimated_savings_usd - 0.00025).abs() < 0.0000001);
    }

    fn point(
        ts: chrono::DateTime<Local>,
        average_percent: f32,
        current_mhz: u32,
        estimated_watts: f64,
        estimated_cost_usd: f64,
        estimated_savings_usd: f64,
    ) -> CpuHistoryPoint {
        CpuHistoryPoint {
            ts,
            average_percent,
            current_mhz: Some(current_mhz),
            base_mhz: Some(3500),
            plan_kind: CpuHistoryPlanKind::Standard,
            plan_name: "Balanced".into(),
            trigger: "standard".into(),
            energy: Some(CpuHistoryEnergyEstimate {
                estimated_watts,
                estimated_kwh: 0.00015,
                estimated_cost_usd,
                baseline_watts: 125.0,
                baseline_cost_usd: 0.00015625,
                estimated_savings_usd,
            }),
        }
    }
}

pub fn render(ui: &mut Ui, config: &mut Config, tx: &mpsc::Sender<MonitorCommand>) {
    let mut changed = false;
    let mut usage_window_minutes = config.general.usage_trend_window_minutes;
    let mut range_mode = config.general.power_usage_range_mode;
    let history = load_power_usage_history(config);

    crate::ui::padded_page(ui, |ui| {
        design::page_header(
            ui,
            "Power Usage",
            "Estimated CPU power, estimated cost, and savings from proper plan usage.",
        );

        design::section_with_header_action(
            ui,
            "Estimated CPU Cost",
            "Modeled CPU package power using sampled CPU average, sampled speed, active plan, and your manual energy rate.",
            |_| {},
            |ui| {
                if !config.general.energy_estimates_enabled {
                    ui.label(
                        RichText::new("Estimated CPU power is disabled in Settings > Energy.")
                            .weak()
                            .size(design::type_size::HELP),
                    );
                    return;
                }

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Time range")
                            .weak()
                            .size(design::type_size::HELP),
                    );
                    usage_range_selector(ui, &mut range_mode, &mut changed);
                    if range_mode == PowerUsageRangeMode::RecentMinutes {
                        usage_window_selector(ui, &mut usage_window_minutes, &mut changed);
                    }
                });
                ui.add_space(design::spacing::ROW_GAP);
                render_callouts(ui, &history);
                ui.add_space(design::spacing::SECTION_GAP);
                let available = ui.available_width();
                if available >= 920.0 {
                    let gap = design::spacing::SECTION_GAP;
                    let chart_width = ((available - gap) * 0.667).max(420.0);
                    let details_width = (available - gap - chart_width).max(260.0);
                    ui.horizontal_wrapped(|ui| {
                        render_chart_legend(ui);
                        ui.add_space(18.0);
                    });
                    ui.add_space(6.0);
                    ui.horizontal_top(|ui| {
                        let selected = ui
                            .allocate_ui(egui::vec2(chart_width, 0.0), |ui| {
                                render_power_usage_chart(ui, &history)
                            })
                            .inner;
                        ui.add_space(gap);
                        ui.allocate_ui(egui::vec2(details_width, 0.0), |ui| {
                            render_sample_details(ui, selected.as_ref(), POWER_GRAPH_HEIGHT);
                        });
                    });
                } else {
                    render_chart_legend(ui);
                    ui.add_space(18.0);
                    let selected = render_power_usage_chart(ui, &history);
                    ui.add_space(design::spacing::SECTION_GAP);
                    render_sample_details(ui, selected.as_ref(), 0.0);
                }
            },
        );
    });

    if changed {
        config.general.usage_trend_window_minutes = usage_window_minutes;
        config.general.power_usage_range_mode = range_mode;
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn load_power_usage_history(config: &Config) -> Vec<CpuHistoryPoint> {
    let Ok(conn) = db::open() else {
        return vec![];
    };
    match config.general.power_usage_range_mode {
        PowerUsageRangeMode::RecentMinutes => db::query_dashboard_samples_recent(
            &conn,
            config.general.usage_trend_window_minutes as i64,
        )
        .unwrap_or_default(),
        PowerUsageRangeMode::AllRetained => {
            db::query_all_dashboard_samples(&conn).unwrap_or_default()
        }
    }
}

fn usage_range_selector(ui: &mut Ui, range_mode: &mut PowerUsageRangeMode, changed: &mut bool) {
    egui::ComboBox::from_id_source("power_usage_range_combo")
        .selected_text(match *range_mode {
            PowerUsageRangeMode::RecentMinutes => "Recent",
            PowerUsageRangeMode::AllRetained => "All retained",
        })
        .width(124.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_value(range_mode, PowerUsageRangeMode::RecentMinutes, "Recent")
                .changed()
            {
                *changed = true;
            }
            if ui
                .selectable_value(range_mode, PowerUsageRangeMode::AllRetained, "All retained")
                .changed()
            {
                *changed = true;
            }
        });
}

fn usage_window_selector(ui: &mut Ui, usage_window_minutes: &mut u64, changed: &mut bool) {
    egui::ComboBox::from_id_source("power_usage_window_combo")
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

fn render_callouts(ui: &mut Ui, history: &[CpuHistoryPoint]) {
    if let Some(summary) = build_power_usage_summary(history) {
        let available = ui.available_width().max(220.0);
        let gap = design::spacing::ROW_GAP;
        let columns = if available >= 920.0 {
            5.0
        } else if available >= 760.0 {
            3.0
        } else if available >= 460.0 {
            2.0
        } else {
            1.0
        };
        let width = ((available - gap * (columns - 1.0)) / columns).max(160.0);
        egui::Grid::new("power_usage_callouts")
            .num_columns(columns as usize)
            .spacing([gap, gap])
            .show(ui, |ui| {
                let mut index = 0_usize;
                let mut add = |ui: &mut Ui, label: &str, value: String| {
                    callout(ui, label, &value, width);
                    index += 1;
                    if index % columns as usize == 0 {
                        ui.end_row();
                    }
                };
                add(
                    ui,
                    "Current est. CPU power",
                    format!("{:.0} W", summary.latest_watts),
                );
                add(
                    ui,
                    "Average est. CPU power",
                    format!("{:.0} W", summary.average_watts),
                );
                add(
                    ui,
                    "Peak est. CPU power",
                    format!("{:.0} W", summary.peak_watts),
                );
                add(
                    ui,
                    "Estimated cost",
                    format_money(summary.estimated_cost_usd),
                );
                add(
                    ui,
                    "Estimated savings",
                    format_money(summary.estimated_savings_usd),
                );
            });
    } else {
        ui.label(
            RichText::new("Waiting for estimated CPU power samples.")
                .weak()
                .size(design::type_size::HELP),
        );
    }
}

fn callout(ui: &mut Ui, label: &str, value: &str, width: f32) {
    egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(design::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).weak().size(design::type_size::HELP));
                ui.add_space(3.0);
                ui.label(RichText::new(value).strong().size(22.0));
            });
        });
}

fn render_power_usage_chart(ui: &mut Ui, history: &[CpuHistoryPoint]) -> Option<CpuHistoryPoint> {
    let desired_width = ui.available_width().max(160.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(desired_width, POWER_GRAPH_HEIGHT),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, design::radius::CONTROL, visuals.extreme_bg_color);
    painter.rect_stroke(
        rect,
        design::radius::CONTROL,
        visuals.widgets.noninteractive.bg_stroke,
    );

    let energy_points: Vec<_> = history
        .iter()
        .filter_map(|point| point.energy.map(|energy| (point, energy)))
        .collect();
    if energy_points.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No estimated CPU power samples in this range",
            egui::TextStyle::Body.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        return None;
    }

    let plot_rect = egui::Rect::from_min_max(
        Pos2::new(rect.left() + 48.0, rect.top() + 14.0),
        Pos2::new(rect.right() - 42.0, rect.bottom() - 30.0),
    );
    let first_ts = energy_points.first().unwrap().0.ts;
    let last_ts = energy_points.last().unwrap().0.ts;
    let total_millis = (last_ts - first_ts).num_milliseconds().max(1) as f32;
    let max_watts = energy_points
        .iter()
        .map(|(_, energy)| energy.baseline_watts.max(energy.estimated_watts))
        .fold(1.0_f64, f64::max)
        .ceil();
    let y_max = nice_watt_ceiling(max_watts);

    let to_x = |ts: chrono::DateTime<chrono::Local>| {
        let elapsed = (ts - first_ts)
            .num_milliseconds()
            .clamp(0, total_millis as i64) as f32;
        plot_rect.left() + (elapsed / total_millis) * plot_rect.width()
    };
    let watts_to_y = |value: f64| {
        plot_rect.bottom() - (value / y_max).clamp(0.0, 1.0) as f32 * plot_rect.height()
    };
    let cpu_to_y =
        |value: f32| plot_rect.bottom() - (value / 100.0).clamp(0.0, 1.0) * plot_rect.height();

    for fraction in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
        let watts = y_max * fraction;
        let y = watts_to_y(watts);
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
            format!("{:.0}", watts),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
        painter.text(
            Pos2::new(plot_rect.right() + 8.0, y),
            Align2::LEFT_CENTER,
            format!("{:.0}%", fraction * 100.0),
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
    }

    let baseline_points: Vec<Pos2> = energy_points
        .iter()
        .map(|(point, energy)| Pos2::new(to_x(point.ts), watts_to_y(energy.baseline_watts)))
        .collect();
    let watt_points: Vec<Pos2> = energy_points
        .iter()
        .map(|(point, energy)| Pos2::new(to_x(point.ts), watts_to_y(energy.estimated_watts)))
        .collect();
    let cpu_points: Vec<Pos2> = energy_points
        .iter()
        .map(|(point, _)| Pos2::new(to_x(point.ts), cpu_to_y(point.average_percent)))
        .collect();

    add_line(
        &painter,
        baseline_points,
        Stroke::new(1.5, design::color::WARNING),
    );
    add_line(
        &painter,
        watt_points.clone(),
        Stroke::new(2.2, CpuHistoryPlanKind::Performance.color()),
    );
    add_line(
        &painter,
        cpu_points,
        Stroke::new(1.4, visuals.text_color().gamma_multiply(0.72)),
    );

    painter.text(
        Pos2::new(plot_rect.left(), plot_rect.bottom() + 8.0),
        Align2::LEFT_TOP,
        first_ts.format("%Y-%m-%d %H:%M").to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );
    painter.text(
        Pos2::new(plot_rect.right(), plot_rect.bottom() + 8.0),
        Align2::RIGHT_TOP,
        last_ts.format("%Y-%m-%d %H:%M").to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );
    painter.text(
        Pos2::new(plot_rect.left(), plot_rect.top() - 2.0),
        Align2::LEFT_BOTTOM,
        "watts",
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );
    painter.text(
        Pos2::new(plot_rect.right(), plot_rect.top() - 2.0),
        Align2::RIGHT_BOTTOM,
        "CPU avg",
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
    );

    let mut selected = energy_points.last().map(|(point, energy)| {
        (
            (*point).clone(),
            *energy,
            watt_points.last().copied(),
            false,
        )
    });

    if let Some(pointer_pos) = response.hover_pos().filter(|pos| plot_rect.contains(*pos)) {
        if let Some(((point, energy), position)) = energy_points
            .iter()
            .zip(watt_points.iter())
            .min_by(|(_, left_pos), (_, right_pos)| {
                (left_pos.x - pointer_pos.x)
                    .abs()
                    .total_cmp(&(right_pos.x - pointer_pos.x).abs())
            })
        {
            selected = Some(((*point).clone(), *energy, Some(*position), true));
            painter.line_segment(
                [
                    Pos2::new(position.x, plot_rect.top()),
                    Pos2::new(position.x, plot_rect.bottom()),
                ],
                Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color),
            );
            painter.circle_filled(*position, 4.5, CpuHistoryPlanKind::Performance.color());
        }
    }

    if let Some((_, _, Some(position), _)) = selected.as_ref() {
        painter.circle_filled(*position, 4.5, CpuHistoryPlanKind::Performance.color());
    }

    selected.map(|(mut point, _, _, highlighted)| {
        if highlighted {
            point.trigger = format!("__highlighted__{}", point.trigger);
        }
        point
    })
}

fn render_chart_legend(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        legend_item(
            ui,
            CpuHistoryPlanKind::Performance.color(),
            "Estimated CPU watts",
        );
        legend_item(ui, design::color::WARNING, "Performance baseline watts");
        legend_item(
            ui,
            ui.visuals().text_color().gamma_multiply(0.72),
            "CPU average",
        );
    });
}

fn legend_item(ui: &mut Ui, color: Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 10.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.left(), rect.center().y),
            Pos2::new(rect.right(), rect.center().y),
        ],
        Stroke::new(2.0, color),
    );
    ui.label(RichText::new(label).weak().size(design::type_size::HELP));
    ui.add_space(8.0);
}

fn render_sample_details(ui: &mut Ui, selected: Option<&CpuHistoryPoint>, target_height: f32) {
    egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(design::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if target_height > 0.0 {
                ui.set_min_height((target_height - 20.0).max(236.0));
            } else {
                ui.set_min_height(236.0);
            }
            if let Some(point) = selected {
                let Some(energy) = point.energy else {
                    ui.label(
                        RichText::new("No estimated energy data for this sample.")
                            .weak()
                            .size(design::type_size::HELP),
                    );
                    return;
                };
                egui::Grid::new("power_usage_sample_details")
                    .num_columns(2)
                    .spacing([14.0, 8.0])
                    .min_col_width(92.0)
                    .show(ui, |ui| {
                        detail_row(
                            ui,
                            "Timestamp",
                            &point.ts.format("%Y-%m-%d %H:%M:%S").to_string(),
                        );
                        detail_row(ui, "Plan", &point.plan_name);
                        detail_row(
                            ui,
                            "Trigger",
                            point.trigger.trim_start_matches("__highlighted__"),
                        );
                        detail_row(ui, "CPU speed", &format_speed_pair(point));
                        detail_row(
                            ui,
                            "Est. CPU power",
                            &format!("{:.1} W", energy.estimated_watts),
                        );
                        detail_row(ui, "CPU average", &format!("{:.1}%", point.average_percent));
                        detail_row(ui, "Est. cost", &format_money(energy.estimated_cost_usd));
                        detail_row(
                            ui,
                            "Est. savings",
                            &format_money(energy.estimated_savings_usd),
                        );
                    });
            } else {
                ui.label(
                    RichText::new("No sample selected.")
                        .weak()
                        .size(design::type_size::HELP),
                );
            }
        });
}

fn detail_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).weak().size(design::type_size::HELP));
    ui.label(RichText::new(value).size(design::type_size::STATUS));
    ui.end_row();
}

fn add_line(painter: &egui::Painter, points: Vec<Pos2>, stroke: Stroke) {
    if points.len() > 1 {
        painter.add(Shape::line(points, stroke));
    } else if let Some(point) = points.first() {
        painter.circle_filled(*point, 3.5, stroke.color);
    }
}

fn build_power_usage_summary(history: &[CpuHistoryPoint]) -> Option<PowerUsageSummary> {
    let mut sample_count = 0;
    let mut latest_watts = 0.0;
    let mut total_watts = 0.0;
    let mut peak_watts = 0.0;
    let mut estimated_cost_usd = 0.0;
    let mut estimated_savings_usd = 0.0;

    for energy in history.iter().filter_map(|point| point.energy) {
        sample_count += 1;
        latest_watts = energy.estimated_watts;
        total_watts += energy.estimated_watts;
        peak_watts = f64::max(peak_watts, energy.estimated_watts);
        estimated_cost_usd += energy.estimated_cost_usd;
        estimated_savings_usd += energy.estimated_savings_usd;
    }

    (sample_count > 0).then_some(PowerUsageSummary {
        sample_count,
        latest_watts,
        average_watts: total_watts / sample_count as f64,
        peak_watts,
        estimated_cost_usd,
        estimated_savings_usd,
    })
}

fn nice_watt_ceiling(watts: f64) -> f64 {
    if watts <= 25.0 {
        25.0
    } else if watts <= 50.0 {
        50.0
    } else if watts <= 100.0 {
        100.0
    } else if watts <= 150.0 {
        150.0
    } else if watts <= 250.0 {
        250.0
    } else {
        (watts / 100.0).ceil() * 100.0
    }
}

fn format_money(value: f64) -> String {
    if value.abs() < 0.01 {
        format!("{:.3} cents", value * 100.0)
    } else {
        format!("${:.2}", value)
    }
}

fn format_speed_pair(point: &CpuHistoryPoint) -> String {
    match (point.current_mhz, point.base_mhz) {
        (Some(current), Some(base)) => {
            format!("{} / {} base", format_mhz(current), format_mhz(base))
        }
        (Some(current), None) => format_mhz(current),
        _ => "Unavailable".to_string(),
    }
}

fn format_mhz(mhz: u32) -> String {
    if mhz >= 1000 {
        format!("{:.2} GHz", mhz as f32 / 1000.0)
    } else {
        format!("{} MHz", mhz)
    }
}
