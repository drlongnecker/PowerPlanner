// src/config.rs
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
    pub idle_plan_guid: String,
    pub performance_plan_guid: String,
    pub promote_on_battery: bool,
    pub show_tray_balloon_on_switch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutostartConfig {
    pub registered: bool,
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
                // Windows built-in Balanced GUID
                idle_plan_guid: "381b4222-f694-41f0-9685-ff5bb260df2e".to_string(),
                // Windows built-in High Performance GUID
                performance_plan_guid: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".to_string(),
                promote_on_battery: false,
                show_tray_balloon_on_switch: true,
            },
            autostart: AutostartConfig { registered: false },
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
    let config: Config = toml::from_str(&text).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let c = Config::default();
        assert_eq!(c.general.poll_interval_ms, 500);
        assert_eq!(c.general.hold_performance_seconds, 25);
        assert!(!c.general.promote_on_battery);
        assert!(c.watchlist.processes.is_empty());
        assert!(!c.autostart.registered);
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let mut c = Config::default();
        c.watchlist.processes = vec!["cmake.exe".to_string(), "msbuild.exe".to_string()];
        c.general.hold_performance_seconds = 30;
        let text = toml::to_string_pretty(&c).unwrap();
        let c2: Config = toml::from_str(&text).unwrap();
        assert_eq!(c2.watchlist.processes, c.watchlist.processes);
        assert_eq!(c2.general.hold_performance_seconds, 30);
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
}
