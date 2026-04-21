use crate::config::Config;
use crate::types::{AppState, MonitorCommand};
use egui::Ui;
use egui_extras::{Column, TableBuilder};
use std::sync::mpsc;

pub fn render(
    ui: &mut Ui,
    state: &AppState,
    tx: &mpsc::Sender<MonitorCommand>,
    config: &mut Config,
) {
    ui.heading("Watch List");
    ui.small("These processes trigger High Performance mode when running.");
    ui.separator();

    // ── Current watchlist: 2-column table ([-] | name) ────────────────────
    let mut to_remove: Option<String> = None;
    egui::Grid::new("watchlist_grid")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for proc in &config.watchlist.processes {
                if ui.small_button("–").clicked() {
                    to_remove = Some(proc.clone());
                }
                ui.label(proc);
                ui.end_row();
            }
        });
    if let Some(proc) = to_remove {
        config.watchlist.processes.retain(|p| p != &proc);
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateWatchlist(
            config.watchlist.processes.clone(),
        ));
    }

    ui.add_space(6.0);

    // ── Add by name or browse ──────────────────────────────────────────────
    ui.horizontal(|ui| {
        let id = ui.make_persistent_id("add_proc_input");
        let mut text = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
        let resp = ui.add(egui::TextEdit::singleline(&mut text).hint_text("e.g. cmake.exe"));
        ui.data_mut(|d| d.insert_temp(id, text.clone()));

        if ui.button("Browse…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Executable", &["exe"])
                .set_title("Select executable to watch")
                .pick_file()
            {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    add_by_name(name, config, tx);
                }
            }
        }

        let submit = ui.button("Add").clicked()
            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

        if submit && !text.trim().is_empty() {
            add_by_name(text.trim(), config, tx);
            ui.data_mut(|d| d.insert_temp(id, String::new()));
        }
    });

    ui.separator();

    // ── Running now: 3-column table (name | path | [+]) ───────────────────
    ui.heading("Running Now");
    ui.small("Promote an app to permanently add it to the watch list.");
    ui.add_space(4.0);

    let watchlist_lower: Vec<String> = config
        .watchlist
        .processes
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let unmatched: Vec<&crate::types::RunningProcess> = state
        .all_running_processes
        .iter()
        .filter(|p| !watchlist_lower.contains(&p.name.to_lowercase()))
        .collect();

    let mut to_promote: Option<String> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::auto()) // [+] button
            .column(Column::initial(180.0)) // process name — wider default
            .column(Column::remainder()) // path — fills all remaining width
            .body(|mut body| {
                for proc in &unmatched {
                    body.row(20.0, |mut row| {
                        row.col(|ui| {
                            if ui
                                .small_button("+")
                                .on_hover_text("Add to watch list")
                                .clicked()
                            {
                                to_promote = Some(proc.name.clone());
                            }
                        });
                        row.col(|ui| {
                            ui.label(&proc.name);
                        });
                        row.col(|ui| {
                            let path_text = proc.path.as_deref().unwrap_or("—");
                            ui.add(
                                egui::Label::new(egui::RichText::new(path_text).weak()).truncate(),
                            );
                        });
                    });
                }
            });
    });

    if let Some(name) = to_promote {
        add_by_name(&name, config, tx);
    }
}

fn add_by_name(name: &str, config: &mut Config, tx: &mpsc::Sender<MonitorCommand>) {
    let normalized = if name.to_lowercase().ends_with(".exe") {
        name.to_string()
    } else {
        format!("{}.exe", name)
    };
    if !config.watchlist.processes.contains(&normalized) {
        config.watchlist.processes.push(normalized);
        let _ = crate::config::save(config);
        let _ = tx.send(MonitorCommand::UpdateWatchlist(
            config.watchlist.processes.clone(),
        ));
    }
}
