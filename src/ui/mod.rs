// src/ui/mod.rs
pub mod dashboard;
pub mod history;
pub mod settings;
pub mod watched;

use egui::{self, Align, Layout, Ui};

const PAGE_CONTENT_INSET: f32 = 10.0;

pub fn padded_page(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let content_width = (ui.available_width() - PAGE_CONTENT_INSET * 2.0).max(320.0);
    let content_height = ui.available_height().max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(PAGE_CONTENT_INSET);
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, content_height),
            Layout::top_down(Align::Min),
            add_contents,
        );
        ui.add_space(PAGE_CONTENT_INSET);
    });
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Nav {
    #[default]
    Dashboard,
    WatchedApps,
    Settings,
    History,
}
