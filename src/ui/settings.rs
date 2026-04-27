use crate::config::Config;
use crate::types::{MonitorCommand, PowerPlan};
use crate::ui::design;
use egui::{self, Align, RichText, Ui};
use std::sync::mpsc;

const SETTINGS_COMBO_WIDTH: f32 = 230.0;
const SETTINGS_VALUE_WIDTH: f32 = 132.0;

pub fn render(
    ui: &mut Ui,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
    available_plans: &[PowerPlan],
) {
    let mut changed = false;

    crate::ui::padded_page(ui, |ui| {
        design::page_header(
            ui,
            "Settings",
            "Tune startup behavior and the Standard > High Performance > Low Power plan flow.",
        );

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_automation_section(ui, config, &mut changed);
                ui.add_space(design::spacing::SECTION_GAP);
                render_standard_section(ui, config, available_plans, &mut changed);
                ui.add_space(design::spacing::SECTION_GAP);
                render_high_performance_section(ui, config, available_plans, &mut changed);
                ui.add_space(design::spacing::SECTION_GAP);
                render_low_power_section(ui, config, available_plans, &mut changed);
            });
    });

    if changed {
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn render_automation_section(ui: &mut Ui, config: &mut Config, changed: &mut bool) {
    let elevated = config.autostart.is_elevated;
    let registered = config.autostart.registered;
    design::section_with_header_action(
        ui,
        "Startup Automation",
        "One-time setup for launching PowerPlanner automatically at login.",
        |ui| {
            let kind = if registered {
                design::StatusKind::Success
            } else if elevated {
                design::StatusKind::Muted
            } else {
                design::StatusKind::Warning
            };
            design::status_badge(ui, design::registered_status_text(registered), kind);
        },
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Task Scheduler registration")
                        .size(design::type_size::LABEL)
                        .strong(),
                );
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
                            .add_sized([160.0, 30.0], egui::Button::new(register_label))
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

            ui.add_space(4.0);
            ui.label(
                RichText::new("Launches through Windows Task Scheduler.")
                    .weak()
                    .size(design::type_size::HELP),
            );

            if !elevated {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Not running as admin. Windows will show a UAC prompt.")
                        .color(design::color::WARNING)
                        .size(design::type_size::HELP),
                );
            }
        },
    );
}

fn render_standard_section(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    design::section(
        ui,
        "1. Standard",
        "Default behavior. These settings apply before specialized plan rules.",
        |ui| {
            settings_grid(ui, |ui| {
                plan_combo_cell(
                    ui,
                    "Plan",
                    "Used when no watched app or low-power condition is active.",
                    available_plans,
                    &mut config.general.standard_plan_guid,
                    changed,
                    "standard_plan_combo",
                );
                numeric_value_cell(
                    ui,
                    "Poll Interval",
                    "Global monitor cadence for scans and rule evaluation.",
                    &mut config.general.poll_interval_ms,
                    100..=5_000,
                    "ms",
                    changed,
                );
                ui.end_row();

                numeric_value_cell(
                    ui,
                    "Hold Timer",
                    "Delay after a watched app closes before relaxing from boosted behavior.",
                    &mut config.general.hold_performance_seconds,
                    0..=300,
                    "s",
                    changed,
                );
                empty_cell(ui);
                ui.end_row();
            });
        },
    );
}

fn render_high_performance_section(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    design::section(
        ui,
        "2. High Performance",
        "Behavior used while watched apps are active or while performance boosting remains allowed.",
        |ui| {
            settings_grid(ui, |ui| {
                plan_combo_cell(
                    ui,
                    "Plan",
                    "Used when a watched app is running.",
                    available_plans,
                    &mut config.general.performance_plan_guid,
                    changed,
                    "performance_plan_combo",
                );
                toggle_cell(
                    ui,
                    "Allow HP on Battery",
                    "Battery power will not suppress the high-performance plan.",
                    &mut config.general.promote_on_battery,
                    changed,
                );
                ui.end_row();
            });
        },
    );
}

fn render_low_power_section(
    ui: &mut Ui,
    config: &mut Config,
    available_plans: &[PowerPlan],
    changed: &mut bool,
) {
    design::section(
        ui,
        "3. Low Power",
        "Idle-state behavior and CPU quiet conditions required before low power is allowed.",
        |ui| {
            settings_grid(ui, |ui| {
                plan_combo_cell(
                    ui,
                    "Plan",
                    "Used when the machine is idle and CPU activity stays low.",
                    available_plans,
                    &mut config.general.low_power_plan_guid,
                    changed,
                    "low_power_plan_combo",
                );
                let mut cpu_threshold = config.general.low_power_cpu_threshold_percent as u64;
                numeric_value_cell(
                    ui,
                    "CPU Threshold",
                    "Maximum average CPU usage allowed before low power is blocked.",
                    &mut cpu_threshold,
                    1..=100,
                    "%",
                    changed,
                );
                config.general.low_power_cpu_threshold_percent = cpu_threshold as u8;
                ui.end_row();

                numeric_value_cell(
                    ui,
                    "Quiet Window",
                    "How long CPU must stay quiet before low power is allowed.",
                    &mut config.general.low_power_cpu_quiet_window_seconds,
                    5..=600,
                    "s",
                    changed,
                );
                numeric_value_cell(
                    ui,
                    "Idle Wait",
                    "How long the user must be inactive before low power is allowed.",
                    &mut config.general.idle_wait_seconds,
                    60..=14_400,
                    "s",
                    changed,
                );
                ui.end_row();
            });
        },
    );
}

fn settings_grid(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let column_width = ((ui.available_width() - 18.0) / 2.0).max(120.0);
    egui::Grid::new(ui.next_auto_id())
        .num_columns(2)
        .spacing([18.0, design::spacing::ROW_GAP])
        .min_col_width(column_width)
        .show(ui, add_contents);
}

fn plan_combo_cell(
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

    ui.vertical(|ui| {
        design::setting_label(ui, label, description);
        ui.add_space(6.0);
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 32.0;
            let control_width = ui.available_width().min(SETTINGS_COMBO_WIDTH);
            egui::ComboBox::from_id_source(combo_id)
                .selected_text(current)
                .width(control_width)
                .show_ui(ui, |ui| {
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
        });
    });
}

fn numeric_value_cell(
    ui: &mut Ui,
    label: &str,
    description: &str,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    suffix: &str,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        design::setting_label(ui, label, description);
        ui.add_space(6.0);
        let numeric = egui::DragValue::new(value)
            .range(range)
            .suffix(format!(" {}", suffix))
            .speed(1.0);
        let control_width = ui.available_width().min(SETTINGS_VALUE_WIDTH);
        if ui.add_sized([control_width, 30.0], numeric).changed() {
            *changed = true;
        }
    });
}

fn toggle_cell(ui: &mut Ui, label: &str, description: &str, value: &mut bool, changed: &mut bool) {
    ui.vertical(|ui| {
        design::setting_label(ui, label, description);
        ui.add_space(6.0);
        if design::enabled_badge_button(ui, *value).clicked() {
            *value = !*value;
            *changed = true;
        }
    });
}

fn empty_cell(ui: &mut Ui) {
    ui.allocate_space(egui::vec2(1.0, 1.0));
}
