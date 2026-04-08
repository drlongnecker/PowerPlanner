// src/ui/history.rs
use egui::Ui;
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

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("history_grid")
            .num_columns(4)
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.strong("Time");
                ui.strong("Plan");
                ui.strong("Trigger");
                ui.strong("Power");
                ui.end_row();

                for event in &state.recent_events {
                    ui.label(event.ts.format("%Y-%m-%d %H:%M:%S").to_string());
                    ui.label(&event.plan_name);
                    ui.label(&event.trigger);
                    let power = if event.on_battery {
                        event.battery_pct
                            .map(|p| format!("Battery {}%", p))
                            .unwrap_or_else(|| "Battery".into())
                    } else {
                        "AC".into()
                    };
                    ui.label(power);
                    ui.end_row();
                }
            });
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
