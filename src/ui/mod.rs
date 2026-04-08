// src/ui/mod.rs
pub mod dashboard;
pub mod history;
pub mod settings;
pub mod watched;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Nav {
    #[default]
    Dashboard,
    WatchedApps,
    Settings,
    History,
}
