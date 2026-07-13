// src/config.rs
use crate::energy::{CpuPowerProfile, EnergyRate};
use crate::types::{
    CpuTopologyKind, PowerPlan, ProcessorClassRecommendation, ProcessorPresetRecommendation,
};
use anyhow::Result as AnyResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    pub general: GeneralConfig,
    pub autostart: AutostartConfig,
    pub watchlist: WatchlistConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub(crate) enum PlanTimeRangeMode {
    #[default]
    MatchUsageTrend,
    AllRetained,
}

impl<'de> Deserialize<'de> for PlanTimeRangeMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)
            .unwrap_or(toml::Value::String("match_usage_trend".to_string()));
        let mode = value
            .as_str()
            .map(str::to_ascii_lowercase)
            .map(|value| match value.as_str() {
                "all_retained" => Self::AllRetained,
                _ => Self::MatchUsageTrend,
            })
            .unwrap_or_default();
        Ok(mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub(crate) enum PowerUsageRangeMode {
    #[default]
    RecentMinutes,
    AllRetained,
}

impl<'de> Deserialize<'de> for PowerUsageRangeMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)
            .unwrap_or(toml::Value::String("recent_minutes".to_string()));
        let mode = value
            .as_str()
            .map(str::to_ascii_lowercase)
            .map(|value| match value.as_str() {
                "all_retained" => Self::AllRetained,
                _ => Self::RecentMinutes,
            })
            .unwrap_or_default();
        Ok(mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub(crate) enum AppearanceMode {
    #[default]
    System,
    Light,
    Dark,
}

impl<'de> Deserialize<'de> for AppearanceMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)
            .unwrap_or(toml::Value::String("system".to_string()));
        let mode = value
            .as_str()
            .map(str::to_ascii_lowercase)
            .map(|value| match value.as_str() {
                "light" => Self::Light,
                "dark" => Self::Dark,
                _ => Self::System,
            })
            .unwrap_or_default();
        Ok(mode)
    }
}

// Config flags map directly to independent user-facing settings; not state-machine states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneralConfig {
    pub poll_interval_ms: u64,
    pub hold_performance_seconds: u64,
    #[serde(alias = "idle_plan_guid")]
    pub standard_plan_guid: String,
    #[serde(default)]
    pub low_power_plan_guid: String,
    pub performance_plan_guid: String,
    #[serde(default = "default_standard_cpu_min_percent")]
    pub standard_cpu_min_percent: u8,
    #[serde(default = "default_standard_cpu_max_percent")]
    pub standard_cpu_max_percent: u8,
    #[serde(default = "default_standard_boost_mode")]
    pub standard_boost_mode: u8,
    #[serde(default = "default_standard_core_parking_min_cores_percent")]
    pub standard_core_parking_min_cores_percent: Option<u8>,
    #[serde(default = "default_performance_cpu_min_percent")]
    pub performance_cpu_min_percent: u8,
    #[serde(default = "default_performance_cpu_max_percent")]
    pub performance_cpu_max_percent: u8,
    #[serde(default = "default_performance_boost_mode")]
    pub performance_boost_mode: u8,
    #[serde(default = "default_performance_core_parking_min_cores_percent")]
    pub performance_core_parking_min_cores_percent: u8,
    #[serde(default = "default_low_power_cpu_min_percent")]
    pub low_power_cpu_min_percent: u8,
    #[serde(default = "default_low_power_cpu_max_percent")]
    pub low_power_cpu_max_percent: u8,
    #[serde(default = "default_low_power_boost_mode")]
    pub low_power_boost_mode: u8,
    #[serde(default = "default_low_power_core_parking_min_cores_percent")]
    pub low_power_core_parking_min_cores_percent: Option<u8>,
    #[serde(default = "default_idle_wait_seconds")]
    pub idle_wait_seconds: u64,
    #[serde(
        default = "default_cpu_average_threshold_percent",
        alias = "low_power_cpu_threshold_percent"
    )]
    pub cpu_average_threshold_percent: u8,
    #[serde(
        default = "default_cpu_average_window_seconds",
        alias = "low_power_cpu_quiet_window_seconds"
    )]
    pub cpu_average_window_seconds: u64,
    #[serde(default = "default_turbo_rescue_enabled")]
    pub turbo_rescue_enabled: bool,
    #[serde(default = "default_turbo_rescue_cpu_threshold_percent")]
    pub turbo_rescue_cpu_threshold_percent: u8,
    #[serde(default = "default_turbo_rescue_window_seconds")]
    pub turbo_rescue_window_seconds: u64,
    #[serde(
        default = "default_usage_trend_window_minutes",
        deserialize_with = "deserialize_usage_trend_window_minutes"
    )]
    pub usage_trend_window_minutes: u64,
    #[serde(default)]
    pub plan_time_range_mode: PlanTimeRangeMode,
    #[serde(default)]
    pub appearance_mode: AppearanceMode,
    #[serde(default)]
    pub power_usage_range_mode: PowerUsageRangeMode,
    #[serde(default = "default_energy_estimates_enabled")]
    pub energy_estimates_enabled: bool,
    #[serde(default = "default_energy_rate_dollars_per_kwh")]
    pub energy_rate_dollars_per_kwh: f64,
    #[serde(default = "default_energy_rate_source_label")]
    pub energy_rate_source_label: String,
    #[serde(default = "default_cpu_idle_watts")]
    pub cpu_idle_watts: f64,
    #[serde(default = "default_cpu_base_watts")]
    pub cpu_base_watts: f64,
    #[serde(default = "default_cpu_turbo_watts")]
    pub cpu_turbo_watts: f64,
    #[serde(default = "default_cpu_power_source_label")]
    pub cpu_power_source_label: String,
    pub promote_on_battery: bool,
    pub show_tray_balloon_on_switch: bool,
}

fn default_idle_wait_seconds() -> u64 {
    600
}
fn default_standard_cpu_min_percent() -> u8 {
    5
}
fn default_standard_cpu_max_percent() -> u8 {
    99
}
fn default_standard_boost_mode() -> u8 {
    1
}
fn default_standard_core_parking_min_cores_percent() -> Option<u8> {
    None
}
fn default_performance_cpu_min_percent() -> u8 {
    100
}
fn default_performance_cpu_max_percent() -> u8 {
    100
}
fn default_performance_boost_mode() -> u8 {
    2
}
fn default_performance_core_parking_min_cores_percent() -> u8 {
    100
}
fn default_low_power_cpu_min_percent() -> u8 {
    0
}
fn default_low_power_cpu_max_percent() -> u8 {
    20
}
fn default_low_power_boost_mode() -> u8 {
    0
}
// Return type is `Option` to match the `low_power_core_parking_min_cores_percent` field type for serde's `default`.
#[allow(clippy::unnecessary_wraps)]
fn default_low_power_core_parking_min_cores_percent() -> Option<u8> {
    Some(0)
}
fn default_cpu_average_threshold_percent() -> u8 {
    10
}
fn default_cpu_average_window_seconds() -> u64 {
    60
}
fn default_turbo_rescue_enabled() -> bool {
    true
}
fn default_turbo_rescue_cpu_threshold_percent() -> u8 {
    10
}
fn default_turbo_rescue_window_seconds() -> u64 {
    15
}
fn default_usage_trend_window_minutes() -> u64 {
    15
}
fn default_energy_estimates_enabled() -> bool {
    true
}
fn default_energy_rate_dollars_per_kwh() -> f64 {
    0.15
}
fn default_energy_rate_source_label() -> String {
    "Manual".to_string()
}
fn default_cpu_idle_watts() -> f64 {
    12.0
}
fn default_cpu_base_watts() -> f64 {
    65.0
}
fn default_cpu_turbo_watts() -> f64 {
    125.0
}
fn default_cpu_power_source_label() -> String {
    "Estimated from CPU profile".to_string()
}

fn is_supported_usage_trend_window_minutes(value: u64) -> bool {
    matches!(value, 15 | 30 | 60 | 90 | 120)
}

// Return type is `Result` because serde's `deserialize_with` requires it.
#[allow(clippy::unnecessary_wraps)]
fn deserialize_usage_trend_window_minutes<'de, D>(
    deserializer: D,
) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer).unwrap_or(toml::Value::Integer(
        default_usage_trend_window_minutes().cast_signed(),
    ));
    let minutes = value
        .as_integer()
        .unwrap_or(default_usage_trend_window_minutes().cast_signed()) as u64;
    if is_supported_usage_trend_window_minutes(minutes) {
        Ok(minutes)
    } else {
        Ok(default_usage_trend_window_minutes())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AutostartConfig {
    pub registered: bool,
    #[serde(skip)]
    pub is_elevated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WatchlistConfig {
    pub processes: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                poll_interval_ms: 1_000,
                hold_performance_seconds: 25,
                standard_plan_guid: String::new(),
                low_power_plan_guid: String::new(),
                performance_plan_guid: String::new(),
                standard_cpu_min_percent: default_standard_cpu_min_percent(),
                standard_cpu_max_percent: default_standard_cpu_max_percent(),
                standard_boost_mode: default_standard_boost_mode(),
                standard_core_parking_min_cores_percent:
                    default_standard_core_parking_min_cores_percent(),
                performance_cpu_min_percent: default_performance_cpu_min_percent(),
                performance_cpu_max_percent: default_performance_cpu_max_percent(),
                performance_boost_mode: default_performance_boost_mode(),
                performance_core_parking_min_cores_percent:
                    default_performance_core_parking_min_cores_percent(),
                low_power_cpu_min_percent: default_low_power_cpu_min_percent(),
                low_power_cpu_max_percent: default_low_power_cpu_max_percent(),
                low_power_boost_mode: default_low_power_boost_mode(),
                low_power_core_parking_min_cores_percent:
                    default_low_power_core_parking_min_cores_percent(),
                idle_wait_seconds: default_idle_wait_seconds(),
                cpu_average_threshold_percent: default_cpu_average_threshold_percent(),
                cpu_average_window_seconds: default_cpu_average_window_seconds(),
                turbo_rescue_enabled: default_turbo_rescue_enabled(),
                turbo_rescue_cpu_threshold_percent: default_turbo_rescue_cpu_threshold_percent(),
                turbo_rescue_window_seconds: default_turbo_rescue_window_seconds(),
                usage_trend_window_minutes: default_usage_trend_window_minutes(),
                plan_time_range_mode: PlanTimeRangeMode::default(),
                appearance_mode: AppearanceMode::default(),
                power_usage_range_mode: PowerUsageRangeMode::default(),
                energy_estimates_enabled: default_energy_estimates_enabled(),
                energy_rate_dollars_per_kwh: default_energy_rate_dollars_per_kwh(),
                energy_rate_source_label: default_energy_rate_source_label(),
                cpu_idle_watts: default_cpu_idle_watts(),
                cpu_base_watts: default_cpu_base_watts(),
                cpu_turbo_watts: default_cpu_turbo_watts(),
                cpu_power_source_label: default_cpu_power_source_label(),
                promote_on_battery: false,
                show_tray_balloon_on_switch: true,
            },
            autostart: AutostartConfig {
                registered: false,
                is_elevated: false,
            },
            watchlist: WatchlistConfig { processes: vec![] },
        }
    }
}

pub(crate) fn config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PowerPlanner")
        .join("config.toml")
}

/// Returns (config, `is_first_run`).
pub(crate) fn load_or_default() -> (Config, bool) {
    let path = config_path();
    if !path.exists() {
        return (Config::default(), true);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut config: Config = toml::from_str(&text).unwrap_or_default();
    migrate_legacy_idle_wait(&text, &mut config);
    (config, false)
}

pub(crate) fn save(config: &Config) -> AnyResult<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config)?;
    std::fs::write(&path, text)?;
    Ok(())
}

fn migrate_legacy_idle_wait(text: &str, config: &mut Config) {
    if text.contains("idle_wait_seconds") {
        return;
    }

    let Ok(value) = text.parse::<toml::Value>() else {
        return;
    };

    let Some(minutes) = value
        .get("general")
        .and_then(|general| general.get("idle_wait_minutes"))
        .and_then(toml::Value::as_integer)
    else {
        return;
    };

    if minutes >= 0 {
        config.general.idle_wait_seconds = (minutes as u64).saturating_mul(60);
    }
}

fn discover_plan_guid_by_name(plans: &[PowerPlan], candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find_map(|candidate| {
            plans.iter().find(|plan| {
                plan.name.eq_ignore_ascii_case(candidate)
                    || plan
                        .name
                        .to_ascii_lowercase()
                        .contains(&candidate.to_ascii_lowercase())
            })
        })
        .map(|plan| plan.guid.clone())
}

pub(crate) fn find_ultimate_performance_plan(plans: &[PowerPlan]) -> Option<&PowerPlan> {
    plans
        .iter()
        .find(|plan| plan.name.eq_ignore_ascii_case("Ultimate Performance"))
}

fn default_available_guid(available_plans: &[PowerPlan]) -> String {
    available_plans
        .first()
        .map(|plan| plan.guid.clone())
        .unwrap_or_default()
}

fn select_valid_guid(configured: &str, available_plans: &[PowerPlan], fallback: &str) -> String {
    if !configured.is_empty() && available_plans.iter().any(|plan| plan.guid == configured) {
        configured.to_string()
    } else {
        fallback.to_string()
    }
}

pub(crate) fn initialize_plan_selection(
    config: &mut Config,
    available_plans: &[PowerPlan],
    active_plan: Option<&PowerPlan>,
    is_first_run: bool,
) {
    let active_guid = active_plan.map_or_else(
        || default_available_guid(available_plans),
        |plan| plan.guid.clone(),
    );
    let discovered_low_power =
        discover_plan_guid_by_name(available_plans, &["power saver", "power save"]);
    let discovered_performance = discover_plan_guid_by_name(
        available_plans,
        &["ultimate performance", "high performance"],
    );

    if is_first_run {
        config.general.standard_plan_guid.clone_from(&active_guid);
        config.general.low_power_plan_guid =
            discovered_low_power.unwrap_or_else(|| config.general.standard_plan_guid.clone());
        config.general.performance_plan_guid =
            discovered_performance.unwrap_or_else(|| config.general.standard_plan_guid.clone());
        return;
    }

    config.general.standard_plan_guid = select_valid_guid(
        &config.general.standard_plan_guid,
        available_plans,
        &active_guid,
    );
    config.general.low_power_plan_guid = select_valid_guid(
        &config.general.low_power_plan_guid,
        available_plans,
        &discovered_low_power.unwrap_or_else(|| config.general.standard_plan_guid.clone()),
    );
    config.general.performance_plan_guid = select_valid_guid(
        &config.general.performance_plan_guid,
        available_plans,
        &discovered_performance.unwrap_or_else(|| config.general.standard_plan_guid.clone()),
    );
}

impl GeneralConfig {
    pub(crate) fn standard_processor_preset(
        &self,
        topology: CpuTopologyKind,
    ) -> ProcessorPresetRecommendation {
        processor_preset(
            self.standard_cpu_min_percent,
            self.standard_cpu_max_percent,
            Some(self.standard_boost_mode),
            self.standard_core_parking_min_cores_percent,
            topology,
        )
    }

    pub(crate) fn high_performance_recommendation(
        &self,
        topology: CpuTopologyKind,
    ) -> ProcessorPresetRecommendation {
        ProcessorPresetRecommendation {
            latency_hint_perf: Some(100),
            idle_promote_threshold: Some(0),
            ..processor_preset(
                self.performance_cpu_min_percent,
                self.performance_cpu_max_percent,
                Some(self.performance_boost_mode),
                Some(self.performance_core_parking_min_cores_percent),
                topology,
            )
        }
    }

    pub(crate) fn low_power_processor_preset(
        &self,
        topology: CpuTopologyKind,
    ) -> ProcessorPresetRecommendation {
        processor_preset(
            self.low_power_cpu_min_percent,
            self.low_power_cpu_max_percent,
            Some(self.low_power_boost_mode),
            self.low_power_core_parking_min_cores_percent,
            topology,
        )
    }

    pub(crate) fn energy_rate(&self) -> EnergyRate {
        EnergyRate {
            dollars_per_kwh: self.energy_rate_dollars_per_kwh.max(0.0),
            source_label: if self.energy_rate_source_label.trim().is_empty() {
                default_energy_rate_source_label()
            } else {
                self.energy_rate_source_label.clone()
            },
        }
    }

    pub(crate) fn cpu_power_profile(&self) -> CpuPowerProfile {
        let idle = self.cpu_idle_watts.max(0.0);
        let base = self.cpu_base_watts.max(idle);
        let turbo = self.cpu_turbo_watts.max(base);
        CpuPowerProfile {
            idle_watts: idle,
            base_watts: base,
            turbo_watts: turbo,
            source_label: if self.cpu_power_source_label.trim().is_empty() {
                default_cpu_power_source_label()
            } else {
                self.cpu_power_source_label.clone()
            },
        }
    }
}

fn processor_preset(
    min_percent: u8,
    max_percent: u8,
    boost_mode: Option<u8>,
    core_parking_min_cores_percent: Option<u8>,
    topology: CpuTopologyKind,
) -> ProcessorPresetRecommendation {
    ProcessorPresetRecommendation {
        min_percent: min_percent as u32,
        max_percent: max_percent as u32,
        boost_mode: boost_mode.map(u32::from),
        core_parking_min_cores_percent: core_parking_min_cores_percent.map(u32::from),
        latency_hint_perf: None,
        idle_promote_threshold: None,
        class1: (topology == CpuTopologyKind::Hybrid).then_some(ProcessorClassRecommendation {
            min_percent: min_percent as u32,
            max_percent: max_percent as u32,
            core_parking_min_cores_percent: core_parking_min_cores_percent.map(u32::from),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::types::PowerPlan;

    fn plan(guid: &str, name: &str) -> PowerPlan {
        PowerPlan {
            guid: guid.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn test_default_config_values() {
        let c = Config::default();
        assert_eq!(c.general.poll_interval_ms, 1_000);
        assert_eq!(c.general.hold_performance_seconds, 25);
        assert!(c.general.standard_plan_guid.is_empty());
        assert!(c.general.low_power_plan_guid.is_empty());
        assert!(c.general.performance_plan_guid.is_empty());
        let standard = c
            .general
            .standard_processor_preset(CpuTopologyKind::Homogeneous);
        assert_eq!((standard.min_percent, standard.max_percent), (5, 99));
        let performance = c
            .general
            .high_performance_recommendation(CpuTopologyKind::Homogeneous);
        assert_eq!(
            (performance.min_percent, performance.max_percent),
            (100, 100)
        );
        assert_eq!(c.general.performance_boost_mode, 2);
        assert_eq!(c.general.performance_core_parking_min_cores_percent, 100);
        assert_eq!(c.general.standard_boost_mode, 1);
        assert_eq!(c.general.standard_core_parking_min_cores_percent, None);
        assert_eq!(c.general.low_power_boost_mode, 0);
        assert_eq!(c.general.low_power_core_parking_min_cores_percent, Some(0));
        let low_power = c
            .general
            .low_power_processor_preset(CpuTopologyKind::Homogeneous);
        assert_eq!((low_power.min_percent, low_power.max_percent), (0, 20));
        assert_eq!(c.general.idle_wait_seconds, 600);
        assert_eq!(c.general.cpu_average_threshold_percent, 10);
        assert_eq!(c.general.cpu_average_window_seconds, 60);
        assert!(c.general.turbo_rescue_enabled);
        assert_eq!(c.general.turbo_rescue_cpu_threshold_percent, 10);
        assert_eq!(c.general.turbo_rescue_window_seconds, 15);
        assert_eq!(c.general.usage_trend_window_minutes, 15);
        assert_eq!(
            c.general.plan_time_range_mode,
            PlanTimeRangeMode::MatchUsageTrend
        );
        assert_eq!(c.general.appearance_mode, AppearanceMode::System);
        assert_eq!(
            c.general.power_usage_range_mode,
            PowerUsageRangeMode::RecentMinutes
        );
        assert!(!c.general.promote_on_battery);
        assert!(c.watchlist.processes.is_empty());
        assert!(!c.autostart.registered);
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let mut c = Config::default();
        c.watchlist.processes = vec!["cmake.exe".to_string(), "msbuild.exe".to_string()];
        c.general.hold_performance_seconds = 30;
        c.general.standard_plan_guid = "standard-guid".into();
        c.general.low_power_plan_guid = "low-guid".into();
        c.general.performance_plan_guid = "perf-guid".into();
        let text = toml::to_string_pretty(&c).unwrap();
        let c2: Config = toml::from_str(&text).unwrap();
        assert_eq!(c2.watchlist.processes, c.watchlist.processes);
        assert_eq!(c2.general.hold_performance_seconds, 30);
        assert_eq!(c2.general.standard_plan_guid, "standard-guid");
        assert_eq!(c2.general.low_power_plan_guid, "low-guid");
        assert_eq!(c2.general.performance_plan_guid, "perf-guid");
    }

    #[test]
    fn test_missing_file_returns_first_run() {
        // Write a valid config, then load it — confirms load_or_default reads files correctly.
        // Testing the "no file → first_run=true" path requires controlling config_path(),
        // which isn't injectable here. We test the "file exists" path instead.
        let dir = std::env::temp_dir().join("powerplanner_test_config");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let c = Config::default();
        let text = toml::to_string_pretty(&c).unwrap();
        std::fs::write(&path, text).unwrap();
        // Confirm the TOML round-trips to the same defaults
        let loaded: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.general.poll_interval_ms, 1_000);
        assert!(loaded.watchlist.processes.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_legacy_idle_plan_guid_migrates_to_standard() {
        let text = r#"
[general]
poll_interval_ms = 500
hold_performance_seconds = 25
idle_plan_guid = "legacy-balanced"
performance_plan_guid = "legacy-perf"
promote_on_battery = false
show_tray_balloon_on_switch = true

[autostart]
registered = false

[watchlist]
processes = []
"#;

        let c: Config = toml::from_str(text).unwrap();

        assert_eq!(c.general.standard_plan_guid, "legacy-balanced");
        assert!(c.general.low_power_plan_guid.is_empty());
        assert_eq!(c.general.performance_plan_guid, "legacy-perf");
    }

    #[test]
    fn test_legacy_idle_wait_minutes_migrates_to_seconds() {
        let text = r#"
[general]
poll_interval_ms = 500
hold_performance_seconds = 25
standard_plan_guid = "standard-guid"
low_power_plan_guid = "low-guid"
performance_plan_guid = "perf-guid"
idle_wait_minutes = 10
low_power_cpu_threshold_percent = 10
low_power_cpu_quiet_window_seconds = 60
usage_trend_window_minutes = 90
plan_time_range_mode = "all_retained"
        appearance_mode = "light"
        power_usage_range_mode = "all_retained"
        promote_on_battery = false
show_tray_balloon_on_switch = true

[autostart]
registered = false

[watchlist]
processes = []
"#;

        let mut c: Config = toml::from_str(text).unwrap();
        migrate_legacy_idle_wait(text, &mut c);

        assert_eq!(c.general.idle_wait_seconds, 600);
        assert_eq!(c.general.usage_trend_window_minutes, 90);
        assert_eq!(c.general.cpu_average_threshold_percent, 10);
        assert_eq!(c.general.cpu_average_window_seconds, 60);
        assert_eq!(
            c.general.plan_time_range_mode,
            PlanTimeRangeMode::AllRetained
        );
        assert_eq!(c.general.appearance_mode, AppearanceMode::Light);
        assert_eq!(
            c.general.power_usage_range_mode,
            PowerUsageRangeMode::AllRetained
        );
    }

    #[test]
    fn test_legacy_low_power_cpu_fields_load_as_shared_cpu_average_fields() {
        let text = r#"
[general]
poll_interval_ms = 500
hold_performance_seconds = 25
standard_plan_guid = "standard-guid"
low_power_plan_guid = "low-guid"
performance_plan_guid = "perf-guid"
idle_wait_seconds = 600
low_power_cpu_threshold_percent = 14
low_power_cpu_quiet_window_seconds = 75
usage_trend_window_minutes = 30
plan_time_range_mode = "match_usage_trend"
appearance_mode = "dark"
promote_on_battery = false
show_tray_balloon_on_switch = true

[autostart]
registered = false

[watchlist]
processes = []
"#;

        let c: Config = toml::from_str(text).unwrap();

        assert_eq!(c.general.cpu_average_threshold_percent, 14);
        assert_eq!(c.general.cpu_average_window_seconds, 75);
        assert!(c.general.turbo_rescue_enabled);
    }

    #[test]
    fn test_shared_cpu_average_fields_serialize_with_new_names() {
        let mut c = Config::default();
        c.general.cpu_average_threshold_percent = 18;
        c.general.cpu_average_window_seconds = 90;

        let text = toml::to_string_pretty(&c).unwrap();

        assert!(text.contains("cpu_average_threshold_percent = 18"));
        assert!(text.contains("cpu_average_window_seconds = 90"));
        assert!(!text.contains("low_power_cpu_threshold_percent"));
        assert!(!text.contains("low_power_cpu_quiet_window_seconds"));
    }

    #[test]
    fn test_invalid_dashboard_preferences_fall_back_to_defaults() {
        let text = r#"
[general]
poll_interval_ms = 500
hold_performance_seconds = 25
standard_plan_guid = "standard-guid"
low_power_plan_guid = "low-guid"
performance_plan_guid = "perf-guid"
idle_wait_seconds = 600
low_power_cpu_threshold_percent = 10
low_power_cpu_quiet_window_seconds = 60
usage_trend_window_minutes = 17
plan_time_range_mode = "lifetime"
        appearance_mode = "sepia"
        power_usage_range_mode = "lifetime"
        promote_on_battery = false
show_tray_balloon_on_switch = true

[autostart]
registered = false

[watchlist]
processes = []
"#;

        let c: Config = toml::from_str(text).unwrap();

        assert_eq!(c.general.usage_trend_window_minutes, 15);
        assert_eq!(
            c.general.plan_time_range_mode,
            PlanTimeRangeMode::MatchUsageTrend
        );
        assert_eq!(c.general.appearance_mode, AppearanceMode::System);
        assert_eq!(
            c.general.power_usage_range_mode,
            PowerUsageRangeMode::RecentMinutes
        );
    }

    #[test]
    fn test_default_energy_estimate_values_are_enabled_and_manual() {
        let c = Config::default();

        assert!(c.general.energy_estimates_enabled);
        assert_eq!(c.general.energy_rate_dollars_per_kwh, 0.15);
        assert_eq!(c.general.energy_rate_source_label, "Manual");
        assert_eq!(c.general.cpu_idle_watts, 12.0);
        assert_eq!(c.general.cpu_base_watts, 65.0);
        assert_eq!(c.general.cpu_turbo_watts, 125.0);
        assert_eq!(
            c.general.cpu_power_source_label,
            "Estimated from CPU profile"
        );
    }

    #[test]
    fn test_energy_estimate_values_roundtrip_through_toml() {
        let mut c = Config::default();
        c.general.energy_estimates_enabled = false;
        c.general.energy_rate_dollars_per_kwh = 0.23;
        c.general.energy_rate_source_label = "Kansas City estimate".into();
        c.general.cpu_idle_watts = 10.0;
        c.general.cpu_base_watts = 72.0;
        c.general.cpu_turbo_watts = 148.0;
        c.general.cpu_power_source_label = "Manual CPU profile".into();

        let text = toml::to_string_pretty(&c).unwrap();
        let loaded: Config = toml::from_str(&text).unwrap();

        assert!(!loaded.general.energy_estimates_enabled);
        assert_eq!(loaded.general.energy_rate_dollars_per_kwh, 0.23);
        assert_eq!(
            loaded.general.energy_rate_source_label,
            "Kansas City estimate"
        );
        assert_eq!(loaded.general.cpu_idle_watts, 10.0);
        assert_eq!(loaded.general.cpu_base_watts, 72.0);
        assert_eq!(loaded.general.cpu_turbo_watts, 148.0);
        assert_eq!(loaded.general.cpu_power_source_label, "Manual CPU profile");
    }

    #[test]
    fn test_initialize_plan_selection_uses_runtime_discovery() {
        let plans = vec![
            plan("balanced-guid", "Balanced"),
            plan("powersaver-guid", "Power Saver"),
            plan("perf-guid", "High Performance"),
        ];
        let active = plan("balanced-guid", "Balanced");
        let mut c = Config::default();

        initialize_plan_selection(&mut c, &plans, Some(&active), true);

        assert_eq!(c.general.standard_plan_guid, "balanced-guid");
        assert_eq!(c.general.low_power_plan_guid, "powersaver-guid");
        assert_eq!(c.general.performance_plan_guid, "perf-guid");
    }

    #[test]
    fn test_initialize_plan_selection_prefers_ultimate_performance() {
        let plans = vec![
            plan("balanced-guid", "Balanced"),
            plan("perf-guid", "High Performance"),
            plan("ultimate-guid", "Ultimate Performance"),
        ];
        let active = plan("balanced-guid", "Balanced");
        let mut c = Config::default();

        initialize_plan_selection(&mut c, &plans, Some(&active), true);

        assert_eq!(c.general.performance_plan_guid, "ultimate-guid");
        assert_eq!(
            find_ultimate_performance_plan(&plans).map(|plan| plan.guid.as_str()),
            Some("ultimate-guid")
        );
    }

    #[test]
    fn test_legacy_config_defaults_advanced_processor_recommendations() {
        let c = Config::default();
        let mut value = toml::Value::try_from(&c).unwrap();
        let general = value
            .get_mut("general")
            .and_then(toml::Value::as_table_mut)
            .unwrap();
        general.remove("performance_boost_mode");
        general.remove("performance_core_parking_min_cores_percent");
        general.remove("standard_boost_mode");
        general.remove("standard_core_parking_min_cores_percent");
        general.remove("low_power_boost_mode");
        general.remove("low_power_core_parking_min_cores_percent");

        let loaded: Config = value.try_into().unwrap();

        assert_eq!(loaded.general.performance_boost_mode, 2);
        assert_eq!(
            loaded.general.performance_core_parking_min_cores_percent,
            100
        );
        assert_eq!(loaded.general.standard_boost_mode, 1);
        assert_eq!(loaded.general.standard_core_parking_min_cores_percent, None);
        assert_eq!(loaded.general.low_power_boost_mode, 0);
        assert_eq!(
            loaded.general.low_power_core_parking_min_cores_percent,
            Some(0)
        );
    }

    #[test]
    fn test_processor_presets_add_class1_only_for_confirmed_hybrid_cpu() {
        let c = Config::default();

        let homogeneous = c
            .general
            .low_power_processor_preset(CpuTopologyKind::Homogeneous);
        assert!(homogeneous.class1.is_none());

        let hybrid = c
            .general
            .low_power_processor_preset(CpuTopologyKind::Hybrid);
        let class1 = hybrid.class1.unwrap();
        assert_eq!(class1.min_percent, 0);
        assert_eq!(class1.max_percent, 20);
        assert_eq!(class1.core_parking_min_cores_percent, Some(0));

        let standard = c.general.standard_processor_preset(CpuTopologyKind::Hybrid);
        assert_eq!(standard.core_parking_min_cores_percent, None);
        assert_eq!(
            standard.class1.unwrap().core_parking_min_cores_percent,
            None
        );
    }

    #[test]
    fn test_initialize_plan_selection_preserves_valid_saved_guids() {
        let plans = vec![
            plan("balanced-guid", "Balanced"),
            plan("powersaver-guid", "Power Saver"),
            plan("perf-guid", "High Performance"),
        ];
        let active = plan("balanced-guid", "Balanced");
        let mut c = Config::default();
        c.general.standard_plan_guid = "balanced-guid".into();
        c.general.low_power_plan_guid = "powersaver-guid".into();
        c.general.performance_plan_guid = "perf-guid".into();

        initialize_plan_selection(&mut c, &plans, Some(&active), false);

        assert_eq!(c.general.standard_plan_guid, "balanced-guid");
        assert_eq!(c.general.low_power_plan_guid, "powersaver-guid");
        assert_eq!(c.general.performance_plan_guid, "perf-guid");
    }
}
