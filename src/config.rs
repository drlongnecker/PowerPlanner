// src/config.rs
use crate::types::PowerPlan;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub autostart: AutostartConfig,
    pub watchlist: WatchlistConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub poll_interval_ms: u64,
    pub hold_performance_seconds: u64,
    #[serde(alias = "idle_plan_guid")]
    pub standard_plan_guid: String,
    #[serde(default)]
    pub low_power_plan_guid: String,
    pub performance_plan_guid: String,
    #[serde(default = "default_idle_wait_seconds")]
    pub idle_wait_seconds: u64,
    #[serde(default = "default_low_power_cpu_threshold_percent")]
    pub low_power_cpu_threshold_percent: u8,
    #[serde(default = "default_low_power_cpu_quiet_window_seconds")]
    pub low_power_cpu_quiet_window_seconds: u64,
    pub promote_on_battery: bool,
    pub show_tray_balloon_on_switch: bool,
}

fn default_idle_wait_seconds() -> u64 {
    600
}
fn default_low_power_cpu_threshold_percent() -> u8 {
    10
}
fn default_low_power_cpu_quiet_window_seconds() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutostartConfig {
    pub registered: bool,
    #[serde(skip)]
    pub is_elevated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistConfig {
    pub processes: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                poll_interval_ms: 500,
                hold_performance_seconds: 25,
                standard_plan_guid: String::new(),
                low_power_plan_guid: String::new(),
                performance_plan_guid: String::new(),
                idle_wait_seconds: default_idle_wait_seconds(),
                low_power_cpu_threshold_percent: default_low_power_cpu_threshold_percent(),
                low_power_cpu_quiet_window_seconds: default_low_power_cpu_quiet_window_seconds(),
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

pub fn config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PowerPlanner")
        .join("config.toml")
}

/// Returns (config, is_first_run).
pub fn load_or_default() -> (Config, bool) {
    let path = config_path();
    if !path.exists() {
        return (Config::default(), true);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut config: Config = toml::from_str(&text).unwrap_or_default();
    migrate_legacy_idle_wait(&text, &mut config);
    (config, false)
}

pub fn save(config: &Config) -> Result<()> {
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
        .and_then(|minutes| minutes.as_integer())
    else {
        return;
    };

    if minutes >= 0 {
        config.general.idle_wait_seconds = (minutes as u64).saturating_mul(60);
    }
}

#[cfg(test)]
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
        assert_eq!(c.general.poll_interval_ms, 500);
        assert_eq!(c.general.hold_performance_seconds, 25);
        assert!(c.general.standard_plan_guid.is_empty());
        assert!(c.general.low_power_plan_guid.is_empty());
        assert!(c.general.performance_plan_guid.is_empty());
        assert_eq!(c.general.idle_wait_seconds, 600);
        assert_eq!(c.general.low_power_cpu_threshold_percent, 10);
        assert_eq!(c.general.low_power_cpu_quiet_window_seconds, 60);
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
        assert_eq!(loaded.general.poll_interval_ms, 500);
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

fn discover_plan_guid_by_name(plans: &[PowerPlan], candidates: &[&str]) -> Option<String> {
    plans
        .iter()
        .find(|plan| {
            candidates.iter().any(|candidate| {
                plan.name.eq_ignore_ascii_case(candidate)
                    || plan
                        .name
                        .to_ascii_lowercase()
                        .contains(&candidate.to_ascii_lowercase())
            })
        })
        .map(|plan| plan.guid.clone())
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

pub fn initialize_plan_selection(
    config: &mut Config,
    available_plans: &[PowerPlan],
    active_plan: Option<&PowerPlan>,
    is_first_run: bool,
) {
    let active_guid = active_plan
        .map(|plan| plan.guid.clone())
        .unwrap_or_else(|| default_available_guid(available_plans));
    let discovered_low_power =
        discover_plan_guid_by_name(available_plans, &["power saver", "power save"]);
    let discovered_performance = discover_plan_guid_by_name(
        available_plans,
        &["high performance", "ultimate performance"],
    );

    if is_first_run {
        config.general.standard_plan_guid = active_guid.clone();
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
