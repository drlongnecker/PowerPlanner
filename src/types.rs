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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    pub efficiency_classes: Vec<CpuEfficiencyClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuEfficiencyClass {
    pub value: u8,
    pub logical_processors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuTopologyKind {
    Unknown,
    Homogeneous,
    Hybrid,
}

impl CpuInfo {
    pub fn topology_kind(&self) -> CpuTopologyKind {
        match self.efficiency_classes.len() {
            1 => CpuTopologyKind::Homogeneous,
            2 => CpuTopologyKind::Hybrid,
            _ => CpuTopologyKind::Unknown,
        }
    }
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
    pub boost_mode: ProcessorLimit,
    pub core_parking_min_cores_percent: ProcessorLimit,
    pub latency_hint_perf: ProcessorLimit,
    pub idle_promote_threshold: ProcessorLimit,
    pub class1_min_percent: ProcessorLimit,
    pub class1_max_percent: ProcessorLimit,
    pub class1_core_parking_min_cores_percent: ProcessorLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessorClassRecommendation {
    pub min_percent: u32,
    pub max_percent: u32,
    pub core_parking_min_cores_percent: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessorPresetRecommendation {
    pub min_percent: u32,
    pub max_percent: u32,
    pub boost_mode: Option<u32>,
    pub core_parking_min_cores_percent: Option<u32>,
    pub latency_hint_perf: Option<u32>,
    pub idle_promote_threshold: Option<u32>,
    pub class1: Option<ProcessorClassRecommendation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorPresetDiagnostics {
    Configured,
    NeedsReview,
    Unavailable,
}

impl ProcessorPresetDiagnostics {
    pub fn for_settings(
        settings: Option<&PlanProcessorSettings>,
        recommendation: ProcessorPresetRecommendation,
    ) -> Self {
        let Some(settings) = settings else {
            return Self::Unavailable;
        };

        let mut values = vec![
            settings.min_percent.ac,
            settings.min_percent.dc,
            settings.max_percent.ac,
            settings.max_percent.dc,
        ];
        if recommendation.boost_mode.is_some() {
            values.extend([settings.boost_mode.ac, settings.boost_mode.dc]);
        }
        if recommendation.core_parking_min_cores_percent.is_some() {
            values.extend([
                settings.core_parking_min_cores_percent.ac,
                settings.core_parking_min_cores_percent.dc,
            ]);
        }
        if recommendation.latency_hint_perf.is_some() {
            values.extend([settings.latency_hint_perf.ac, settings.latency_hint_perf.dc]);
        }
        if recommendation.idle_promote_threshold.is_some() {
            values.extend([
                settings.idle_promote_threshold.ac,
                settings.idle_promote_threshold.dc,
            ]);
        }
        if let Some(class1) = recommendation.class1 {
            values.extend([
                settings.class1_min_percent.ac,
                settings.class1_min_percent.dc,
                settings.class1_max_percent.ac,
                settings.class1_max_percent.dc,
            ]);
            if class1.core_parking_min_cores_percent.is_some() {
                values.extend([
                    settings.class1_core_parking_min_cores_percent.ac,
                    settings.class1_core_parking_min_cores_percent.dc,
                ]);
            }
        }
        if values.iter().any(|value| value.is_none()) {
            return Self::Unavailable;
        }

        if settings.min_percent.ac == Some(recommendation.min_percent)
            && settings.min_percent.dc == Some(recommendation.min_percent)
            && settings.max_percent.ac == Some(recommendation.max_percent)
            && settings.max_percent.dc == Some(recommendation.max_percent)
            && recommendation
                .boost_mode
                .is_none_or(|value| settings.boost_mode.ac == Some(value))
            && recommendation
                .boost_mode
                .is_none_or(|value| settings.boost_mode.dc == Some(value))
            && recommendation
                .core_parking_min_cores_percent
                .is_none_or(|value| {
                    settings.core_parking_min_cores_percent.ac == Some(value)
                        && settings.core_parking_min_cores_percent.dc == Some(value)
                })
            && recommendation.latency_hint_perf.is_none_or(|value| {
                settings.latency_hint_perf.ac == Some(value)
                    && settings.latency_hint_perf.dc == Some(value)
            })
            && recommendation.idle_promote_threshold.is_none_or(|value| {
                settings.idle_promote_threshold.ac == Some(value)
                    && settings.idle_promote_threshold.dc == Some(value)
            })
            && recommendation.class1.is_none_or(|class1| {
                settings.class1_min_percent.ac == Some(class1.min_percent)
                    && settings.class1_min_percent.dc == Some(class1.min_percent)
                    && settings.class1_max_percent.ac == Some(class1.max_percent)
                    && settings.class1_max_percent.dc == Some(class1.max_percent)
                    && class1.core_parking_min_cores_percent.is_none_or(|value| {
                        settings.class1_core_parking_min_cores_percent.ac == Some(value)
                            && settings.class1_core_parking_min_cores_percent.dc == Some(value)
                    })
            })
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
    #[allow(dead_code)]
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
    pub current_mhz: Option<u32>,
    pub base_mhz: Option<u32>,
    pub plan_kind: CpuHistoryPlanKind,
    pub plan_name: String,
    pub trigger: String,
    pub energy: Option<CpuHistoryEnergyEstimate>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuHistoryEnergyEstimate {
    pub estimated_watts: f64,
    pub estimated_kwh: f64,
    pub estimated_cost_usd: f64,
    pub baseline_watts: f64,
    pub baseline_cost_usd: f64,
    pub estimated_savings_usd: f64,
}

#[derive(Debug)]
pub enum MonitorCommand {
    ForcePlan(Option<String>), // Some(guid) = force and lock; None = clear force, resume auto
    ForceConfiguredStandard,
    ForceConfiguredPerformance,
    UpdateWatchlist(Vec<String>), // replace watchlist; monitor picks up next tick
    UpdateConfig(crate::config::Config), // replaces full config; monitor picks up next tick
    ApplyProcessorPreset {
        guid: String,
        recommendation: ProcessorPresetRecommendation,
    },
    InstallUltimatePerformance {
        recommendation: ProcessorPresetRecommendation,
    },
    RefreshPlans,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UltimatePerformanceSetupState {
    #[default]
    Idle,
    Pending,
    Succeeded(PowerPlan),
    Failed(String),
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
    pub ultimate_performance_setup: UltimatePerformanceSetupState,
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
            boost_mode: ProcessorLimit {
                ac: Some(2),
                dc: Some(2),
            },
            core_parking_min_cores_percent: ProcessorLimit {
                ac: Some(100),
                dc: Some(100),
            },
            latency_hint_perf: ProcessorLimit::default(),
            idle_promote_threshold: ProcessorLimit::default(),
            class1_min_percent: ProcessorLimit::default(),
            class1_max_percent: ProcessorLimit::default(),
            class1_core_parking_min_cores_percent: ProcessorLimit::default(),
        }
    }

    #[test]
    fn processor_preset_diagnostics_cover_hidden_processor_settings() {
        let recommendation = ProcessorPresetRecommendation {
            min_percent: 100,
            max_percent: 100,
            boost_mode: Some(2),
            core_parking_min_cores_percent: Some(100),
            latency_hint_perf: None,
            idle_promote_threshold: None,
            class1: None,
        };
        let configured = settings(100, 100);
        assert_eq!(
            ProcessorPresetDiagnostics::for_settings(Some(&configured), recommendation),
            ProcessorPresetDiagnostics::Configured
        );

        let mut needs_review = configured;
        needs_review.boost_mode.ac = Some(1);
        assert_eq!(
            ProcessorPresetDiagnostics::for_settings(Some(&needs_review), recommendation),
            ProcessorPresetDiagnostics::NeedsReview
        );

        let mut unavailable = configured;
        unavailable.core_parking_min_cores_percent.dc = None;
        assert_eq!(
            ProcessorPresetDiagnostics::for_settings(Some(&unavailable), recommendation),
            ProcessorPresetDiagnostics::Unavailable
        );
    }

    #[test]
    fn cpu_topology_classification_is_conservative() {
        let mut info = CpuInfo::default();
        assert_eq!(info.topology_kind(), CpuTopologyKind::Unknown);

        info.efficiency_classes = vec![CpuEfficiencyClass {
            value: 0,
            logical_processors: 8,
        }];
        assert_eq!(info.topology_kind(), CpuTopologyKind::Homogeneous);

        info.efficiency_classes.push(CpuEfficiencyClass {
            value: 2,
            logical_processors: 8,
        });
        assert_eq!(info.topology_kind(), CpuTopologyKind::Hybrid);

        info.efficiency_classes.push(CpuEfficiencyClass {
            value: 7,
            logical_processors: 2,
        });
        assert_eq!(info.topology_kind(), CpuTopologyKind::Unknown);
    }

    #[test]
    fn hybrid_preset_requires_class1_values_but_ignores_preserved_parking() {
        let mut configured = settings(5, 99);
        configured.boost_mode = ProcessorLimit {
            ac: Some(0),
            dc: Some(0),
        };
        configured.class1_min_percent = ProcessorLimit {
            ac: Some(5),
            dc: Some(5),
        };
        configured.class1_max_percent = ProcessorLimit {
            ac: Some(99),
            dc: Some(99),
        };
        let recommendation = ProcessorPresetRecommendation {
            min_percent: 5,
            max_percent: 99,
            boost_mode: Some(0),
            core_parking_min_cores_percent: None,
            latency_hint_perf: None,
            idle_promote_threshold: None,
            class1: Some(ProcessorClassRecommendation {
                min_percent: 5,
                max_percent: 99,
                core_parking_min_cores_percent: None,
            }),
        };

        assert_eq!(
            ProcessorPresetDiagnostics::for_settings(Some(&configured), recommendation),
            ProcessorPresetDiagnostics::Configured
        );
        configured.class1_max_percent.dc = None;
        assert_eq!(
            ProcessorPresetDiagnostics::for_settings(Some(&configured), recommendation),
            ProcessorPresetDiagnostics::Unavailable
        );
    }
}
