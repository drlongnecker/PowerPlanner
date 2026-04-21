// src/power.rs
use crate::types::{BatteryStatus, PowerPlan};
use anyhow::{bail, Result};

pub trait PowerApi: Send + Sync {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>>;
    fn get_active_plan(&self) -> Result<PowerPlan>;
    fn set_active_plan(&self, guid: &str) -> Result<()>;
    fn get_battery_status(&self) -> Result<BatteryStatus>;
}

pub struct WindowsPowerApi;

#[cfg(windows)]
fn powercfg(args: &[&str]) -> Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Ok(std::process::Command::new("powercfg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?)
}

#[cfg(windows)]
fn parse_scheme_line(line: &str) -> Option<PowerPlan> {
    let rest = line.strip_prefix("Power Scheme GUID:")?.trim();
    let space = rest.find(' ')?;
    let guid = rest[..space].trim().to_lowercase();
    let name_part = rest[space..].trim();
    let name = if let (Some(s), Some(e)) = (name_part.find('('), name_part.rfind(')')) {
        name_part[s + 1..e].to_string()
    } else {
        guid.clone()
    };
    Some(PowerPlan { guid, name })
}

#[cfg(windows)]
impl PowerApi for WindowsPowerApi {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>> {
        let output = powercfg(&["/list"])?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().filter_map(parse_scheme_line).collect())
    }

    fn get_active_plan(&self) -> Result<PowerPlan> {
        let output = powercfg(&["/getactivescheme"])?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(parse_scheme_line)
            .ok_or_else(|| anyhow::anyhow!("Could not determine active power scheme"))
    }

    fn set_active_plan(&self, guid: &str) -> Result<()> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let status = std::process::Command::new("powercfg")
            .args(["/setactive", guid])
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;
        if !status.success() {
            bail!("powercfg /setactive failed for GUID {}", guid);
        }
        Ok(())
    }

    fn get_battery_status(&self) -> Result<BatteryStatus> {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        unsafe {
            let mut s = SYSTEM_POWER_STATUS::default();
            GetSystemPowerStatus(&mut s)?;
            Ok(BatteryStatus {
                on_battery: s.ACLineStatus == 0,
                percent: if s.BatteryLifePercent == 255 {
                    None
                } else {
                    Some(s.BatteryLifePercent)
                },
                charging: (s.BatteryFlag & 0x08) != 0,
            })
        }
    }
}

#[cfg(not(windows))]
impl PowerApi for WindowsPowerApi {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>> {
        Ok(vec![])
    }
    fn get_active_plan(&self) -> Result<PowerPlan> {
        Ok(PowerPlan {
            guid: "stub".into(),
            name: "Stub Plan".into(),
        })
    }
    fn set_active_plan(&self, _guid: &str) -> Result<()> {
        Ok(())
    }
    fn get_battery_status(&self) -> Result<BatteryStatus> {
        Ok(BatteryStatus::default())
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    pub struct MockPowerApi {
        pub plans: Vec<PowerPlan>,
        pub active_guid: Mutex<String>,
        pub battery: BatteryStatus,
    }

    impl MockPowerApi {
        pub fn new() -> Self {
            Self {
                plans: vec![
                    PowerPlan {
                        guid: "balanced-guid".into(),
                        name: "Balanced".into(),
                    },
                    PowerPlan {
                        guid: "perf-guid".into(),
                        name: "High Performance".into(),
                    },
                ],
                active_guid: Mutex::new("balanced-guid".into()),
                battery: BatteryStatus::default(),
            }
        }
    }

    impl PowerApi for MockPowerApi {
        fn enumerate_plans(&self) -> Result<Vec<PowerPlan>> {
            Ok(self.plans.clone())
        }
        fn get_active_plan(&self) -> Result<PowerPlan> {
            let guid = self.active_guid.lock().unwrap().clone();
            let plan = self
                .plans
                .iter()
                .find(|p| p.guid == guid)
                .cloned()
                .unwrap_or(PowerPlan {
                    name: guid.clone(),
                    guid,
                });
            Ok(plan)
        }
        fn set_active_plan(&self, guid: &str) -> Result<()> {
            *self.active_guid.lock().unwrap() = guid.to_string();
            Ok(())
        }
        fn get_battery_status(&self) -> Result<BatteryStatus> {
            Ok(self.battery.clone())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_mock_get_set_active_plan() {
            let api = MockPowerApi::new();
            assert_eq!(api.get_active_plan().unwrap().guid, "balanced-guid");
            api.set_active_plan("perf-guid").unwrap();
            let p = api.get_active_plan().unwrap();
            assert_eq!(p.guid, "perf-guid");
            assert_eq!(p.name, "High Performance");
        }

        #[test]
        fn test_mock_enumerate_returns_both_plans() {
            let api = MockPowerApi::new();
            let plans = api.enumerate_plans().unwrap();
            assert_eq!(plans.len(), 2);
            assert!(plans.iter().any(|p| p.name == "Balanced"));
            assert!(plans.iter().any(|p| p.name == "High Performance"));
        }
    }
}
