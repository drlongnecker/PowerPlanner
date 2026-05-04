use crate::types::CpuHistoryPlanKind;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyRate {
    pub dollars_per_kwh: f64,
    pub source_label: String,
}

pub trait EnergyRateProvider {
    fn current_rate(&self) -> EnergyRate;
}

#[derive(Debug, Clone)]
pub struct ManualRateProvider {
    rate: EnergyRate,
}

impl ManualRateProvider {
    pub fn new(dollars_per_kwh: f64, source_label: String) -> Self {
        Self {
            rate: EnergyRate {
                dollars_per_kwh,
                source_label,
            },
        }
    }
}

impl EnergyRateProvider for ManualRateProvider {
    fn current_rate(&self) -> EnergyRate {
        self.rate.clone()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CpuPowerProfile {
    pub idle_watts: f64,
    pub base_watts: f64,
    pub turbo_watts: f64,
    pub source_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuPowerSample {
    pub cpu_average_percent: f32,
    pub current_mhz: Option<u32>,
    pub base_mhz: Option<u32>,
    pub plan_kind: CpuHistoryPlanKind,
}

pub trait CpuPowerProvider {
    fn estimated_watts(&self, sample: CpuPowerSample) -> f64;
}

#[derive(Debug, Clone)]
pub struct ModeledCpuPowerProvider {
    profile: CpuPowerProfile,
}

impl ModeledCpuPowerProvider {
    pub fn new(profile: CpuPowerProfile) -> Self {
        Self { profile }
    }
}

impl CpuPowerProvider for ModeledCpuPowerProvider {
    fn estimated_watts(&self, sample: CpuPowerSample) -> f64 {
        let utilization = (sample.cpu_average_percent as f64 / 100.0).clamp(0.0, 1.0);
        let turbo_active = sample
            .current_mhz
            .zip(sample.base_mhz)
            .is_some_and(|(current, base)| current > base.saturating_add(100));
        let peak_watts = if turbo_active || sample.plan_kind == CpuHistoryPlanKind::Performance {
            self.profile.turbo_watts
        } else {
            self.profile.base_watts
        };

        let frequency_factor = if turbo_active {
            1.0
        } else {
            sample
                .current_mhz
                .zip(sample.base_mhz)
                .map(|(current, base)| {
                    if base == 0 {
                        1.0
                    } else {
                        (current as f64 / base as f64).clamp(0.5, 1.0)
                    }
                })
                .unwrap_or(1.0)
        };

        self.profile.idle_watts
            + (peak_watts - self.profile.idle_watts).max(0.0) * utilization * frequency_factor
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyEstimate {
    pub estimated_kwh: f64,
    pub estimated_cost_usd: f64,
    pub baseline_cost_usd: f64,
    pub estimated_savings_usd: f64,
}

pub fn estimate_sample_energy(
    estimated_watts: f64,
    baseline_watts: f64,
    sample_duration: Duration,
    rate: EnergyRate,
) -> EnergyEstimate {
    let hours = sample_duration.as_secs_f64() / 3600.0;
    let estimated_kwh = estimated_watts.max(0.0) * hours / 1000.0;
    let baseline_kwh = baseline_watts.max(0.0) * hours / 1000.0;
    let estimated_cost_usd = estimated_kwh * rate.dollars_per_kwh;
    let baseline_cost_usd = baseline_kwh * rate.dollars_per_kwh;
    EnergyEstimate {
        estimated_kwh,
        estimated_cost_usd,
        baseline_cost_usd,
        estimated_savings_usd: (baseline_cost_usd - estimated_cost_usd).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn profile() -> CpuPowerProfile {
        CpuPowerProfile {
            idle_watts: 12.0,
            base_watts: 65.0,
            turbo_watts: 125.0,
            source_label: "Test profile".to_string(),
        }
    }

    #[test]
    fn manual_rate_returns_configured_kwh_price() {
        let provider = ManualRateProvider::new(0.15, "Manual".to_string());

        let rate = provider.current_rate();

        assert_eq!(rate.dollars_per_kwh, 0.15);
        assert_eq!(rate.source_label, "Manual");
    }

    #[test]
    fn modeled_power_uses_idle_watts_for_quiet_low_power_samples() {
        let provider = ModeledCpuPowerProvider::new(profile());

        let watts = provider.estimated_watts(CpuPowerSample {
            cpu_average_percent: 2.0,
            current_mhz: Some(900),
            base_mhz: Some(3500),
            plan_kind: CpuHistoryPlanKind::LowPower,
        });

        assert_eq!(watts.round(), 13.0);
    }

    #[test]
    fn modeled_power_moves_toward_base_watts_under_standard_load() {
        let provider = ModeledCpuPowerProvider::new(profile());

        let watts = provider.estimated_watts(CpuPowerSample {
            cpu_average_percent: 50.0,
            current_mhz: Some(3500),
            base_mhz: Some(3500),
            plan_kind: CpuHistoryPlanKind::Standard,
        });

        assert_eq!(watts.round(), 39.0);
    }

    #[test]
    fn modeled_power_moves_toward_turbo_watts_above_base_speed() {
        let provider = ModeledCpuPowerProvider::new(profile());

        let watts = provider.estimated_watts(CpuPowerSample {
            cpu_average_percent: 80.0,
            current_mhz: Some(4700),
            base_mhz: Some(3500),
            plan_kind: CpuHistoryPlanKind::Performance,
        });

        assert_eq!(watts.round(), 102.0);
    }

    #[test]
    fn energy_estimate_converts_watts_to_kwh_cost_and_savings() {
        let estimate = estimate_sample_energy(
            60.0,
            125.0,
            Duration::from_secs(30),
            EnergyRate {
                dollars_per_kwh: 0.15,
                source_label: "Manual".to_string(),
            },
        );

        assert!((estimate.estimated_kwh - 0.0005).abs() < f64::EPSILON);
        assert!((estimate.estimated_cost_usd - 0.000075).abs() < f64::EPSILON);
        assert!((estimate.baseline_cost_usd - 0.00015625).abs() < f64::EPSILON);
        assert!((estimate.estimated_savings_usd - 0.00008125).abs() < f64::EPSILON);
    }
}
