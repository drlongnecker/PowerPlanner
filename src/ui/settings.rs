use crate::config::Config;
use crate::types::{MonitorCommand, PowerPlan};
use egui::Ui;
use std::sync::mpsc;

#[cfg(test)]
mod tests {
    #[test]
    fn settings_copy_uses_standard_and_low_power_labels() {
        const SETTINGS_COPY: &str =
            "Standard Plan|Low Power Plan|Performance Plan|Idle Wait|Low Power CPU Threshold|CPU Quiet Window";
        assert!(SETTINGS_COPY.contains("Standard Plan"));
        assert!(SETTINGS_COPY.contains("Low Power Plan"));
        assert!(!SETTINGS_COPY.contains("Idle Plan"));
    }
}

pub fn render(
    ui: &mut Ui,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
    available_plans: &[PowerPlan],
) {
    ui.heading("Settings");
    ui.separator();

    let mut changed = false;

    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([20.0, 10.0])
        .min_col_width(200.0)
        .show(ui, |ui| {
            // ── Idle Plan ─────────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Standard Plan");
                ui.small("Used during normal day-to-day usage.");
            });
            let current = available_plans
                .iter()
                .find(|p| p.guid == config.general.standard_plan_guid)
                .map(|p| p.name.as_str())
                .unwrap_or("Select a plan");
            egui::ComboBox::from_id_source("standard_plan_combo")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for plan in available_plans {
                        let sel = config.general.standard_plan_guid == plan.guid;
                        if ui.selectable_label(sel, &plan.name).clicked() {
                            config.general.standard_plan_guid = plan.guid.clone();
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            // ── Low Power Plan ────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Low Power Plan");
                ui.small("Used when the machine is idle and CPU activity stays low.");
            });
            let current = available_plans
                .iter()
                .find(|p| p.guid == config.general.low_power_plan_guid)
                .map(|p| p.name.as_str())
                .unwrap_or("Select a plan");
            egui::ComboBox::from_id_source("low_power_plan_combo")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for plan in available_plans {
                        let sel = config.general.low_power_plan_guid == plan.guid;
                        if ui.selectable_label(sel, &plan.name).clicked() {
                            config.general.low_power_plan_guid = plan.guid.clone();
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            // ── Performance Plan ──────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Performance Plan");
                ui.small("Used when a watched app is running.");
            });
            let current = available_plans
                .iter()
                .find(|p| p.guid == config.general.performance_plan_guid)
                .map(|p| p.name.as_str())
                .unwrap_or("Select a plan");
            egui::ComboBox::from_id_source("perf_plan_combo")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for plan in available_plans {
                        let sel = config.general.performance_plan_guid == plan.guid;
                        if ui.selectable_label(sel, &plan.name).clicked() {
                            config.general.performance_plan_guid = plan.guid.clone();
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            // ── Idle Wait ─────────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Idle Wait");
                ui.small("How long the user must be inactive before low power is allowed.");
            });
            let mut idle_wait = config.general.idle_wait_seconds as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut idle_wait)
                        .range(1..=14_400)
                        .suffix(" s"),
                )
                .changed()
            {
                config.general.idle_wait_seconds = idle_wait as u64;
                changed = true;
            }
            ui.end_row();

            // ── Low Power CPU Threshold ──────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Low Power CPU Threshold");
                ui.small("Maximum average CPU usage allowed before low power is blocked.");
            });
            let mut cpu_threshold = config.general.low_power_cpu_threshold_percent as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut cpu_threshold)
                        .range(1..=100)
                        .suffix(" %"),
                )
                .changed()
            {
                config.general.low_power_cpu_threshold_percent = cpu_threshold as u8;
                changed = true;
            }
            ui.end_row();

            // ── CPU Quiet Window ─────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("CPU Quiet Window");
                ui.small("How long CPU must stay quiet before low power is allowed.");
            });
            let mut quiet_window = config.general.low_power_cpu_quiet_window_seconds as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut quiet_window)
                        .range(5..=600)
                        .suffix(" s"),
                )
                .changed()
            {
                config.general.low_power_cpu_quiet_window_seconds = quiet_window as u64;
                changed = true;
            }
            ui.end_row();

            // ── Hold Timer ────────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Hold Timer");
                ui.small("Stay in Performance mode after the last watched app closes.");
            });
            let mut hold = config.general.hold_performance_seconds as i32;
            if ui
                .add(egui::DragValue::new(&mut hold).range(0..=300).suffix(" s"))
                .changed()
            {
                config.general.hold_performance_seconds = hold as u64;
                changed = true;
            }
            ui.end_row();

            // ── Poll Interval ─────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Poll Interval");
                ui.small("How often to scan running processes.");
            });
            let mut poll = config.general.poll_interval_ms as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut poll)
                        .range(100..=5000)
                        .suffix(" ms"),
                )
                .changed()
            {
                config.general.poll_interval_ms = poll as u64;
                changed = true;
            }
            ui.end_row();

            // ── Laptop Mode ───────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Laptop Mode");
                ui.small("When unchecked, High Performance is suppressed while on battery.");
            });
            if ui
                .checkbox(
                    &mut config.general.promote_on_battery,
                    "Allow High Performance on battery",
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            // ── Autostart ─────────────────────────────────────────────────
            let elevated = config.autostart.is_elevated;
            ui.vertical(|ui| {
                ui.strong("Autostart");
                ui.small("Launch PowerPlanner automatically at login via Task Scheduler.");
                if !elevated {
                    ui.add(egui::Label::new(
                        egui::RichText::new("Not running as admin — UAC will prompt.")
                            .small()
                            .color(egui::Color32::from_rgb(210, 170, 60)),
                    ));
                }
            });
            if config.autostart.registered {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Registered")
                            .color(egui::Color32::from_rgb(100, 210, 100)),
                    );
                    let remove_label = if elevated { "Remove" } else { "Remove (UAC)" };
                    if ui.button(remove_label).clicked() {
                        match crate::scheduler::unregister() {
                            Ok(()) => {
                                config.autostart.registered = false;
                                changed = true;
                            }
                            Err(e) => log::error!("Unregister failed: {}", e),
                        }
                    }
                });
            } else {
                let register_label = if elevated {
                    "Register at login"
                } else {
                    "Register at login (UAC)"
                };
                if ui.button(register_label).clicked() {
                    match crate::scheduler::register() {
                        Ok(()) => {
                            config.autostart.registered = true;
                            changed = true;
                        }
                        Err(e) => log::error!("Register failed: {}", e),
                    }
                }
            }
            ui.end_row();
        });

    if changed {
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}
