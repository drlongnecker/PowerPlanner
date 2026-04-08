// src/monitor.rs
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant};
use crate::config::Config;
use crate::db;
use crate::power::PowerApi;
use crate::types::{AppState, MonitorCommand, PowerEvent, PowerPlan, RunningProcess};

pub struct MonitorState {
    pub config: Config,
    pub last_match_at: Option<Instant>,
    pub current_plan_guid: String,
}

impl MonitorState {
    pub fn new(config: Config, initial_guid: String) -> Self {
        Self { config, last_match_at: None, current_plan_guid: initial_guid }
    }

    /// Pure decision function: given current conditions, return the target plan GUID.
    pub fn decide_plan(&self, has_match: bool, on_battery: bool, now: Instant) -> String {
        let suppress = on_battery && !self.config.general.promote_on_battery;

        if !suppress && has_match {
            return self.config.general.performance_plan_guid.clone();
        }

        if !suppress {
            if let Some(last) = self.last_match_at {
                let hold = Duration::from_secs(self.config.general.hold_performance_seconds);
                if now.duration_since(last) < hold {
                    return self.config.general.performance_plan_guid.clone();
                }
            }
        }

        self.config.general.idle_plan_guid.clone()
    }
}

pub fn run(
    config: Config,
    app_state: Arc<RwLock<AppState>>,
    rx: mpsc::Receiver<MonitorCommand>,
    db_conn: rusqlite::Connection,
    power: Arc<dyn PowerApi>,
) {
    let initial_guid = power.get_active_plan()
        .map(|p| p.guid)
        .unwrap_or_else(|_| config.general.idle_plan_guid.clone());

    let mut state = MonitorState::new(config, initial_guid);
    let mut last_sanity = Instant::now();

    loop {
        // Drain commands before each tick
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                MonitorCommand::Stop => return,
                MonitorCommand::SwitchPlan(guid) => {
                    if power.set_active_plan(&guid).is_ok() {
                        state.current_plan_guid = guid;
                    }
                }
                MonitorCommand::UpdateWatchlist(list) => {
                    state.config.watchlist.processes = list;
                }
                MonitorCommand::UpdateConfig(cfg) => {
                    state.config = cfg;
                }
            }
        }

        let poll = Duration::from_millis(state.config.general.poll_interval_ms);
        let now = Instant::now();

        // Enumerate processes
        let running = get_running_processes();
        let watchlist_lower: Vec<String> = state.config.watchlist.processes
            .iter().map(|s| s.to_lowercase()).collect();
        // running is already deduplicated by name (one entry per unique process name)
        let matched: Vec<String> = running.iter()
            .filter(|p| watchlist_lower.contains(&p.name.to_lowercase()))
            .map(|p| p.name.clone())
            .collect();
        let has_match = !matched.is_empty();

        if has_match {
            state.last_match_at = Some(now);
        }

        let battery = power.get_battery_status().unwrap_or_default();
        let target_guid = state.decide_plan(has_match, battery.on_battery, now);

        // Switch if the target differs from current
        if target_guid != state.current_plan_guid {
            let trigger = if has_match {
                matched.join(", ")
            } else if state.last_match_at.is_some() {
                "hold expired".to_string()
            } else {
                "idle".to_string()
            };

            if power.set_active_plan(&target_guid).is_ok() {
                let plan_name = app_state.read().unwrap().available_plans.iter()
                    .find(|p| p.guid == target_guid)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| target_guid.clone());

                let event = PowerEvent {
                    ts: chrono::Local::now(),
                    plan_guid: target_guid.clone(),
                    plan_name: plan_name.clone(),
                    trigger,
                    on_battery: battery.on_battery,
                    battery_pct: battery.percent,
                };
                let _ = db::insert_event(&db_conn, &event);

                let mut s = app_state.write().unwrap();
                s.push_event(event);
                state.current_plan_guid = target_guid.clone();
            } else {
                app_state.write().unwrap().last_error =
                    Some(format!("Failed to switch to plan {}", target_guid));
            }
        }

        // Sanity check every 10 seconds
        if last_sanity.elapsed() >= Duration::from_secs(10) {
            if let Ok(actual) = power.get_active_plan() {
                if actual.guid != state.current_plan_guid {
                    log::info!(
                        "External plan change detected: was '{}', now '{}'",
                        state.current_plan_guid, actual.guid
                    );
                    state.current_plan_guid = actual.guid;
                }
            }
            last_sanity = Instant::now();
        }

        // Update shared AppState for UI
        {
            let hold_remaining = state.last_match_at.map(|t| {
                let hold = state.config.general.hold_performance_seconds as f32;
                let elapsed = now.duration_since(t).as_secs_f32();
                (hold - elapsed).max(0.0)
            }).filter(|&r| r > 0.0);

            let available = app_state.read().unwrap().available_plans.clone();
            let current_plan = available.iter()
                .find(|p| p.guid == state.current_plan_guid)
                .cloned()
                .or_else(|| Some(PowerPlan {
                    guid: state.current_plan_guid.clone(),
                    name: state.current_plan_guid.clone(),
                }));

            let mut s = app_state.write().unwrap();
            s.current_plan = current_plan;
            s.matched_processes = matched;
            s.all_running_processes = running;
            s.hold_remaining_secs = hold_remaining;
            s.battery = battery;
            s.monitor_running = true;
        }

        std::thread::sleep(poll);
    }
}

fn get_running_processes() -> Vec<RunningProcess> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes();
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<RunningProcess> = sys.processes().values()
        .filter_map(|p| {
            let name = p.name().to_string();
            if seen.insert(name.to_lowercase()) {
                let path = p.exe().and_then(|e| e.to_str()).map(|s| s.to_string());
                Some(RunningProcess { name, path })
            } else {
                None
            }
        })
        .collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::time::{Duration, Instant};

    fn test_config() -> Config {
        let mut c = Config::default();
        c.general.idle_plan_guid = "idle-guid".into();
        c.general.performance_plan_guid = "perf-guid".into();
        c.general.hold_performance_seconds = 10;
        c.general.promote_on_battery = false;
        c
    }

    #[test]
    fn test_match_triggers_performance() {
        let s = MonitorState::new(test_config(), "idle-guid".into());
        assert_eq!(s.decide_plan(true, false, Instant::now()), "perf-guid");
    }

    #[test]
    fn test_no_match_no_history_returns_idle() {
        let s = MonitorState::new(test_config(), "idle-guid".into());
        assert_eq!(s.decide_plan(false, false, Instant::now()), "idle-guid");
    }

    #[test]
    fn test_within_hold_period_stays_performance() {
        let mut s = MonitorState::new(test_config(), "perf-guid".into());
        let base = Instant::now();
        s.last_match_at = Some(base);
        // 5 seconds later — within 10s hold
        assert_eq!(s.decide_plan(false, false, base + Duration::from_secs(5)), "perf-guid");
    }

    #[test]
    fn test_after_hold_expires_returns_idle() {
        let mut s = MonitorState::new(test_config(), "perf-guid".into());
        let base = Instant::now();
        s.last_match_at = Some(base);
        // 15 seconds later — past 10s hold
        assert_eq!(s.decide_plan(false, false, base + Duration::from_secs(15)), "idle-guid");
    }

    #[test]
    fn test_on_battery_suppresses_promotion() {
        let s = MonitorState::new(test_config(), "idle-guid".into());
        // on_battery = true, promote_on_battery = false → stay idle even with match
        assert_eq!(s.decide_plan(true, true, Instant::now()), "idle-guid");
    }

    #[test]
    fn test_battery_bypass_allows_promotion() {
        let mut cfg = test_config();
        cfg.general.promote_on_battery = true;
        let s = MonitorState::new(cfg, "idle-guid".into());
        assert_eq!(s.decide_plan(true, true, Instant::now()), "perf-guid");
    }
}
