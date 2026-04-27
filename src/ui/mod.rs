pub mod dashboard;
pub mod design;
pub mod history;
pub mod settings;
pub mod watched;

use egui::{self, Align, Layout, Ui};

pub fn padded_page(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let content_width = (ui.available_width() - design::spacing::PAGE_X * 2.0).max(320.0);
    let content_height = ui.available_height().max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(design::spacing::PAGE_X);
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, content_height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.add_space(design::spacing::PAGE_Y);
                add_contents(ui);
            },
        );
        ui.add_space(design::spacing::PAGE_X);
    });
}

#[derive(Clone, PartialEq, Default)]
pub enum Nav {
    #[default]
    Dashboard,
    WatchedApps,
    Settings,
    History,
}

#[cfg(test)]
mod tests {
    use super::design;

    #[test]
    fn design_tokens_keep_settings_and_dashboard_on_one_scale() {
        assert_eq!(design::type_size::PAGE_TITLE, 24.0);
        assert_eq!(design::type_size::SECTION_TITLE, 18.0);
        assert_eq!(design::type_size::LABEL, 14.0);
        assert_eq!(design::type_size::HELP, 12.5);
        assert_eq!(design::spacing::SECTION_GAP, 12.0);
        assert_eq!(design::radius::SECTION, 8.0);
    }

    #[test]
    fn status_badge_copy_is_consistent_for_enabled_states() {
        assert_eq!(design::enabled_status_text(true), "On");
        assert_eq!(design::enabled_status_text(false), "Off");
        assert_eq!(design::registered_status_text(true), "Registered");
        assert_eq!(design::registered_status_text(false), "Not registered");
    }
}
