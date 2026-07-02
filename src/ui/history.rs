use crate::types::{AppState, PowerEvent};
use crate::ui::design;
use chrono::{DateTime, Duration, Local, NaiveDate};
use egui::{RichText, Stroke, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventPlanKind {
    LowPower,
    Standard,
    Performance,
    Other,
}

struct HistorySummary {
    event_count: usize,
    last_switch: String,
    high_performance_trigger: String,
}

pub(crate) fn render(ui: &mut Ui, state: &AppState) {
    crate::ui::padded_page(ui, |ui| {
        design::section_with_header_action(
            ui,
            "Recent Events",
            "Review plan changes, triggers, and power source context.",
            |ui| {
                if design::command_button(ui, "Open Log").clicked() {
                    open_log();
                }
                if design::command_button(ui, "Export CSV").clicked() {
                    export_to_desktop();
                }
            },
            |ui| {
                render_summary(ui, &build_summary(&state.recent_events));
                ui.add_space(design::spacing::ROW_GAP);
                render_event_feed(ui, state);
            },
        );
    });
}

fn render_summary(ui: &mut Ui, summary: &HistorySummary) {
    ui.horizontal_wrapped(|ui| {
        design::chip(ui, &format!("{} events", summary.event_count), design::ChipTone::Muted);
        ui.add_space(10.0);
        design::chip(ui, &format!("Last switch: {}", summary.last_switch), design::ChipTone::Muted);
        if summary.high_performance_trigger != "-" {
            ui.add_space(10.0);
            design::chip(
                ui,
                &format!("High Performance trigger: {}", summary.high_performance_trigger),
                design::ChipTone::Warning,
            );
        }
    });
}

fn render_event_feed(ui: &mut Ui, state: &AppState) {
    if state.recent_events.is_empty() {
        ui.label(
            RichText::new("No recent plan events yet.")
                .weak()
                .size(design::type_size::LABEL),
        );
        return;
    }

    let now = Local::now();
    let mut last_group: Option<NaiveDate> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for event in &state.recent_events {
                let event_date = event.ts.date_naive();
                if last_group != Some(event_date) {
                    if last_group.is_some() {
                        ui.add_space(design::spacing::ROW_GAP);
                    }
                    ui.label(
                        RichText::new(date_group_label(event.ts, now))
                            .weak()
                            .strong()
                            .size(design::type_size::STATUS),
                    );
                    ui.add_space(4.0);
                    last_group = Some(event_date);
                }
                render_event_row(ui, event);
            }
        });
}

fn render_event_row(ui: &mut Ui, event: &PowerEvent) {
    let border_color = ui.visuals().widgets.noninteractive.bg_stroke.color;
    egui::Frame::none()
        .stroke(Stroke::new(1.0, border_color))
        .rounding(design::radius::SECTION)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                let time_str = event.ts.format("%H:%M:%S").to_string();
                ui.allocate_ui_with_layout(
                    egui::vec2(80.0, 36.0),
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        ui.label(
                            RichText::new(&time_str)
                                .monospace()
                                .size(design::type_size::LABEL),
                        );
                    },
                );
                ui.add_space(6.0);
                design::plan_pill(ui, &event.plan_name);
                ui.add_space(6.0);
                ui.label(
                    RichText::new(trigger_label(&event.trigger))
                        .size(design::type_size::LABEL),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    design::chip(
                        ui,
                        &power_label(event.on_battery, event.battery_pct),
                        design::ChipTone::Muted,
                    );
                });
            });
        });
    ui.add_space(6.0);
}

fn build_summary(events: &std::collections::VecDeque<PowerEvent>) -> HistorySummary {
    let last_switch = events
        .front().map_or_else(|| "-".to_string(), |event| event.ts.format("%H:%M:%S").to_string());
    let high_performance_trigger = events
        .iter()
        .find(|event| matches!(plan_kind(&event.plan_name), EventPlanKind::Performance)).map_or_else(|| "-".to_string(), |event| short_trigger(&event.trigger));

    HistorySummary {
        event_count: events.len(),
        last_switch,
        high_performance_trigger,
    }
}

fn plan_kind(plan_name: &str) -> EventPlanKind {
    let lower = plan_name.to_lowercase();
    if lower.contains("performance") || lower.contains("ultra") || lower.contains("high") {
        EventPlanKind::Performance
    } else if lower.contains("saver") || lower.contains("low") {
        EventPlanKind::LowPower
    } else if lower.contains("balanced") || lower.contains("standard") {
        EventPlanKind::Standard
    } else {
        EventPlanKind::Other
    }
}


fn trigger_label(trigger: &str) -> String {
    match trigger.trim().to_lowercase().as_str() {
        "" => "Unknown trigger".to_string(),
        "input resumed" => "Input resumed".to_string(),
        "cpu above threshold" => "CPU rose above threshold".to_string(),
        "entered low power" => "Idle and CPU quiet".to_string(),
        "startup" => "Startup".to_string(),
        "hold expired" => "Hold timer expired".to_string(),
        "manual" => "Manual override".to_string(),
        _ if trigger.trim().to_lowercase().ends_with(".exe") => {
            format!("Triggered by {}", trigger.trim())
        }
        _ => capitalize_first(trigger.trim()),
    }
}

fn short_trigger(trigger: &str) -> String {
    let label = trigger_label(trigger);
    label
        .strip_prefix("Triggered by ")
        .unwrap_or(label.as_str())
        .to_string()
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

fn date_group_label(ts: DateTime<Local>, now: DateTime<Local>) -> String {
    let date = ts.date_naive();
    let today = now.date_naive();
    if date == today {
        "Today".to_string()
    } else if date == today - Duration::days(1) {
        "Yesterday".to_string()
    } else {
        date.format("%Y-%m-%d").to_string()
    }
}

fn power_label(on_battery: bool, battery_pct: Option<u8>) -> String {
    if on_battery {
        battery_pct.map_or_else(|| "Battery".to_string(), |p| format!("Battery {p}%"))
    } else {
        "AC".to_string()
    }
}

fn export_to_desktop() {
    if let Ok(conn) = crate::db::open() {
        if let Ok(csv) = crate::db::export_csv(&conn) {
            let path = dirs::desktop_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("powerplanner_history.csv");
            if std::fs::write(&path, csv).is_ok() {
                let _ = spawn_no_window("explorer", &[path.to_string_lossy().as_ref()]);
            }
        }
    }
}

fn open_log() {
    let log_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("PowerPlanner")
        .join("powerplanner.log");
    let _ = spawn_no_window("notepad", &[log_path.to_string_lossy().as_ref()]);
}

fn spawn_no_window(prog: &str, args: &[&str]) -> std::io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new(prog)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(prog).args(args).spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_label_turns_raw_reasons_into_user_copy() {
        assert_eq!(trigger_label("rustc.exe"), "Triggered by rustc.exe");
        assert_eq!(trigger_label("input resumed"), "Input resumed");
        assert_eq!(
            trigger_label("cpu above threshold"),
            "CPU rose above threshold"
        );
        assert_eq!(trigger_label("entered low power"), "Idle and CPU quiet");
        assert_eq!(trigger_label("startup"), "Startup");
    }

    #[test]
    fn plan_kind_detects_common_power_plan_names() {
        assert!(matches!(
            plan_kind("Ultra performance"),
            EventPlanKind::Performance
        ));
        assert!(matches!(plan_kind("Power saver"), EventPlanKind::LowPower));
        assert!(matches!(plan_kind("Balanced"), EventPlanKind::Standard));
    }

    #[test]
    fn date_group_label_uses_relative_names_for_recent_days() {
        let now = Local::now();
        assert_eq!(date_group_label(now, now), "Today");
        assert_eq!(date_group_label(now - Duration::days(1), now), "Yesterday");
    }

    #[test]
    fn power_label_includes_battery_percent_when_available() {
        assert_eq!(power_label(true, Some(63)), "Battery 63%");
        assert_eq!(power_label(true, None), "Battery");
        assert_eq!(power_label(false, Some(63)), "AC");
    }
}
