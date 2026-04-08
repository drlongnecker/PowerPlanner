// src/ui/dashboard.rs
use egui::Ui;
use egui_extras::{Column, TableBuilder};
use std::sync::mpsc;
use crate::config::Config;
use crate::types::{AppState, MonitorCommand};

pub fn render(
    ui: &mut Ui,
    state: &AppState,
    config: &mut Config,
    tx: &mpsc::Sender<MonitorCommand>,
) {
    // Current plan banner
    let plan_name = state.current_plan.as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown");
    ui.heading(format!("Current Plan: {}", plan_name));

    ui.horizontal(|ui| {
        if ui.button("Set as Idle Plan").clicked() {
            if let Some(ref plan) = state.current_plan {
                config.general.idle_plan_guid = plan.guid.clone();
                let _ = crate::config::save(config);
                let _ = tx.send(MonitorCommand::UpdateConfig(config.clone()));
            }
        }
    });

    ui.separator();

    // Battery / AC status
    let bat = &state.battery;
    if bat.percent.is_none() {
        ui.label("Desktop (no battery)");
    } else if bat.on_battery {
        let pct = bat.percent.unwrap_or(0);
        ui.label(format!(
            "On Battery — {}%{}",
            pct,
            if bat.charging { " (charging)" } else { "" }
        ));
    } else {
        ui.label("AC Connected");
    }

    // Monitor status
    let status = if state.monitor_running { "Monitor: Running" } else { "Monitor: Stopped" };
    ui.label(status);

    // Active triggers
    if !state.matched_processes.is_empty() {
        ui.label(format!("Active triggers: {}", state.matched_processes.join(", ")));
    }

    // Hold timer countdown
    if let Some(r) = state.hold_remaining_secs {
        if r > 0.0 {
            ui.label(format!("Hold timer: {:.0}s remaining", r));
        }
    }

    // Forced plan banner
    if let Some(ref forced) = state.forced_plan {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::YELLOW, format!("Forced: {}", forced.name));
            if ui.button("Resume Auto").clicked() {
                let _ = tx.send(MonitorCommand::ForcePlan(None));
            }
        });
    }

    // Error banner
    if let Some(ref err) = state.last_error {
        ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
    }

    ui.separator();
    ui.heading("Recent Events");

    // Measure the widest plan name so the column never wraps.
    let plan_col_width = {
        let font_id = egui::TextStyle::Body.resolve(ui.style());
        let mut max_w = ui.fonts(|f| f.layout_no_wrap("Plan".to_string(), font_id.clone(), egui::Color32::WHITE).size().x);
        for event in state.recent_events.iter().take(10) {
            let w = ui.fonts(|f| f.layout_no_wrap(event.plan_name.clone(), font_id.clone(), egui::Color32::WHITE).size().x);
            if w > max_w { max_w = w; }
        }
        max_w + 8.0
    };

    // TableBuilder handles its own scroll area and keeps the header
    // pinned above the rows so it doesn't scroll out of view.
    TableBuilder::new(ui)
        .striped(true)
        .max_scroll_height(200.0)
        .column(Column::auto())
        .column(Column::initial(plan_col_width))
        .column(Column::remainder())
        .header(20.0, |mut h| {
            h.col(|ui| { ui.strong("Time"); });
            h.col(|ui| { ui.strong("Plan"); });
            h.col(|ui| { ui.strong("Trigger"); });
        })
        .body(|mut body| {
            for event in state.recent_events.iter().take(10) {
                body.row(18.0, |mut row| {
                    row.col(|ui| { ui.label(event.ts.format("%H:%M:%S").to_string()); });
                    row.col(|ui| { ui.label(&event.plan_name); });
                    row.col(|ui| { ui.label(&event.trigger); });
                });
            }
        });
}
