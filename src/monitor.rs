// src/monitor.rs
use crate::config::Config;
use crate::db;
use crate::idle::{IdleReader, WindowsIdleReader};
use crate::power::PowerApi;
use crate::types::{AppState, MonitorCommand, PowerEvent, PowerPlan, RunningProcess};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use sysinfo::System as SysInfo;

#[derive(Debug, Clone)]
struct CpuSample {
    at: Instant,
    usage_percent: f32,
}

pub struct MonitorState {
    pub config: Config,
    pub last_match_at: Option<Instant>,
    pub current_plan_guid: String,
    pub forced_plan_guid: Option<String>,
    pub available_plans: Vec<PowerPlan>,
    watchlist_lower: Vec<String>,
    cpu_samples: VecDeque<CpuSample>,
    sys: SysInfo,
}

impl MonitorState {
    pub fn new(config: Config, initial_guid: String, available_plans: Vec<PowerPlan>) -> Self {
        let watchlist_lower = config
            .watchlist
            .processes
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        Self {
            config,
            last_match_at: None,
            current_plan_guid: initial_guid,
            forced_plan_guid: None,
            available_plans,
            watchlist_lower,
            cpu_samples: VecDeque::new(),
            sys: SysInfo::new(),
        }
    }

    fn rebuild_watchlist_lower(&mut self) {
        self.watchlist_lower = self
            .config
            .watchlist
            .processes
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
    }

    fn record_cpu_sample(&mut self, now: Instant, usage_percent: f32) {
        self.cpu_samples.push_back(CpuSample {
            at: now,
            usage_percent,
        });
        let quiet_window =
            Duration::from_secs(self.config.general.low_power_cpu_quiet_window_seconds);
        while let Some(front) = self.cpu_samples.front() {
            if now.duration_since(front.at) > quiet_window {
                self.cpu_samples.pop_front();
            } else {
                break;
            }
        }
    }

    fn cpu_is_quiet(&self, now: Instant) -> bool {
        let _ = now;
        self.cpu_average_percent()
            .map(|average| average <= self.config.general.low_power_cpu_threshold_percent as f32)
            .unwrap_or(false)
    }

    fn cpu_average_percent(&self) -> Option<f32> {
        if self.cpu_samples.is_empty() {
            return None;
        }

        if self.cpu_samples.len() < 2 {
            return None;
        }

        let earliest = self.cpu_samples.front().unwrap().at;
        let latest = self.cpu_samples.back().unwrap().at;
        if latest <= earliest {
            return None;
        }

        let total: f32 = self
            .cpu_samples
            .iter()
            .map(|sample| sample.usage_percent)
            .sum();
        Some(total / self.cpu_samples.len() as f32)
    }

    fn input_is_idle_enough(&self, idle_for: Duration) -> bool {
        idle_for >= Duration::from_secs(self.config.general.idle_wait_seconds)
    }

    /// Pure decision function: given current conditions, return the target plan GUID.
    pub fn decide_plan(
        &self,
        has_match: bool,
        on_battery: bool,
        now: Instant,
        idle_for: Duration,
    ) -> String {
        // A forced plan overrides all automatic logic.
        if let Some(ref guid) = self.forced_plan_guid {
            return guid.clone();
        }

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

        if self.input_is_idle_enough(idle_for) && self.cpu_is_quiet(now) {
            return self.config.general.low_power_plan_guid.clone();
        }

        self.config.general.standard_plan_guid.clone()
    }
}

pub fn run(
    config: Config,
    app_state: Arc<RwLock<AppState>>,
    rx: mpsc::Receiver<MonitorCommand>,
    db_conn: rusqlite::Connection,
    power: Arc<dyn PowerApi>,
    repaint_ctx: Arc<OnceLock<egui::Context>>,
) {
    let idle_reader = WindowsIdleReader;
    let initial_guid = power
        .get_active_plan()
        .map(|p| p.guid)
        .unwrap_or_else(|_| config.general.standard_plan_guid.clone());

    let available_plans = app_state.read().unwrap().available_plans.clone();
    let mut state = MonitorState::new(config, initial_guid, available_plans);
    let mut last_sanity = Instant::now();

    loop {
        // Drain commands before each tick
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                MonitorCommand::Stop => return,
                MonitorCommand::ForcePlan(Some(guid)) => {
                    if power.set_active_plan(&guid).is_ok() {
                        let plan_name = state
                            .available_plans
                            .iter()
                            .find(|p| p.guid == guid)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| guid.clone());
                        let (bat_on, bat_pct) = {
                            let s = app_state.read().unwrap();
                            (s.battery.on_battery, s.battery.percent)
                        };
                        let event = PowerEvent {
                            ts: chrono::Local::now(),
                            plan_guid: guid.clone(),
                            plan_name,
                            trigger: "manual".into(),
                            on_battery: bat_on,
                            battery_pct: bat_pct,
                        };
                        let _ = db::insert_event(&db_conn, &event);
                        app_state.write().unwrap().push_event(event);
                        state.forced_plan_guid = Some(guid.clone());
                        state.current_plan_guid = guid;
                    }
                }
                MonitorCommand::ForcePlan(None) => {
                    state.forced_plan_guid = None;
                }
                MonitorCommand::UpdateWatchlist(list) => {
                    state.config.watchlist.processes = list;
                    state.rebuild_watchlist_lower();
                }
                MonitorCommand::UpdateConfig(cfg) => {
                    state.config = cfg;
                    state.rebuild_watchlist_lower();
                }
                MonitorCommand::RefreshPlans => {
                    if let Ok(plans) = power.enumerate_plans() {
                        state.available_plans = plans.clone();
                        app_state.write().unwrap().available_plans = plans;
                    }
                }
            }
        }

        let poll = Duration::from_millis(state.config.general.poll_interval_ms);
        let now = Instant::now();

        state.sys.refresh_cpu();
        let cpu_usage = state.sys.global_cpu_info().cpu_usage();
        state.record_cpu_sample(now, cpu_usage);

        // Enumerate processes
        let running = get_running_processes(&mut state.sys);
        // running is already deduplicated by name (one entry per unique process name)
        let matched: Vec<String> = running
            .iter()
            .filter(|p| state.watchlist_lower.contains(&p.name.to_lowercase()))
            .map(|p| p.name.clone())
            .collect();
        let has_match = !matched.is_empty();

        if has_match {
            state.last_match_at = Some(now);
        }

        let battery = power.get_battery_status().unwrap_or_default();
        let idle_for = idle_reader.idle_duration().unwrap_or(Duration::ZERO);
        let target_guid = state.decide_plan(has_match, battery.on_battery, now, idle_for);

        // Switch if the target differs from current
        if target_guid != state.current_plan_guid {
            let trigger = if has_match {
                matched.join(", ")
            } else if target_guid == state.config.general.low_power_plan_guid {
                "entered low power".to_string()
            } else if idle_for < Duration::from_secs(state.config.general.idle_wait_seconds) {
                "input resumed".to_string()
            } else if !state.cpu_is_quiet(now) {
                "cpu above threshold".to_string()
            } else if state.last_match_at.is_some() {
                "hold expired".to_string()
            } else {
                "standard".to_string()
            };

            if power.set_active_plan(&target_guid).is_ok() {
                let plan_name = state
                    .available_plans
                    .iter()
                    .find(|p| p.guid == target_guid)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| target_guid.clone());

                let event = PowerEvent {
                    ts: chrono::Local::now(),
                    plan_guid: target_guid.clone(),
                    plan_name,
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
                        state.current_plan_guid,
                        actual.guid
                    );
                    state.current_plan_guid = actual.guid;
                }
            }
            last_sanity = Instant::now();
        }

        // Update shared AppState for UI
        {
            let cpu_average_percent = state.cpu_average_percent();
            let low_power_ready_input = state.input_is_idle_enough(idle_for);
            let low_power_ready_cpu = state.cpu_is_quiet(now);
            let hold_remaining = state
                .last_match_at
                .map(|t| {
                    let hold = state.config.general.hold_performance_seconds as f32;
                    let elapsed = now.duration_since(t).as_secs_f32();
                    (hold - elapsed).max(0.0)
                })
                .filter(|&r| r > 0.0);

            let current_plan = state
                .available_plans
                .iter()
                .find(|p| p.guid == state.current_plan_guid)
                .cloned()
                .or_else(|| {
                    Some(PowerPlan {
                        guid: state.current_plan_guid.clone(),
                        name: state.current_plan_guid.clone(),
                    })
                });

            let mut s = app_state.write().unwrap();
            s.current_plan = current_plan;
            s.matched_processes = matched;
            s.all_running_processes = running;
            s.hold_remaining_secs = hold_remaining;
            s.idle_for_secs = Some(idle_for.as_secs_f32());
            s.cpu_average_percent = cpu_average_percent;
            s.low_power_ready_input = low_power_ready_input;
            s.low_power_ready_cpu = low_power_ready_cpu;
            s.battery = battery;
            s.monitor_running = true;
            s.forced_plan = state.forced_plan_guid.as_ref().map(|guid| {
                state
                    .available_plans
                    .iter()
                    .find(|p| p.guid == *guid)
                    .cloned()
                    .unwrap_or_else(|| PowerPlan {
                        guid: guid.clone(),
                        name: guid.clone(),
                    })
            });
        }

        if let Some(ctx) = repaint_ctx.get() {
            ctx.request_repaint();
        }

        std::thread::sleep(poll);
    }
}

fn get_running_processes(sys: &mut SysInfo) -> Vec<RunningProcess> {
    sys.refresh_processes();
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<RunningProcess> = sys
        .processes()
        .values()
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
        c.general.standard_plan_guid = "standard-guid".into();
        c.general.low_power_plan_guid = "low-guid".into();
        c.general.performance_plan_guid = "perf-guid".into();
        c.general.hold_performance_seconds = 10;
        c.general.idle_wait_seconds = 600;
        c.general.low_power_cpu_threshold_percent = 10;
        c.general.low_power_cpu_quiet_window_seconds = 60;
        c.general.promote_on_battery = false;
        c
    }

    #[test]
    fn test_match_triggers_performance() {
        let s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        assert_eq!(
            s.decide_plan(true, false, Instant::now(), Duration::ZERO),
            "perf-guid"
        );
    }

    #[test]
    fn test_no_match_no_history_returns_idle() {
        let s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        assert_eq!(
            s.decide_plan(false, false, Instant::now(), Duration::ZERO),
            "standard-guid"
        );
    }

    #[test]
    fn test_within_hold_period_stays_performance() {
        let mut s = MonitorState::new(test_config(), "perf-guid".into(), vec![]);
        let base = Instant::now();
        s.last_match_at = Some(base);
        // 5 seconds later — within 10s hold
        assert_eq!(
            s.decide_plan(false, false, base + Duration::from_secs(5), Duration::ZERO),
            "perf-guid"
        );
    }

    #[test]
    fn test_after_hold_expires_returns_idle() {
        let mut s = MonitorState::new(test_config(), "perf-guid".into(), vec![]);
        let base = Instant::now();
        s.last_match_at = Some(base);
        // 15 seconds later — past 10s hold
        assert_eq!(
            s.decide_plan(false, false, base + Duration::from_secs(15), Duration::ZERO),
            "standard-guid"
        );
    }

    #[test]
    fn test_on_battery_suppresses_promotion() {
        let s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        // on_battery = true, promote_on_battery = false → stay idle even with match
        assert_eq!(
            s.decide_plan(true, true, Instant::now(), Duration::ZERO),
            "standard-guid"
        );
    }

    #[test]
    fn test_battery_bypass_allows_promotion() {
        let mut cfg = test_config();
        cfg.general.promote_on_battery = true;
        let s = MonitorState::new(cfg, "standard-guid".into(), vec![]);
        assert_eq!(
            s.decide_plan(true, true, Instant::now(), Duration::ZERO),
            "perf-guid"
        );
    }

    #[test]
    fn test_user_idle_and_cpu_quiet_enter_low_power() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 4.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);

        assert_eq!(
            s.decide_plan(
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "low-guid"
        );
    }

    #[test]
    fn test_busy_cpu_blocks_low_power_even_when_input_is_idle() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 25.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 20.0);

        assert_eq!(
            s.decide_plan(
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "standard-guid"
        );
    }

    #[test]
    fn test_recent_input_blocks_low_power_even_when_cpu_is_quiet() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 3.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 5.0);

        assert_eq!(
            s.decide_plan(
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(120)
            ),
            "standard-guid"
        );
    }

    #[test]
    fn test_watched_process_still_wins_over_low_power() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 2.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 3.0);

        assert_eq!(
            s.decide_plan(
                true,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "perf-guid"
        );
    }

    #[test]
    fn test_cpu_average_requires_multiple_samples() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 4.0);
        assert_eq!(s.cpu_average_percent(), None);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);

        assert_eq!(s.cpu_average_percent(), Some(5.0));
    }

    #[test]
    fn test_input_idle_gate_uses_configured_seconds() {
        let s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);

        assert!(!s.input_is_idle_enough(Duration::from_secs(599)));
        assert!(s.input_is_idle_enough(Duration::from_secs(600)));
    }

    #[test]
    fn test_cpu_average_available_during_continuous_sampling() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        for second in 0..=90 {
            s.record_cpu_sample(base + Duration::from_secs(second), 5.0);
        }

        assert_eq!(s.cpu_average_percent(), Some(5.0));
        assert!(s.cpu_is_quiet(base + Duration::from_secs(90)));
    }
}
