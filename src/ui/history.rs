// src/ui/history.rs
use egui::Ui;
use egui_extras::{Column, TableBuilder};
use crate::types::AppState;

pub fn render(ui: &mut Ui, state: &AppState) {
    ui.heading("History");

    ui.horizontal(|ui| {
        if ui.button("Export CSV").clicked() {
            export_to_desktop();
        }
        if ui.button("Open Log").clicked() {
            open_log();
        }
    });

    ui.separator();

    // Measure widest plan name so the column never wraps.
    let plan_col_width = {
        let font_id = egui::TextStyle::Body.resolve(ui.style());
        let mut max_w = ui.fonts(|f| f.layout_no_wrap("Plan".to_string(), font_id.clone(), egui::Color32::WHITE).size().x);
        for event in &state.recent_events {
            let w = ui.fonts(|f| f.layout_no_wrap(event.plan_name.clone(), font_id.clone(), egui::Color32::WHITE).size().x);
            if w > max_w { max_w = w; }
        }
        max_w + 8.0
    };

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto())
        .column(Column::initial(plan_col_width))
        .column(Column::remainder())
        .column(Column::auto())
        .header(20.0, |mut h| {
            h.col(|ui| { ui.strong("Time"); });
            h.col(|ui| { ui.strong("Plan"); });
            h.col(|ui| { ui.strong("Trigger"); });
            h.col(|ui| { ui.strong("Power"); });
        })
        .body(|mut body| {
            for event in &state.recent_events {
                body.row(18.0, |mut row| {
                    row.col(|ui| { ui.label(event.ts.format("%Y-%m-%d %H:%M:%S").to_string()); });
                    row.col(|ui| { ui.label(&event.plan_name); });
                    row.col(|ui| { ui.label(&event.trigger); });
                    row.col(|ui| {
                        let power = if event.on_battery {
                            event.battery_pct
                                .map(|p| format!("Battery {}%", p))
                                .unwrap_or_else(|| "Battery".into())
                        } else {
                            "AC".into()
                        };
                        ui.label(power);
                    });
                });
            }
        });

    ui.separator();
    ui.weak("Graph view planned for v2.");
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
