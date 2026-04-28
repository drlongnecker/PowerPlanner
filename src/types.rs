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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuInfo {
    pub manufacturer: String,
    pub brand: String,
    pub base_mhz: Option<u32>,
    pub cores: Option<u32>,
    pub logical_processors: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuFrequencySample {
    pub max_mhz: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessorLimit {
    pub ac: Option<u32>,
    pub dc: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanProcessorSettings {
    pub min_percent: ProcessorLimit,
    pub max_percent: ProcessorLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanProcessorRecommendation {
    pub min_percent: u32,
    pub max_percent: u32,
}

impl PlanProcessorRecommendation {
    pub fn new(min_percent: u32, max_percent: u32) -> Self {
        Self {
            min_percent,
            max_percent,
        }
    }

    pub fn standard_default() -> Self {
        Self {
            min_percent: 5,
            max_percent: 99,
        }
    }

    pub fn low_power_default() -> Self {
        Self {
            min_percent: 0,
            max_percent: 20,
        }
    }

    pub fn performance_default() -> Self {
        Self {
            min_percent: 100,
            max_percent: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanDiagnostics {
    Configured,
    NeedsReview,
    Unavailable,
}

impl PlanDiagnostics {
    pub fn for_settings(
        settings: Option<&PlanProcessorSettings>,
        recommendation: PlanProcessorRecommendation,
    ) -> Self {
        let Some(settings) = settings else {
            return Self::Unavailable;
        };

        let values = [
            settings.min_percent.ac,
            settings.min_percent.dc,
            settings.max_percent.ac,
            settings.max_percent.dc,
        ];
        if values.iter().any(|value| value.is_none()) {
            return Self::Unavailable;
        }

        if settings.min_percent.ac == Some(recommendation.min_percent)
            && settings.min_percent.dc == Some(recommendation.min_percent)
            && settings.max_percent.ac == Some(recommendation.max_percent)
            && settings.max_percent.dc == Some(recommendation.max_percent)
        {
            Self::Configured
        } else {
            Self::NeedsReview
        }
    }
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
    pub fn from_storage(value: i64) -> Self {
        match value {
            0 => Self::LowPower,
            2 => Self::Performance,
            _ => Self::Standard,
        }
    }

    pub fn storage_value(self) -> i64 {
        match self {
            Self::LowPower => 0,
            Self::Standard => 1,
            Self::Performance => 2,
        }
    }

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
    ApplyPlanProcessorRecommendation {
        guid: String,
        recommendation: PlanProcessorRecommendation,
    },
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
    pub cpu_info: Option<CpuInfo>,
    pub cpu_frequency: CpuFrequencySample,
    pub turbo_rescue_state: String,
    pub plan_processor_settings: std::collections::BTreeMap<String, Option<PlanProcessorSettings>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(min: u32, max: u32) -> PlanProcessorSettings {
        PlanProcessorSettings {
            min_percent: ProcessorLimit {
                ac: Some(min),
                dc: Some(min),
            },
            max_percent: ProcessorLimit {
                ac: Some(max),
                dc: Some(max),
            },
        }
    }

    #[test]
    fn plan_diagnostics_marks_matching_values_configured() {
        assert_eq!(
            PlanDiagnostics::for_settings(
                Some(&settings(5, 99)),
                PlanProcessorRecommendation::standard_default()
            ),
            PlanDiagnostics::Configured
        );
    }

    #[test]
    fn plan_diagnostics_marks_mismatched_values_for_review() {
        assert_eq!(
            PlanDiagnostics::for_settings(
                Some(&settings(100, 100)),
                PlanProcessorRecommendation::standard_default()
            ),
            PlanDiagnostics::NeedsReview
        );
    }

    #[test]
    fn plan_diagnostics_marks_missing_values_unavailable() {
        let settings = PlanProcessorSettings {
            min_percent: ProcessorLimit {
                ac: Some(5),
                dc: None,
            },
            max_percent: ProcessorLimit {
                ac: Some(99),
                dc: Some(99),
            },
        };

        assert_eq!(
            PlanDiagnostics::for_settings(
                Some(&settings),
                PlanProcessorRecommendation::standard_default(),
            ),
            PlanDiagnostics::Unavailable
        );
    }

    #[test]
    fn plan_diagnostics_marks_unreadable_settings_unavailable() {
        assert_eq!(
            PlanDiagnostics::for_settings(None, PlanProcessorRecommendation::standard_default()),
            PlanDiagnostics::Unavailable
        );
    }
}
