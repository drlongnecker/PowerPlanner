use egui::Ui;
use std::sync::mpsc;
use crate::config::Config;
use crate::types::{MonitorCommand, PowerPlan};

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
                ui.strong("Idle Plan");
                ui.small("Used when no watched apps are running.");
            });
            let current = available_plans.iter()
                .find(|p| p.guid == config.general.idle_plan_guid)
                .map(|p| p.name.as_str())
                .unwrap_or("Select a plan");
            egui::ComboBox::from_id_source("idle_plan_combo")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for plan in available_plans {
                        let sel = config.general.idle_plan_guid == plan.guid;
                        if ui.selectable_label(sel, &plan.name).clicked() {
                            config.general.idle_plan_guid = plan.guid.clone();
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
            let current = available_plans.iter()
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

            // ── Hold Timer ────────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Hold Timer");
                ui.small("Stay in Performance mode after the last watched app closes.");
            });
            let mut hold = config.general.hold_performance_seconds as i32;
            if ui.add(egui::DragValue::new(&mut hold).range(0..=300).suffix(" s")).changed() {
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
            if ui.add(egui::DragValue::new(&mut poll).range(100..=5000).suffix(" ms")).changed() {
                config.general.poll_interval_ms = poll as u64;
                changed = true;
            }
            ui.end_row();

            // ── Laptop Mode ───────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.strong("Laptop Mode");
                ui.small("When unchecked, High Performance is suppressed while on battery.");
            });
            if ui.checkbox(
                &mut config.general.promote_on_battery,
                "Allow High Performance on battery",
            ).changed() {
                changed = true;
            }
            ui.end_row();

            // ── Autostart ─────────────────────────────────────────────────
            let elevated = crate::scheduler::is_elevated();
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
                            Ok(()) => { config.autostart.registered = false; changed = true; }
                            Err(e) => log::error!("Unregister failed: {}", e),
                        }
                    }
                });
            } else {
                let register_label = if elevated { "Register at login" } else { "Register at login (UAC)" };
                if ui.button(register_label).clicked() {
                    match crate::scheduler::register() {
                        Ok(()) => { config.autostart.registered = true; changed = true; }
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
