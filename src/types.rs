// src/types.rs
use chrono::{DateTime, Local};
use egui::Color32;
use std::collections::VecDeque;

/// A deduplicated snapshot of a running process (name + optional exe path).
#[derive(Debug, Clone, Default)]
pub struct RunningProcess {
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryStatus {
    pub on_battery: bool,
    pub percent: Option<u8>, // None = desktop (no battery)
    pub charging: bool,
}

#[derive(Debug, Clone)]
pub struct PowerEvent {
    pub ts: DateTime<Local>,
    pub plan_name: String,
    pub plan_guid: String,
    pub trigger: String, // process name | "manual" | "hold expired" | "startup"
    pub on_battery: bool,
    pub battery_pct: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CpuHistoryPlanKind {
    LowPower,
    #[default]
    Standard,
    Performance,
}

impl CpuHistoryPlanKind {
    pub fn color(self) -> Color32 {
        match self {
            Self::LowPower => Color32::from_rgb(0xC9, 0xCB, 0xA3),
            Self::Standard => Color32::from_rgb(0xFF, 0xE1, 0xA8),
            Self::Performance => Color32::from_rgb(0x00, 0xA9, 0xA5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CpuHistoryPoint {
    pub ts: DateTime<Local>,
    pub average_percent: f32,
    pub plan_kind: CpuHistoryPlanKind,
    pub plan_name: String,
    pub trigger: String,
}

#[derive(Debug)]
pub enum MonitorCommand {
    ForcePlan(Option<String>), // Some(guid) = force and lock; None = clear force, resume auto
    UpdateWatchlist(Vec<String>), // replace watchlist; monitor picks up next tick
    UpdateConfig(crate::config::Config), // replaces full config; monitor picks up next tick
    RefreshPlans,
    Stop,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub current_plan: Option<PowerPlan>,
    pub available_plans: Vec<PowerPlan>,
    pub matched_processes: Vec<String>,
    pub all_running_processes: Vec<RunningProcess>,
    pub hold_remaining_secs: Option<f32>,
    pub idle_for_secs: Option<f32>,
    pub cpu_average_percent: Option<f32>,
    pub cpu_history: VecDeque<CpuHistoryPoint>,
    pub low_power_ready_input: bool,
    pub low_power_ready_cpu: bool,
    pub battery: BatteryStatus,
    pub monitor_running: bool,
    pub recent_events: VecDeque<PowerEvent>,
    pub last_error: Option<String>,
    pub forced_plan: Option<PowerPlan>,
}

impl AppState {
    pub fn push_event(&mut self, event: PowerEvent) {
        self.recent_events.push_front(event);
        if self.recent_events.len() > 50 {
            self.recent_events.pop_back();
        }
    }
}
