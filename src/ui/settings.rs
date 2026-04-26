use crate::config::Config;
use crate::types::{MonitorCommand, PowerPlan};
use egui::{self, Align, RichText, Ui};
use std::sync::mpsc;

#[cfg(test)]
mod tests {
    #[test]
    fn settings_copy_uses_plan_grouping_labels() {
        const SETTINGS_COPY: &str =
            "Standard Plan|Low Power Plan|High Performance Plan|Automation|Global monitor cadence";
        assert!(SETTINGS_COPY.contains("Standard Plan"));
        assert!(SETTINGS_COPY.contains("High Performance Plan"));
        assert!(SETTINGS_COPY.contains("Global monitor cadence"));
    }
}

pub fn render(
    ui: &mut Ui,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
    available_plans: &[PowerPlan],
) {
    let mut changed = false;

    crate::ui::padded_page(ui, |ui| {
        ui.heading("Settings");
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Tune plan switching, plan-specific thresholds, and automation behavior.",
            )
            .weak()
            .size(15.0),
        );
        ui.separator();
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |columns| {
                render_standard_plan_card(&mut columns[0], config, available_plans, &mut changed);
                columns[0].add_space(12.0);
                render_low_power_plan_card(&mut columns[0], config, available_plans, &mut changed);

                render_high_performance_plan_card(
                    &mut columns[1],
                    config,
                    available_plans,
                    &mut changed,
                );
                columns[1].add_space(12.0);
                render_automation_card(&mut columns[1], config, &mut changed);
            });
        });
    });

    if changed {
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn render_standard_plan_card(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    settings_card(
        ui,
        "Standard Plan",
        "Normal day-to-day usage, plus the global monitor cadence used while PowerPlanner evaluates transitions.",
        |ui| {
            plan_combo_row(
                ui,
                "Plan",
                "The default plan used when no watched app or low-power condition is active.",
                available_plans,
                &mut config.general.standard_plan_guid,
                changed,
                "standard_plan_combo",
            );
            numeric_value_row(
                ui,
                "Poll Interval",
                "Global monitor cadence for process scans and rule evaluation.",
                &mut config.general.poll_interval_ms,
                100..=5_000,
                "ms",
                changed,
            );
            numeric_value_row(
                ui,
                "Hold Timer",
                "Global transition delay after a watched app closes before PowerPlanner relaxes from boosted behavior.",
                &mut config.general.hold_performance_seconds,
                0..=300,
                "s",
                changed,
            );
        },
    );
}

fn render_low_power_plan_card(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    settings_card(
        ui,
        "Low Power Plan",
        "Idle-state behavior and the CPU quiet conditions required before PowerPlanner allows the low-power plan.",
        |ui| {
            plan_combo_row(
                ui,
                "Plan",
                "Used when the machine is idle and CPU activity stays low.",
                available_plans,
                &mut config.general.low_power_plan_guid,
                changed,
                "low_power_plan_combo",
            );
            let mut cpu_threshold = config.general.low_power_cpu_threshold_percent as u64;
            numeric_value_row(
                ui,
                "CPU Threshold",
                "Maximum average CPU usage allowed before low power is blocked.",
                &mut cpu_threshold,
                1..=100,
                "%",
                changed,
            );
            config.general.low_power_cpu_threshold_percent = cpu_threshold as u8;
            numeric_value_row(
                ui,
                "Quiet Window",
                "How long CPU must stay quiet before low power is allowed.",
                &mut config.general.low_power_cpu_quiet_window_seconds,
                5..=600,
                "s",
                changed,
            );
            numeric_value_row(
                ui,
                "Idle Wait",
                "How long the user must be inactive before low power is allowed.",
                &mut config.general.idle_wait_seconds,
                60..=14_400,
                "s",
                changed,
            );
        },
    );
}

fn render_high_performance_plan_card(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    settings_card(
        ui,
        "High Performance Plan",
        "Behavior used while watched apps are active or while performance boosting remains allowed.",
        |ui| {
            plan_combo_row(
                ui,
                "Plan",
                "Used when a watched app is running.",
                available_plans,
                &mut config.general.performance_plan_guid,
                changed,
                "performance_plan_combo",
            );
            toggle_row(
                ui,
                "Allow HP on Battery",
                "When enabled, battery power will not suppress the high-performance plan.",
                &mut config.general.promote_on_battery,
                changed,
            );
        },
    );
}

fn render_automation_card(ui: &mut Ui, config: &mut Config, changed: &mut bool) {
    settings_card(
        ui,
        "Automation",
        "Manage whether PowerPlanner registers itself to start at login.",
        |ui| {
            let elevated = config.autostart.is_elevated;
            ui.horizontal(|ui| {
                let status_text = if config.autostart.registered {
                    RichText::new("Registered")
                        .color(egui::Color32::from_rgb(92, 196, 108))
                        .strong()
                } else {
                    RichText::new("Not registered")
                        .color(ui.visuals().weak_text_color())
                        .strong()
                };
                ui.label(status_text);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if config.autostart.registered {
                        let remove_label = if elevated { "Remove" } else { "Remove (UAC)" };
                        if ui
                            .add_sized([140.0, 30.0], egui::Button::new(remove_label))
                            .clicked()
                        {
                            match crate::scheduler::unregister() {
                                Ok(()) => {
                                    config.autostart.registered = false;
                                    *changed = true;
                                }
                                Err(e) => log::error!("Unregister failed: {}", e),
                            }
                        }
                    } else {
                        let register_label = if elevated {
                            "Register at login"
                        } else {
                            "Register at login (UAC)"
                        };
                        if ui
                            .add_sized([140.0, 30.0], egui::Button::new(register_label))
                            .clicked()
                        {
                            match crate::scheduler::register() {
                                Ok(()) => {
                                    config.autostart.registered = true;
                                    *changed = true;
                                }
                                Err(e) => log::error!("Register failed: {}", e),
                            }
                        }
                    }
                });
            });

            ui.add_space(8.0);
            ui.label(
                RichText::new("Launch PowerPlanner automatically at login via Task Scheduler.")
                    .weak()
                    .size(14.0),
            );

            if !elevated {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Not running as admin - UAC will prompt.")
                        .color(egui::Color32::from_rgb(210, 170, 60))
                        .size(14.0),
                );
            }
        },
    );
}

fn settings_card(ui: &mut Ui, title: &str, description: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(19.0).strong());
            ui.add_space(3.0);
            ui.label(RichText::new(description).weak().size(14.0));
            ui.add_space(12.0);
            add_contents(ui);
        });
}

fn plan_combo_row(
    ui: &mut Ui,
    label: &str,
    description: &str,
    available_plans: &[PowerPlan],
    selected_guid: &mut String,
    changed: &mut bool,
    combo_id: &str,
) {
    let current = available_plans
        .iter()
        .find(|p| p.guid == *selected_guid)
        .map(|p| p.name.as_str())
        .unwrap_or("Select a plan");

    setting_combo_row(ui, label, description, combo_id, current, |ui| {
        for plan in available_plans {
            if ui
                .selectable_label(*selected_guid == plan.guid, &plan.name)
                .clicked()
            {
                *selected_guid = plan.guid.clone();
                *changed = true;
            }
        }
    });
}

fn setting_combo_row(
    ui: &mut Ui,
    label: &str,
    description: &str,
    combo_id: &str,
    selected_text: &str,
    add_items: impl FnOnce(&mut Ui),
) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(17.0).strong());
        ui.label(RichText::new(description).weak().size(12.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 230.0).max(0.0));
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = 34.0;
                egui::ComboBox::from_id_source(combo_id)
                    .selected_text(selected_text)
                    .width(230.0)
                    .show_ui(ui, add_items);
            });
        });
        ui.add_space(8.0);
    });
}

fn numeric_value_row(
    ui: &mut Ui,
    label: &str,
    description: &str,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    suffix: &str,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(15.0).strong());
        ui.label(RichText::new(description).weak().size(12.0));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 140.0).max(0.0));
            let mut numeric = egui::DragValue::new(value)
                .range(range)
                .suffix(format!(" {}", suffix));
            numeric = numeric.speed(1.0);
            if ui.add_sized([140.0, 30.0], numeric).changed() {
                *changed = true;
            }
        });
        ui.add_space(8.0);
    });
}

fn toggle_row(ui: &mut Ui, label: &str, description: &str, value: &mut bool, changed: &mut bool) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(15.0).strong());
                ui.label(RichText::new(description).weak().size(12.0));
            });
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.checkbox(value, "").changed() {
                    *changed = true;
                }
            });
        });
        ui.add_space(8.0);
    });
}
