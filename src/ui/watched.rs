use crate::config::Config;
use crate::types::{AppState, MonitorCommand};
use crate::ui::design;
use egui::Ui;
use egui_extras::{Column, TableBuilder};
use std::sync::mpsc;

const ACTION_COLUMN_WIDTH: f32 = 38.0;
const NAME_COLUMN_WIDTH: f32 = 180.0;
const WATCH_ROW_HEIGHT: f32 = 32.0;

pub fn render(
    ui: &mut Ui,
    state: &AppState,
    tx: &mpsc::Sender<MonitorCommand>,
    config: &mut Config,
) {
    crate::ui::padded_page(ui, |ui| {
        design::page_header(
            ui,
            "Watched Apps",
            "Processes here trigger High Performance mode when running.",
        );

        design::section(
            ui,
            "Watch List",
            "Add executables by name or browse for an installed app.",
            |ui| {
                ui.horizontal_top(|ui| {
                    let id = ui.make_persistent_id("add_proc_input");
                    let mut text = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
                    let input_width = (ui.available_width() - 188.0).max(180.0);
                    let resp = ui.add_sized(
                        [input_width, 30.0],
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("e.g. cmake.exe")
                            .margin(egui::vec2(8.0, 5.0)),
                    );
                    ui.data_mut(|d| d.insert_temp(id, text.clone()));

                    if design::command_button(ui, "Browse").clicked() {
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

                    let submit = design::accent_command_button(ui, "Add").clicked()
                        || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                    if submit && !text.trim().is_empty() {
                        add_by_name(text.trim(), config, tx);
                        ui.data_mut(|d| d.insert_temp(id, String::new()));
                    }
                });

                ui.add_space(design::spacing::ROW_GAP);

                let mut to_remove: Option<String> = None;
                TableBuilder::new(ui)
                    .striped(true)
                    .column(Column::exact(ACTION_COLUMN_WIDTH))
                    .column(Column::initial(NAME_COLUMN_WIDTH))
                    .body(|mut body| {
                        for proc in &config.watchlist.processes {
                            body.row(WATCH_ROW_HEIGHT, |mut row| {
                                row.col(|ui| {
                                    if design::icon_button(
                                        ui,
                                        "-",
                                        "Remove from watch list",
                                        design::color::DANGER,
                                    )
                                    .clicked()
                                    {
                                        to_remove = Some(proc.clone());
                                    }
                                });
                                row.col(|ui| {
                                    ui.label(proc);
                                });
                            });
                        }
                    });
                if let Some(proc) = to_remove {
                    config.watchlist.processes.retain(|p| p != &proc);
                    let _ = crate::config::save(config);
                    let _ = tx.send(MonitorCommand::UpdateWatchlist(
                        config.watchlist.processes.clone(),
                    ));
                }
            },
        );

        ui.add_space(design::spacing::SECTION_GAP);

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
        design::section(
            ui,
            "Running Now",
            "Promote an app to permanently add it to the watch list.",
            |ui| {
                let path_width =
                    (ui.available_width() - ACTION_COLUMN_WIDTH - NAME_COLUMN_WIDTH - 28.0)
                        .max(160.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .column(Column::exact(ACTION_COLUMN_WIDTH))
                        .column(Column::initial(NAME_COLUMN_WIDTH))
                        .column(Column::exact(path_width))
                        .body(|mut body| {
                            for proc in &unmatched {
                                body.row(WATCH_ROW_HEIGHT, |mut row| {
                                    row.col(|ui| {
                                        if design::icon_button(
                                            ui,
                                            "+",
                                            "Add to watch list",
                                            design::color::SUCCESS,
                                        )
                                        .clicked()
                                        {
                                            to_promote = Some(proc.name.clone());
                                        }
                                    });
                                    row.col(|ui| {
                                        ui.label(&proc.name);
                                    });
                                    row.col(|ui| {
                                        let path_text = proc.path.as_deref().unwrap_or("-");
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(path_text)
                                                    .weak()
                                                    .size(design::type_size::HELP),
                                            )
                                            .truncate(),
                                        );
                                    });
                                });
                            }
                        });
                });
            },
        );

        if let Some(name) = to_promote {
            add_by_name(&name, config, tx);
        }
    });
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
