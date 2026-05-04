use crate::config::Config;
use crate::types::{
    AppState, MonitorCommand, PlanDiagnostics, PlanProcessorRecommendation, PlanProcessorSettings,
    PowerPlan,
};
use crate::ui::design;
use egui::{self, Align, RichText, Ui};
use std::sync::mpsc;

const SETTINGS_COMBO_WIDTH: f32 = 230.0;
const SETTINGS_VALUE_WIDTH: f32 = 132.0;
const ENERGY_RATE_LINKS: [(&str, &str); 3] = [
    (
        "EnergySage local electricity costs",
        "https://www.energysage.com/local-data/electricity-cost/",
    ),
    (
        "EIA electricity data",
        "https://www.eia.gov/electricity/data.php",
    ),
    (
        "OpenEI Utility Rate Database",
        "https://openei.org/services/doc/rest/util_rates/?version=8",
    ),
];
const CPU_PROFILE_LINKS: [(&str, &str); 3] = [
    (
        "Intel processor specifications",
        "https://www.intel.com/content/www/us/en/products/details/processors.html",
    ),
    (
        "AMD processor specifications",
        "https://www.amd.com/en/products/specifications/processors",
    ),
    (
        "TechPowerUp CPU database",
        "https://www.techpowerup.com/cpu-specs/",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Automation,
    Standard,
    Performance,
    LowPower,
    Energy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_tabs_include_energy_estimates() {
        let labels = settings_tab_labels();

        assert_eq!(labels.len(), 5);
        assert!(labels
            .iter()
            .any(|(tab, label)| *tab == SettingsTab::Energy && *label == "Energy"));
    }

    #[test]
    fn energy_settings_exposes_reference_links() {
        assert_eq!(ENERGY_RATE_LINKS.len(), 3);
        assert_eq!(CPU_PROFILE_LINKS.len(), 3);
        assert!(ENERGY_RATE_LINKS
            .iter()
            .any(|(label, _)| label.contains("EIA")));
        assert!(CPU_PROFILE_LINKS
            .iter()
            .any(|(label, _)| label.contains("Intel")));
    }
}

pub fn render(
    ui: &mut Ui,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
    state: &AppState,
) {
    let mut changed = false;

    crate::ui::padded_page(ui, |ui| {
        design::page_header(
            ui,
            "Settings",
            "Tune startup behavior and the Standard > High Performance > Low Power plan flow.",
        );

        let tab_id = ui.make_persistent_id("settings_tab");
        let mut selected_tab = ui
            .data_mut(|data| data.get_persisted::<SettingsTab>(tab_id))
            .unwrap_or(SettingsTab::Standard);

        design::tabs(ui, &mut selected_tab, &settings_tab_labels());
        ui.data_mut(|data| data.insert_persisted(tab_id, selected_tab));
        ui.add_space(design::spacing::SECTION_GAP);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match selected_tab {
                SettingsTab::Automation => render_automation_section(ui, config, &mut changed),
                SettingsTab::Standard => {
                    let selected_guid = render_plan_tab(
                        ui,
                        state,
                        PlanTab {
                            title: "Standard",
                            description: "Default behavior when no watched app, turbo rescue, or low-power condition is active.",
                            plan_label: "Plan",
                            plan_description: "Used when specialized plan rules are inactive.",
                            combo_id: "standard_plan_combo",
                            guid: config.general.standard_plan_guid.clone(),
                            recommendation: config.general.standard_recommendation(),
                        },
                        &mut changed,
                        |ui, changed| {
                            settings_grid(ui, |ui| {
                                numeric_value_cell(
                                    ui,
                                    "Poll Interval",
                                    "Global monitor cadence for scans and rule evaluation.",
                                    &mut config.general.poll_interval_ms,
                                    100..=5_000,
                                    "ms",
                                    changed,
                                );
                                numeric_value_cell(
                                    ui,
                                    "Hold Timer",
                                    "Delay after a boost trigger clears before relaxing from High Performance.",
                                    &mut config.general.hold_performance_seconds,
                                    0..=300,
                                    "s",
                                    changed,
                                );
                                ui.end_row();
                            });
                        },
                        |ui, guid, settings, recommendation, changed| {
                            processor_limit_controls(
                                ui,
                                tx,
                                guid,
                                settings,
                                recommendation,
                                "Use 99% max to prevent Windows 11 turbo while balanced.",
                                &mut config.general.standard_cpu_min_percent,
                                &mut config.general.standard_cpu_max_percent,
                                changed,
                            );
                        },
                    );
                    if selected_guid != config.general.standard_plan_guid {
                        config.general.standard_plan_guid = selected_guid;
                        changed = true;
                    }
                }
                SettingsTab::Performance => {
                    let mut promote_on_battery = config.general.promote_on_battery;
                    let mut turbo_rescue_enabled = config.general.turbo_rescue_enabled;
                    let mut turbo_rescue_cpu_threshold_percent =
                        config.general.turbo_rescue_cpu_threshold_percent;
                    let mut turbo_rescue_window_seconds =
                        config.general.turbo_rescue_window_seconds;
                    let mut performance_cpu_min_percent =
                        config.general.performance_cpu_min_percent;
                    let mut performance_cpu_max_percent =
                        config.general.performance_cpu_max_percent;
                    let selected_guid = render_plan_tab(
                        ui,
                        state,
                        PlanTab {
                            title: "High Performance",
                            description: "Boosted behavior used for watched apps and sustained turbo rescue.",
                            plan_label: "Plan",
                            plan_description: "Used when a watched app or turbo rescue is active.",
                            combo_id: "performance_plan_combo",
                            guid: config.general.performance_plan_guid.clone(),
                            recommendation: config.general.performance_recommendation(),
                        },
                        &mut changed,
                        |ui, changed| {
                            settings_grid(ui, |ui| {
                                toggle_cell(
                                    ui,
                                    "Allow HP on Battery",
                                    "Battery power will not suppress the high-performance plan.",
                                    &mut promote_on_battery,
                                    changed,
                                );
                                empty_cell(ui);
                                ui.end_row();
                            });
                            ui.add_space(design::spacing::SECTION_GAP);
                            turbo_rescue_controls(
                                ui,
                                &mut turbo_rescue_enabled,
                                &mut turbo_rescue_cpu_threshold_percent,
                                &mut turbo_rescue_window_seconds,
                                changed,
                            );
                        },
                        |ui, guid, settings, recommendation, changed| {
                            processor_limit_controls(
                                ui,
                                tx,
                                guid,
                                settings,
                                recommendation,
                                "Keep both limits at 100% so High Performance releases the governor.",
                                &mut performance_cpu_min_percent,
                                &mut performance_cpu_max_percent,
                                changed,
                            );
                        },
                    );
                    if selected_guid != config.general.performance_plan_guid {
                        config.general.performance_plan_guid = selected_guid;
                        changed = true;
                    }
                    if promote_on_battery != config.general.promote_on_battery {
                        config.general.promote_on_battery = promote_on_battery;
                        changed = true;
                    }
                    if performance_cpu_min_percent != config.general.performance_cpu_min_percent {
                        config.general.performance_cpu_min_percent = performance_cpu_min_percent;
                        changed = true;
                    }
                    if performance_cpu_max_percent != config.general.performance_cpu_max_percent {
                        config.general.performance_cpu_max_percent = performance_cpu_max_percent;
                        changed = true;
                    }
                    if turbo_rescue_enabled != config.general.turbo_rescue_enabled {
                        config.general.turbo_rescue_enabled = turbo_rescue_enabled;
                        changed = true;
                    }
                    if turbo_rescue_cpu_threshold_percent
                        != config.general.turbo_rescue_cpu_threshold_percent
                    {
                        config.general.turbo_rescue_cpu_threshold_percent =
                            turbo_rescue_cpu_threshold_percent;
                        changed = true;
                    }
                    if turbo_rescue_window_seconds != config.general.turbo_rescue_window_seconds {
                        config.general.turbo_rescue_window_seconds = turbo_rescue_window_seconds;
                        changed = true;
                    }
                }
                SettingsTab::LowPower => {
                    let selected_guid = render_plan_tab(
                        ui,
                        state,
                        PlanTab {
                            title: "Low Power",
                            description: "Idle-state behavior and CPU average conditions required before low power is allowed.",
                            plan_label: "Plan",
                            plan_description: "Used when the machine is idle and CPU activity stays low.",
                            combo_id: "low_power_plan_combo",
                            guid: config.general.low_power_plan_guid.clone(),
                            recommendation: config.general.low_power_recommendation(),
                        },
                        &mut changed,
                        |ui, changed| {
                            settings_grid(ui, |ui| {
                                let mut cpu_threshold =
                                    config.general.cpu_average_threshold_percent as u64;
                                numeric_value_cell(
                                    ui,
                                    "CPU Avg Threshold",
                                    "Average CPU usage gate used by low power and turbo rescue.",
                                    &mut cpu_threshold,
                                    1..=100,
                                    "%",
                                    changed,
                                );
                                config.general.cpu_average_threshold_percent = cpu_threshold as u8;
                                numeric_value_cell(
                                    ui,
                                    "CPU Avg Window",
                                    "Rolling window used for shared CPU average decisions.",
                                    &mut config.general.cpu_average_window_seconds,
                                    5..=600,
                                    "s",
                                    changed,
                                );
                                ui.end_row();

                                numeric_value_cell(
                                    ui,
                                    "Idle Wait",
                                    "How long the user must be inactive before low power is allowed.",
                                    &mut config.general.idle_wait_seconds,
                                    60..=14_400,
                                    "s",
                                    changed,
                                );
                                empty_cell(ui);
                                ui.end_row();
                            });
                        },
                        |ui, guid, settings, recommendation, changed| {
                            processor_limit_controls(
                                ui,
                                tx,
                                guid,
                                settings,
                                recommendation,
                                "Cap max at 20% so idle low-power behavior stays visibly constrained.",
                                &mut config.general.low_power_cpu_min_percent,
                                &mut config.general.low_power_cpu_max_percent,
                                changed,
                            );
                        },
                    );
                    if selected_guid != config.general.low_power_plan_guid {
                        config.general.low_power_plan_guid = selected_guid;
                        changed = true;
                    }
                }
                SettingsTab::Energy => render_energy_section(ui, config, &mut changed),
            });
    });

    if changed {
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
    }
}

fn settings_tab_labels() -> [(SettingsTab, &'static str); 5] {
    [
        (SettingsTab::Automation, "Automation"),
        (SettingsTab::Standard, "Standard"),
        (SettingsTab::Performance, "High Performance"),
        (SettingsTab::LowPower, "Low Power"),
        (SettingsTab::Energy, "Energy"),
    ]
}

struct PlanTab {
    title: &'static str,
    description: &'static str,
    plan_label: &'static str,
    plan_description: &'static str,
    combo_id: &'static str,
    guid: String,
    recommendation: PlanProcessorRecommendation,
}

fn render_plan_tab(
    ui: &mut Ui,
    state: &AppState,
    mut plan: PlanTab,
    changed: &mut bool,
    add_behavior: impl FnOnce(&mut Ui, &mut bool),
    add_processor_recommendation: impl FnOnce(
        &mut Ui,
        &str,
        Option<PlanProcessorSettings>,
        PlanProcessorRecommendation,
        &mut bool,
    ),
) -> String {
    design::section(ui, plan.title, plan.description, |ui| {
        settings_grid(ui, |ui| {
            plan_combo_cell(
                ui,
                plan.plan_label,
                plan.plan_description,
                &state.available_plans,
                &mut plan.guid,
                changed,
                plan.combo_id,
            );
            empty_cell(ui);
            ui.end_row();
        });

        ui.add_space(design::spacing::SECTION_GAP);
        add_behavior(ui, changed);
        ui.add_space(design::spacing::SECTION_GAP);
        design::subsection_heading(ui, "Processor Limits");
        ui.add_space(6.0);
        let settings = state
            .plan_processor_settings
            .get(&plan.guid)
            .and_then(|settings| *settings);
        add_processor_recommendation(ui, &plan.guid, settings, plan.recommendation, changed);
    });
    plan.guid
}

fn processor_limit_controls(
    ui: &mut Ui,
    tx: &mpsc::Sender<MonitorCommand>,
    guid: &str,
    settings: Option<PlanProcessorSettings>,
    recommendation: PlanProcessorRecommendation,
    description: &str,
    min_percent: &mut u8,
    max_percent: &mut u8,
    changed: &mut bool,
) {
    let diagnostic = PlanDiagnostics::for_settings(settings.as_ref(), recommendation);
    settings_grid(ui, |ui| {
        recommendation_controls(ui, description, min_percent, max_percent, changed);
        current_configuration(ui, tx, guid, settings, recommendation, diagnostic);
        ui.end_row();
    });
}

fn recommendation_controls(
    ui: &mut Ui,
    description: &str,
    min_percent: &mut u8,
    max_percent: &mut u8,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(description)
                .weak()
                .size(design::type_size::HELP),
        );
        ui.add_space(design::spacing::ROW_GAP);

        let mut min = *min_percent as u64;
        numeric_value_cell(
            ui,
            "Min",
            "Recommended minimum processor state for plugged-in and battery power.",
            &mut min,
            0..=100,
            "%",
            changed,
        );
        ui.add_space(design::spacing::ROW_GAP);

        let mut max = *max_percent as u64;
        numeric_value_cell(
            ui,
            "Max",
            "Recommended maximum processor state for plugged-in and battery power.",
            &mut max,
            0..=100,
            "%",
            changed,
        );

        *min_percent = min.min(max) as u8;
        *max_percent = max.max(min) as u8;
    });
}

fn current_configuration(
    ui: &mut Ui,
    tx: &mpsc::Sender<MonitorCommand>,
    guid: &str,
    settings: Option<PlanProcessorSettings>,
    recommendation: PlanProcessorRecommendation,
    diagnostic: PlanDiagnostics,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Current Configuration")
                    .size(design::type_size::LABEL)
                    .strong(),
            );
            let (label, kind) = match diagnostic {
                PlanDiagnostics::Configured => ("Configured", design::StatusKind::Success),
                PlanDiagnostics::NeedsReview => ("Not Configured", design::StatusKind::Warning),
                PlanDiagnostics::Unavailable => ("Unavailable", design::StatusKind::Muted),
            };
            design::compact_status_badge(ui, label, kind);
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new("Minimum and maximum processor state for plugged-in and battery power.")
                .weak()
                .size(design::type_size::HELP),
        );
        ui.add_space(6.0);
        if let Some(settings) = settings {
            egui::Grid::new(ui.next_auto_id())
                .num_columns(3)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(
                        RichText::new("Plugged in")
                            .weak()
                            .size(design::type_size::HELP),
                    );
                    ui.label(
                        RichText::new("On battery")
                            .weak()
                            .size(design::type_size::HELP),
                    );
                    ui.end_row();

                    ui.label("Min");
                    ui.label(format_limit(settings.min_percent.ac));
                    ui.label(format_limit(settings.min_percent.dc));
                    ui.end_row();

                    ui.label("Max");
                    ui.label(format_limit(settings.max_percent.ac));
                    ui.label(format_limit(settings.max_percent.dc));
                    ui.end_row();
                });
            if diagnostic == PlanDiagnostics::NeedsReview
                && ui
                    .button(format!(
                        "Apply recommended CPU limits ({}% min, {}% max)",
                        recommendation.min_percent, recommendation.max_percent
                    ))
                    .clicked()
            {
                let _ = tx.send(MonitorCommand::ApplyPlanProcessorRecommendation {
                    guid: guid.to_string(),
                    recommendation,
                });
            }
        } else {
            ui.label(
                RichText::new("Windows did not return processor limit settings for this plan.")
                    .weak()
                    .size(design::type_size::HELP),
            );
        }
    });
}

fn turbo_rescue_controls(
    ui: &mut Ui,
    enabled: &mut bool,
    threshold_percent: &mut u8,
    window_seconds: &mut u64,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        design::setting_label(
            ui,
            "Turbo Rescue",
            "Promotes to High Performance when CPU speed stays above base clock.",
        );
        ui.add_space(6.0);
        settings_grid(ui, |ui| {
            toggle_cell(
                ui,
                "Enabled",
                "Allow sustained above-base CPU speed to promote High Performance.",
                enabled,
                changed,
            );
            ui.vertical(|ui| {
                let mut threshold = *threshold_percent as u64;
                numeric_value_cell(
                    ui,
                    "CPU Avg Threshold",
                    "Minimum rolling CPU average needed before Turbo Rescue can trigger.",
                    &mut threshold,
                    1..=100,
                    "%",
                    changed,
                );
                *threshold_percent = threshold as u8;
                ui.add_space(design::spacing::ROW_GAP);
                numeric_value_cell(
                    ui,
                    "Sustain Window",
                    "How long CPU speed must remain above base before switching plans.",
                    window_seconds,
                    3..=120,
                    "s",
                    changed,
                );
            });
            ui.end_row();
        });
    });
}

fn format_limit(value: Option<u32>) -> String {
    value
        .map(|value| format!("{}%", value))
        .unwrap_or_else(|| "n/a".to_string())
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

fn render_energy_section(ui: &mut Ui, config: &mut Config, changed: &mut bool) {
    design::section(
        ui,
        "Energy Estimates",
        "Configure estimated CPU power and estimated cost calculations.",
        |ui| {
            render_energy_reference_links(ui);
            ui.add_space(design::spacing::SECTION_GAP);
            settings_grid(ui, |ui| {
                toggle_cell(
                    ui,
                    "Estimated CPU Power",
                    "Show modeled CPU watts, cost, and savings on the dashboard.",
                    &mut config.general.energy_estimates_enabled,
                    changed,
                );
                float_value_cell(
                    ui,
                    "Electricity Rate",
                    "Manual electricity price used for estimated CPU cost.",
                    &mut config.general.energy_rate_dollars_per_kwh,
                    0.0..=2.0,
                    " $/kWh",
                    0.001,
                    changed,
                );
                ui.end_row();

                text_value_cell(
                    ui,
                    "Rate Source",
                    "Short label shown for the manual electricity rate.",
                    &mut config.general.energy_rate_source_label,
                    changed,
                );
                text_value_cell(
                    ui,
                    "CPU Profile Source",
                    "Short label shown for the modeled CPU power profile.",
                    &mut config.general.cpu_power_source_label,
                    changed,
                );
                ui.end_row();
            });

            ui.add_space(design::spacing::SECTION_GAP);
            design::subsection_heading(ui, "CPU Watt Profile");
            ui.add_space(6.0);
            settings_grid(ui, |ui| {
                float_value_cell(
                    ui,
                    "Idle Watts",
                    "Estimated CPU package watts when usage is quiet.",
                    &mut config.general.cpu_idle_watts,
                    0.0..=500.0,
                    " W",
                    0.5,
                    changed,
                );
                float_value_cell(
                    ui,
                    "Base Watts",
                    "Estimated CPU package watts near base frequency under load.",
                    &mut config.general.cpu_base_watts,
                    0.0..=500.0,
                    " W",
                    1.0,
                    changed,
                );
                ui.end_row();

                float_value_cell(
                    ui,
                    "Turbo Watts",
                    "Estimated CPU package watts when running above base frequency.",
                    &mut config.general.cpu_turbo_watts,
                    0.0..=500.0,
                    " W",
                    1.0,
                    changed,
                );
                empty_cell(ui);
                ui.end_row();
            });

            if config.general.cpu_base_watts < config.general.cpu_idle_watts {
                config.general.cpu_base_watts = config.general.cpu_idle_watts;
                *changed = true;
            }
            if config.general.cpu_turbo_watts < config.general.cpu_base_watts {
                config.general.cpu_turbo_watts = config.general.cpu_base_watts;
                *changed = true;
            }
        },
    );
}

fn render_energy_reference_links(ui: &mut Ui) {
    egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(design::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Reference data")
                    .size(design::type_size::LABEL)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("PowerPlanner uses manual estimates in v1. These sources can help set your electricity rate and CPU watt profile.")
                    .weak()
                    .size(design::type_size::HELP),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Energy rates:").strong());
                for (label, url) in ENERGY_RATE_LINKS {
                    ui.hyperlink_to(label, url);
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("CPU watt profile:").strong());
                for (label, url) in CPU_PROFILE_LINKS {
                    ui.hyperlink_to(label, url);
                }
            });
        });
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

fn float_value_cell(
    ui: &mut Ui,
    label: &str,
    description: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    speed: f64,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        design::setting_label(ui, label, description);
        ui.add_space(6.0);
        let numeric = egui::DragValue::new(value)
            .range(range)
            .suffix(suffix)
            .speed(speed);
        let control_width = ui.available_width().min(SETTINGS_VALUE_WIDTH);
        if ui.add_sized([control_width, 30.0], numeric).changed() {
            *changed = true;
        }
    });
}

fn text_value_cell(
    ui: &mut Ui,
    label: &str,
    description: &str,
    value: &mut String,
    changed: &mut bool,
) {
    ui.vertical(|ui| {
        design::setting_label(ui, label, description);
        ui.add_space(6.0);
        let control_width = ui.available_width().min(SETTINGS_COMBO_WIDTH);
        if ui
            .add_sized([control_width, 30.0], egui::TextEdit::singleline(value))
            .changed()
        {
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
