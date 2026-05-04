// src/monitor.rs
use crate::config::Config;
use crate::db;
use crate::energy::{
    estimate_sample_energy, CpuPowerProvider, CpuPowerSample, EnergyRateProvider,
    ManualRateProvider, ModeledCpuPowerProvider,
};
use crate::idle::{IdleReader, WindowsIdleReader};
use crate::power::PowerApi;
use crate::types::{
    AppState, CpuFrequencySample, CpuHistoryEnergyEstimate, CpuHistoryPlanKind, CpuHistoryPoint,
    MonitorCommand, PowerEvent, PowerPlan, RunningProcess,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{mpsc, Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use sysinfo::System as SysInfo;

const DASHBOARD_CPU_HISTORY_WINDOW: Duration = Duration::from_secs(15 * 60);
const DASHBOARD_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct CpuSample {
    at: Instant,
    usage_percent: f32,
}

#[derive(Debug, Clone)]
struct DashboardCpuSample {
    at: Instant,
    point: CpuHistoryPoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlanDecision {
    guid: String,
    trigger: String,
}

pub struct MonitorState {
    pub config: Config,
    pub last_match_at: Option<Instant>,
    pub last_match_trigger: Option<String>,
    pub current_plan_guid: String,
    pub forced_plan_guid: Option<String>,
    pub available_plans: Vec<PowerPlan>,
    watchlist_lower: Vec<String>,
    low_power_busy_since: Option<Instant>,
    cpu_sampler_primed: bool,
    first_cpu_sample_at: Option<Instant>,
    cpu_samples: VecDeque<CpuSample>,
    dashboard_cpu_history: VecDeque<DashboardCpuSample>,
    last_dashboard_sample_at: Option<Instant>,
    turbo_rescue_since: Option<Instant>,
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
            last_match_trigger: None,
            current_plan_guid: initial_guid,
            forced_plan_guid: None,
            available_plans,
            watchlist_lower,
            low_power_busy_since: None,
            cpu_sampler_primed: false,
            first_cpu_sample_at: None,
            cpu_samples: VecDeque::new(),
            dashboard_cpu_history: VecDeque::new(),
            last_dashboard_sample_at: None,
            turbo_rescue_since: None,
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
        if self.first_cpu_sample_at.is_none() {
            self.first_cpu_sample_at = Some(now);
        }
        self.cpu_samples.push_back(CpuSample {
            at: now,
            usage_percent,
        });
        let quiet_window = Duration::from_secs(self.config.general.cpu_average_window_seconds);
        while let Some(front) = self.cpu_samples.front() {
            if now.duration_since(front.at) > quiet_window {
                self.cpu_samples.pop_front();
            } else {
                break;
            }
        }
    }

    fn record_cpu_observation(&mut self, now: Instant, usage_percent: f32) {
        if !self.cpu_sampler_primed {
            self.cpu_sampler_primed = true;
            return;
        }

        self.record_cpu_sample(now, usage_percent);
    }

    fn cpu_is_quiet(&self, now: Instant) -> bool {
        let _ = now;
        self.cpu_average_percent()
            .map(|average| average <= self.config.general.cpu_average_threshold_percent as f32)
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

    fn plan_kind_for_guid(&self, guid: &str) -> CpuHistoryPlanKind {
        if guid == self.config.general.low_power_plan_guid {
            CpuHistoryPlanKind::LowPower
        } else if guid == self.config.general.performance_plan_guid {
            CpuHistoryPlanKind::Performance
        } else {
            CpuHistoryPlanKind::Standard
        }
    }

    fn plan_name_for_guid(&self, guid: &str) -> String {
        self.available_plans
            .iter()
            .find(|plan| plan.guid == guid)
            .map(|plan| plan.name.clone())
            .unwrap_or_else(|| guid.to_string())
    }

    fn current_trigger_description(
        &self,
        matched: &[String],
        idle_for: Duration,
        now: Instant,
    ) -> String {
        if self.forced_plan_guid.is_some() {
            return "manual".to_string();
        }
        if !matched.is_empty() {
            return matched.join(", ");
        }
        if self.last_match_at.is_some()
            && self
                .last_match_at
                .map(|last| {
                    now.duration_since(last)
                        < Duration::from_secs(self.config.general.hold_performance_seconds)
                })
                .unwrap_or(false)
        {
            return self
                .last_match_trigger
                .as_ref()
                .map(|trigger| format!("{} (holding)", trigger))
                .unwrap_or_else(|| "hold timer".to_string());
        }
        if idle_for < Duration::from_secs(self.config.general.idle_wait_seconds) {
            return "input resumed".to_string();
        }
        if !self.cpu_is_quiet(now) {
            return "cpu above threshold".to_string();
        }
        if self.current_plan_guid == self.config.general.low_power_plan_guid {
            return "idle + quiet cpu".to_string();
        }
        "standard".to_string()
    }

    fn record_cpu_history(
        &mut self,
        now: Instant,
        trigger: &str,
        frequency: CpuFrequencySample,
        cpu_base_mhz: Option<u32>,
    ) -> Option<CpuHistoryPoint> {
        let Some(average_percent) = self.cpu_average_percent() else {
            return None;
        };

        if let Some(last_at) = self.last_dashboard_sample_at {
            if now.duration_since(last_at) < DASHBOARD_SAMPLE_INTERVAL {
                return None;
            }
        }

        self.last_dashboard_sample_at = Some(now);

        let point = CpuHistoryPoint {
            ts: chrono::Local::now(),
            average_percent,
            current_mhz: frequency.max_mhz,
            base_mhz: cpu_base_mhz,
            plan_kind: self.plan_kind_for_guid(&self.current_plan_guid),
            plan_name: self.plan_name_for_guid(&self.current_plan_guid),
            trigger: trigger.to_string(),
            energy: self.energy_estimate_for_sample(average_percent, frequency, cpu_base_mhz),
        };

        self.dashboard_cpu_history.push_back(DashboardCpuSample {
            at: now,
            point: point.clone(),
        });

        while let Some(front) = self.dashboard_cpu_history.front() {
            if now.duration_since(front.at) > DASHBOARD_CPU_HISTORY_WINDOW {
                self.dashboard_cpu_history.pop_front();
            } else {
                break;
            }
        }

        Some(point)
    }

    fn energy_estimate_for_sample(
        &self,
        cpu_average_percent: f32,
        frequency: CpuFrequencySample,
        cpu_base_mhz: Option<u32>,
    ) -> Option<CpuHistoryEnergyEstimate> {
        if !self.config.general.energy_estimates_enabled {
            return None;
        }

        let power_provider = ModeledCpuPowerProvider::new(self.config.general.cpu_power_profile());
        let rate_provider = ManualRateProvider::new(
            self.config.general.energy_rate().dollars_per_kwh,
            self.config.general.energy_rate().source_label,
        );
        let plan_kind = self.plan_kind_for_guid(&self.current_plan_guid);
        let sample = CpuPowerSample {
            cpu_average_percent,
            current_mhz: frequency.max_mhz,
            base_mhz: cpu_base_mhz,
            plan_kind,
        };
        let estimated_watts = power_provider.estimated_watts(sample);
        let baseline_watts = power_provider.estimated_watts(CpuPowerSample {
            cpu_average_percent,
            current_mhz: cpu_base_mhz.map(|base| base.saturating_add(101)),
            base_mhz: cpu_base_mhz,
            plan_kind: CpuHistoryPlanKind::Performance,
        });
        let estimate = estimate_sample_energy(
            estimated_watts,
            baseline_watts,
            DASHBOARD_SAMPLE_INTERVAL,
            rate_provider.current_rate(),
        );

        Some(CpuHistoryEnergyEstimate {
            estimated_watts,
            estimated_kwh: estimate.estimated_kwh,
            estimated_cost_usd: estimate.estimated_cost_usd,
            baseline_watts,
            baseline_cost_usd: estimate.baseline_cost_usd,
            estimated_savings_usd: estimate.estimated_savings_usd,
        })
    }

    fn input_is_idle_enough(&self, idle_for: Duration) -> bool {
        idle_for >= Duration::from_secs(self.config.general.idle_wait_seconds)
    }

    fn turbo_rescue_is_ready(
        &mut self,
        now: Instant,
        frequency: CpuFrequencySample,
        cpu_average_percent: Option<f32>,
        cpu_base_mhz: Option<u32>,
    ) -> bool {
        let active = self.config.general.turbo_rescue_enabled
            && frequency
                .max_mhz
                .zip(cpu_base_mhz)
                .is_some_and(|(current, base)| current > base.saturating_add(100))
            && cpu_average_percent.is_some_and(|average| {
                average >= self.config.general.turbo_rescue_cpu_threshold_percent as f32
            });

        if !active {
            self.turbo_rescue_since = None;
            return false;
        }

        let since = *self.turbo_rescue_since.get_or_insert(now);
        now.duration_since(since)
            >= Duration::from_secs(self.config.general.turbo_rescue_window_seconds)
    }

    fn turbo_rescue_status_text(
        &self,
        turbo_rescue_ready: bool,
        cpu_base_mhz: Option<u32>,
        frequency: CpuFrequencySample,
    ) -> String {
        if cpu_base_mhz.is_none() || frequency.max_mhz.is_none() {
            return "unavailable".to_string();
        }
        if turbo_rescue_ready {
            return "holding".to_string();
        }
        if let Some(since) = self.turbo_rescue_since {
            return format!(
                "watching ({:.0}s / {}s)",
                since.elapsed().as_secs_f32(),
                self.config.general.turbo_rescue_window_seconds
            );
        }
        "inactive".to_string()
    }

    fn decide_plan(
        &mut self,
        has_match: bool,
        on_battery: bool,
        now: Instant,
        idle_for: Duration,
        turbo_rescue_ready: bool,
    ) -> PlanDecision {
        // A forced plan overrides all automatic logic.
        if let Some(ref guid) = self.forced_plan_guid {
            self.low_power_busy_since = None;
            return PlanDecision {
                guid: guid.clone(),
                trigger: "manual".to_string(),
            };
        }

        let suppress = on_battery && !self.config.general.promote_on_battery;

        if !suppress && has_match {
            self.low_power_busy_since = None;
            return PlanDecision {
                guid: self.config.general.performance_plan_guid.clone(),
                trigger: self
                    .last_match_trigger
                    .clone()
                    .unwrap_or_else(|| "watched app".to_string()),
            };
        }

        if !suppress && turbo_rescue_ready {
            self.low_power_busy_since = None;
            self.last_match_at = Some(now);
            self.last_match_trigger = Some("cpu turbo rescue".to_string());
            return PlanDecision {
                guid: self.config.general.performance_plan_guid.clone(),
                trigger: "cpu turbo rescue".to_string(),
            };
        }

        if !suppress {
            if let Some(last) = self.last_match_at {
                let hold = Duration::from_secs(self.config.general.hold_performance_seconds);
                if now.duration_since(last) < hold {
                    self.low_power_busy_since = None;
                    return PlanDecision {
                        guid: self.config.general.performance_plan_guid.clone(),
                        trigger: self
                            .last_match_trigger
                            .as_ref()
                            .map(|trigger| format!("{} (holding)", trigger))
                            .unwrap_or_else(|| "hold timer".to_string()),
                    };
                }
            }
        }

        let idle_ready = self.input_is_idle_enough(idle_for);
        let cpu_quiet = self.cpu_is_quiet(now);

        if idle_ready && cpu_quiet {
            self.low_power_busy_since = None;
            return PlanDecision {
                guid: self.config.general.low_power_plan_guid.clone(),
                trigger: "entered low power".to_string(),
            };
        }

        let is_currently_low_power =
            self.current_plan_guid == self.config.general.low_power_plan_guid;
        if is_currently_low_power && idle_ready {
            let hold = Duration::from_secs(self.config.general.hold_performance_seconds);
            if let Some(busy_since) = self.low_power_busy_since {
                if now.duration_since(busy_since) < hold {
                    return PlanDecision {
                        guid: self.config.general.low_power_plan_guid.clone(),
                        trigger: "idle + quiet cpu".to_string(),
                    };
                }
            } else {
                self.low_power_busy_since = Some(now);
                return PlanDecision {
                    guid: self.config.general.low_power_plan_guid.clone(),
                    trigger: "idle + quiet cpu".to_string(),
                };
            }
        }

        self.low_power_busy_since = None;
        PlanDecision {
            guid: self.config.general.standard_plan_guid.clone(),
            trigger: if idle_for < Duration::from_secs(self.config.general.idle_wait_seconds) {
                "input resumed".to_string()
            } else if !self.cpu_is_quiet(now) {
                "cpu above threshold".to_string()
            } else if self.last_match_at.is_some() {
                "hold expired".to_string()
            } else {
                "standard".to_string()
            },
        }
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
    let mut cpu_info = power.get_cpu_info().ok();
    let mut plan_processor_settings =
        refresh_plan_processor_settings(&*power, &state.config).unwrap_or_default();
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
                    plan_processor_settings =
                        refresh_plan_processor_settings(&*power, &state.config).unwrap_or_default();
                }
                MonitorCommand::ApplyPlanProcessorRecommendation {
                    guid,
                    recommendation,
                } => {
                    if let Err(err) =
                        power.apply_plan_processor_recommendation(&guid, recommendation)
                    {
                        app_state.write().unwrap().last_error =
                            Some(format!("Failed to update processor limits: {}", err));
                    }
                    plan_processor_settings =
                        refresh_plan_processor_settings(&*power, &state.config).unwrap_or_default();
                }
                MonitorCommand::RefreshPlans => {
                    if let Ok(plans) = power.enumerate_plans() {
                        state.available_plans = plans.clone();
                        app_state.write().unwrap().available_plans = plans;
                    }
                    cpu_info = power.get_cpu_info().ok();
                    plan_processor_settings =
                        refresh_plan_processor_settings(&*power, &state.config).unwrap_or_default();
                }
            }
        }

        let poll = Duration::from_millis(state.config.general.poll_interval_ms);
        let now = Instant::now();

        state.sys.refresh_cpu();
        let cpu_usage = state.sys.global_cpu_info().cpu_usage();
        state.record_cpu_observation(now, cpu_usage);
        let cpu_frequency = power.get_cpu_frequency_sample().unwrap_or_default();
        let cpu_average_percent = state.cpu_average_percent();
        let turbo_rescue_ready = state.turbo_rescue_is_ready(
            now,
            cpu_frequency,
            cpu_average_percent,
            cpu_info.as_ref().and_then(|info| info.base_mhz),
        );

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
            state.last_match_trigger = Some(matched.join(", "));
        }

        let battery = power.get_battery_status().unwrap_or_default();
        let idle_for = idle_reader.idle_duration().unwrap_or(Duration::ZERO);
        let decision = state.decide_plan(
            has_match,
            battery.on_battery,
            now,
            idle_for,
            turbo_rescue_ready,
        );
        let target_guid = decision.guid.clone();

        // Switch if the target differs from current
        if target_guid != state.current_plan_guid {
            let trigger = decision.trigger.clone();

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

            let history_trigger = state.current_trigger_description(&matched, idle_for, now);
            if let Some(point) = state.record_cpu_history(
                now,
                &history_trigger,
                cpu_frequency,
                cpu_info.as_ref().and_then(|info| info.base_mhz),
            ) {
                let _ = db::insert_dashboard_sample(&db_conn, &point);
            }

            let mut s = app_state.write().unwrap();
            s.current_plan = current_plan;
            s.matched_processes = matched;
            s.all_running_processes = running;
            s.hold_remaining_secs = hold_remaining;
            s.idle_for_secs = Some(idle_for.as_secs_f32());
            s.cpu_average_percent = cpu_average_percent;
            s.cpu_info = cpu_info.clone();
            s.cpu_frequency = cpu_frequency;
            s.turbo_rescue_state = state.turbo_rescue_status_text(
                turbo_rescue_ready,
                cpu_info.as_ref().and_then(|info| info.base_mhz),
                cpu_frequency,
            );
            s.plan_processor_settings = plan_processor_settings.clone();
            s.cpu_history = state
                .dashboard_cpu_history
                .iter()
                .map(|sample| sample.point.clone())
                .collect();
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

fn refresh_plan_processor_settings(
    power: &dyn PowerApi,
    config: &Config,
) -> Result<BTreeMap<String, Option<crate::types::PlanProcessorSettings>>, ()> {
    let mut result = BTreeMap::new();
    for guid in [
        &config.general.standard_plan_guid,
        &config.general.performance_plan_guid,
        &config.general.low_power_plan_guid,
    ] {
        if guid.is_empty() || result.contains_key(guid) {
            continue;
        }
        result.insert(guid.clone(), power.read_plan_processor_settings(guid).ok());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::types::CpuHistoryPlanKind;
    use std::time::{Duration, Instant};

    fn test_config() -> Config {
        let mut c = Config::default();
        c.general.standard_plan_guid = "standard-guid".into();
        c.general.low_power_plan_guid = "low-guid".into();
        c.general.performance_plan_guid = "perf-guid".into();
        c.general.hold_performance_seconds = 10;
        c.general.idle_wait_seconds = 600;
        c.general.cpu_average_threshold_percent = 10;
        c.general.cpu_average_window_seconds = 60;
        c.general.turbo_rescue_enabled = true;
        c.general.turbo_rescue_cpu_threshold_percent = 10;
        c.general.turbo_rescue_window_seconds = 15;
        c.general.promote_on_battery = false;
        c
    }

    fn decide_guid(
        state: &mut MonitorState,
        has_match: bool,
        on_battery: bool,
        now: Instant,
        idle_for: Duration,
    ) -> String {
        state
            .decide_plan(has_match, on_battery, now, idle_for, false)
            .guid
    }

    #[test]
    fn test_match_triggers_performance() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        assert_eq!(
            decide_guid(&mut s, true, false, Instant::now(), Duration::ZERO),
            "perf-guid"
        );
    }

    #[test]
    fn test_no_match_no_history_returns_idle() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        assert_eq!(
            decide_guid(&mut s, false, false, Instant::now(), Duration::ZERO),
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
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(5),
                Duration::ZERO
            ),
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
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(15),
                Duration::ZERO
            ),
            "standard-guid"
        );
    }

    #[test]
    fn test_on_battery_suppresses_promotion() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        // on_battery = true, promote_on_battery = false → stay idle even with match
        assert_eq!(
            decide_guid(&mut s, true, true, Instant::now(), Duration::ZERO),
            "standard-guid"
        );
    }

    #[test]
    fn test_battery_bypass_allows_promotion() {
        let mut cfg = test_config();
        cfg.general.promote_on_battery = true;
        let mut s = MonitorState::new(cfg, "standard-guid".into(), vec![]);
        assert_eq!(
            decide_guid(&mut s, true, true, Instant::now(), Duration::ZERO),
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
            decide_guid(
                &mut s,
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
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "standard-guid"
        );
    }

    #[test]
    fn test_low_power_holds_during_brief_cpu_spike() {
        let mut s = MonitorState::new(test_config(), "low-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 25.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 20.0);
        s.record_cpu_sample(base + Duration::from_secs(61), 22.0);

        assert_eq!(
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "low-guid"
        );
    }

    #[test]
    fn test_low_power_exits_after_sustained_cpu_spike() {
        let mut s = MonitorState::new(test_config(), "low-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 25.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 20.0);
        s.record_cpu_sample(base + Duration::from_secs(61), 22.0);
        s.record_cpu_sample(base + Duration::from_secs(72), 24.0);

        assert_eq!(
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "low-guid"
        );

        assert_eq!(
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(72),
                Duration::from_secs(612)
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
            decide_guid(
                &mut s,
                false,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(120)
            ),
            "standard-guid"
        );
    }

    #[test]
    fn test_input_resume_exits_low_power_immediately() {
        let mut s = MonitorState::new(test_config(), "low-guid".into(), vec![]);
        let base = Instant::now();
        s.record_cpu_sample(base, 25.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 20.0);
        s.record_cpu_sample(base + Duration::from_secs(61), 22.0);

        assert_eq!(
            decide_guid(
                &mut s,
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
            decide_guid(
                &mut s,
                true,
                false,
                base + Duration::from_secs(61),
                Duration::from_secs(601)
            ),
            "perf-guid"
        );
    }

    #[test]
    fn test_turbo_rescue_requires_frequency_above_base() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        assert!(!s.turbo_rescue_is_ready(
            base + Duration::from_secs(16),
            CpuFrequencySample {
                max_mhz: Some(3560),
            },
            Some(25.0),
            Some(3500),
        ));
    }

    #[test]
    fn test_turbo_rescue_requires_cpu_average_above_threshold() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        assert!(!s.turbo_rescue_is_ready(
            base + Duration::from_secs(16),
            CpuFrequencySample {
                max_mhz: Some(3800),
            },
            Some(5.0),
            Some(3500),
        ));
    }

    #[test]
    fn test_sustained_turbo_rescue_promotes_to_performance() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();
        let frequency = CpuFrequencySample {
            max_mhz: Some(3800),
        };

        assert!(!s.turbo_rescue_is_ready(base, frequency, Some(25.0), Some(3500)));
        let ready = s.turbo_rescue_is_ready(
            base + Duration::from_secs(16),
            frequency,
            Some(25.0),
            Some(3500),
        );
        let decision = s.decide_plan(
            false,
            false,
            base + Duration::from_secs(16),
            Duration::from_secs(120),
            ready,
        );

        assert!(ready);
        assert_eq!(decision.guid, "perf-guid");
        assert_eq!(decision.trigger, "cpu turbo rescue");
    }

    #[test]
    fn test_battery_suppresses_turbo_rescue() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        let decision = s.decide_plan(
            false,
            true,
            base + Duration::from_secs(61),
            Duration::from_secs(120),
            true,
        );

        assert_eq!(decision.guid, "standard-guid");
    }

    #[test]
    fn test_manual_force_overrides_turbo_rescue() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        s.forced_plan_guid = Some("low-guid".into());
        let base = Instant::now();

        let decision = s.decide_plan(
            false,
            false,
            base + Duration::from_secs(61),
            Duration::from_secs(120),
            true,
        );

        assert_eq!(decision.guid, "low-guid");
        assert_eq!(decision.trigger, "manual");
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

    #[test]
    fn test_cpu_history_requires_average_before_recording() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        s.record_cpu_sample(base, 4.0);
        s.record_cpu_history(base, "standard", CpuFrequencySample::default(), None);

        assert!(s.dashboard_cpu_history.is_empty());

        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);
        s.record_cpu_history(
            base + Duration::from_secs(30),
            "standard",
            CpuFrequencySample::default(),
            None,
        );

        assert_eq!(s.dashboard_cpu_history.len(), 1);
        assert_eq!(s.dashboard_cpu_history[0].point.average_percent, 5.0);
    }

    #[test]
    fn test_cpu_history_records_before_full_quiet_window() {
        let mut s = MonitorState::new(
            test_config(),
            "standard-guid".into(),
            vec![PowerPlan {
                guid: "standard-guid".into(),
                name: "Balanced".into(),
            }],
        );
        let base = Instant::now();

        s.record_cpu_sample(base, 4.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);
        s.record_cpu_history(
            base + Duration::from_secs(30),
            "startup",
            CpuFrequencySample::default(),
            None,
        );

        assert_eq!(s.dashboard_cpu_history.len(), 1);
        assert_eq!(s.dashboard_cpu_history[0].point.plan_name, "Balanced");
        assert_eq!(s.dashboard_cpu_history[0].point.trigger, "startup");
    }

    #[test]
    fn test_cpu_history_prunes_to_fifteen_minutes() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        s.record_cpu_sample(base, 4.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);
        s.record_cpu_history(
            base + Duration::from_secs(30),
            "standard",
            CpuFrequencySample::default(),
            None,
        );

        let sixteen_minutes_later = base + Duration::from_secs(16 * 60);
        s.record_cpu_sample(sixteen_minutes_later - Duration::from_secs(30), 7.0);
        s.record_cpu_sample(sixteen_minutes_later, 9.0);
        s.record_cpu_history(
            sixteen_minutes_later,
            "standard",
            CpuFrequencySample::default(),
            None,
        );

        assert_eq!(s.dashboard_cpu_history.len(), 1);
        assert_eq!(s.dashboard_cpu_history[0].point.average_percent, 8.0);
    }

    #[test]
    fn test_cpu_history_records_plan_kind() {
        let mut s = MonitorState::new(
            test_config(),
            "perf-guid".into(),
            vec![PowerPlan {
                guid: "perf-guid".into(),
                name: "Ultra performance".into(),
            }],
        );
        let base = Instant::now();

        s.record_cpu_sample(base, 15.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 21.0);
        s.record_cpu_sample(base + Duration::from_secs(61), 18.0);
        s.record_cpu_history(
            base + Duration::from_secs(61),
            "rustc.exe",
            CpuFrequencySample::default(),
            None,
        );

        assert_eq!(s.dashboard_cpu_history.len(), 1);
        assert_eq!(
            s.dashboard_cpu_history[0].point.plan_kind,
            CpuHistoryPlanKind::Performance
        );
        assert_eq!(
            s.dashboard_cpu_history[0].point.plan_name,
            "Ultra performance"
        );
        assert_eq!(s.dashboard_cpu_history[0].point.trigger, "rustc.exe");
    }

    #[test]
    fn test_hold_trigger_keeps_last_matching_executable() {
        let mut s = MonitorState::new(test_config(), "perf-guid".into(), vec![]);
        let base = Instant::now();
        s.last_match_at = Some(base);
        s.last_match_trigger = Some("rustc.exe".into());

        let trigger = s.current_trigger_description(
            &[],
            Duration::from_secs(601),
            base + Duration::from_secs(5),
        );

        assert_eq!(trigger, "rustc.exe (holding)");
    }

    #[test]
    fn test_first_cpu_observation_is_ignored() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        s.record_cpu_observation(base, 95.0);
        assert!(s.cpu_samples.is_empty());

        s.record_cpu_observation(base + Duration::from_secs(30), 5.0);
        assert_eq!(s.cpu_samples.len(), 1);
        assert_eq!(s.cpu_samples[0].usage_percent, 5.0);
    }

    #[test]
    fn test_cpu_history_prunes_with_first_real_sample_retained() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        s.record_cpu_sample(base, 4.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);
        s.record_cpu_sample(base + Duration::from_secs(61), 7.0);
        s.record_cpu_sample(base + Duration::from_secs(91), 8.0);

        assert_eq!(s.first_cpu_sample_at, Some(base));
        assert_eq!(s.cpu_samples.len(), 2);
        assert_eq!(s.cpu_samples[0].usage_percent, 7.0);
        assert_eq!(s.cpu_samples[1].usage_percent, 8.0);
    }

    #[test]
    fn test_dashboard_history_sampling_uses_fixed_interval() {
        let mut s = MonitorState::new(test_config(), "standard-guid".into(), vec![]);
        let base = Instant::now();

        s.record_cpu_sample(base, 4.0);
        s.record_cpu_sample(base + Duration::from_secs(30), 6.0);
        let first = s.record_cpu_history(
            base + Duration::from_secs(30),
            "standard",
            CpuFrequencySample::default(),
            None,
        );
        let second = s.record_cpu_history(
            base + Duration::from_secs(35),
            "standard",
            CpuFrequencySample::default(),
            None,
        );
        let third = s.record_cpu_history(
            base + Duration::from_secs(60),
            "standard",
            CpuFrequencySample::default(),
            None,
        );

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(third.is_some());
        assert_eq!(s.dashboard_cpu_history.len(), 2);
    }
}
