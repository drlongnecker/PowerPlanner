// src/power.rs
use crate::types::{
    BatteryStatus, CpuFrequencySample, CpuInfo, PlanProcessorRecommendation, PlanProcessorSettings,
    PowerPlan, ProcessorLimit,
};
use anyhow::{bail, Result};
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};

pub trait PowerApi: Send + Sync {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>>;
    fn get_active_plan(&self) -> Result<PowerPlan>;
    fn set_active_plan(&self, guid: &str) -> Result<()>;
    fn get_battery_status(&self) -> Result<BatteryStatus>;
    fn get_cpu_info(&self) -> Result<CpuInfo>;
    fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample>;
    fn read_plan_processor_settings(&self, guid: &str) -> Result<PlanProcessorSettings>;
    fn apply_plan_processor_recommendation(
        &self,
        guid: &str,
        recommendation: PlanProcessorRecommendation,
    ) -> Result<()>;
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
        if let Ok(guid) = get_active_scheme_guid() {
            let plans = self.enumerate_plans().unwrap_or_default();
            if let Some(plan) = plans.into_iter().find(|plan| plan.guid == guid) {
                return Ok(plan);
            }
            return Ok(PowerPlan {
                name: guid.clone(),
                guid,
            });
        }

        let output = powercfg(&["/getactivescheme"])?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(parse_scheme_line)
            .ok_or_else(|| anyhow::anyhow!("Could not determine active power scheme"))
    }

    fn set_active_plan(&self, guid: &str) -> Result<()> {
        set_active_scheme_guid(guid)
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

    fn get_cpu_info(&self) -> Result<CpuInfo> {
        let sys = sysinfo::System::new_all();
        let cpu = sys.cpus().first();
        let brand = cpu.map(|cpu| cpu.brand().to_string()).unwrap_or_default();
        let manufacturer = cpu
            .map(|cpu| cpu.vendor_id().to_string())
            .unwrap_or_default();
        let base_mhz = parse_base_mhz_from_brand(&brand);
        Ok(CpuInfo {
            manufacturer,
            brand,
            base_mhz,
            cores: sys.physical_core_count().map(|cores| cores as u32),
            logical_processors: Some(sys.cpus().len() as u32).filter(|count| *count > 0),
        })
    }

    fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample> {
        read_effective_cpu_frequency_sample().or_else(|_| read_cpu_frequency_sample())
    }

    fn read_plan_processor_settings(&self, guid: &str) -> Result<PlanProcessorSettings> {
        read_plan_processor_settings(guid)
    }

    fn apply_plan_processor_recommendation(
        &self,
        guid: &str,
        recommendation: PlanProcessorRecommendation,
    ) -> Result<()> {
        write_plan_processor_settings(guid, recommendation)
    }
}

#[cfg(windows)]
fn parse_base_mhz_from_brand(brand: &str) -> Option<u32> {
    let marker = brand.rfind('@')?;
    let value = brand[marker + 1..].trim();
    let ghz = value
        .strip_suffix("GHz")
        .or_else(|| value.strip_suffix("Ghz"))
        .or_else(|| value.strip_suffix("ghz"))?
        .trim()
        .parse::<f32>()
        .ok()?;
    Some((ghz * 1000.0).round() as u32)
}

#[cfg(windows)]
fn guid_from_string(guid: &str) -> Result<windows::core::GUID> {
    windows::core::GUID::try_from(guid).map_err(|_| anyhow::anyhow!("Invalid GUID {}", guid))
}

#[cfg(windows)]
fn guid_to_string(guid: windows::core::GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

#[cfg(windows)]
fn get_active_scheme_guid() -> Result<String> {
    use windows::Win32::System::Power::PowerGetActiveScheme;
    use windows::Win32::System::Registry::HKEY;

    unsafe {
        let mut raw: *mut windows::core::GUID = std::ptr::null_mut();
        let err = PowerGetActiveScheme(HKEY::default(), &mut raw);
        if err.0 != 0 {
            bail!("PowerGetActiveScheme failed: {}", err.0);
        }
        let guid = *raw;
        windows::Win32::System::Com::CoTaskMemFree(Some(raw.cast()));
        Ok(guid_to_string(guid))
    }
}

#[cfg(windows)]
fn set_active_scheme_guid(guid: &str) -> Result<()> {
    use windows::Win32::System::Power::PowerSetActiveScheme;
    use windows::Win32::System::Registry::HKEY;

    let guid = guid_from_string(guid)?;
    unsafe {
        let err = PowerSetActiveScheme(HKEY::default(), Some(&guid));
        if err.0 != 0 {
            bail!("PowerSetActiveScheme failed: {}", err.0);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn read_cpu_frequency_sample() -> Result<CpuFrequencySample> {
    use windows::Win32::System::Power::{
        CallNtPowerInformation, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
    };

    let logical_processors = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let mut infos = vec![PROCESSOR_POWER_INFORMATION::default(); logical_processors];
    let bytes = (infos.len() * std::mem::size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    unsafe {
        let status = CallNtPowerInformation(
            ProcessorInformation,
            None,
            0,
            Some(infos.as_mut_ptr().cast()),
            bytes,
        );
        if status.0 != 0 {
            bail!("CallNtPowerInformation failed: {}", status.0);
        }
    }
    Ok(CpuFrequencySample {
        max_mhz: infos.iter().map(|info| info.CurrentMhz).max(),
    })
}

#[cfg(windows)]
struct PdhPerformanceReader {
    query: isize,
    counter: isize,
}

#[cfg(windows)]
impl PdhPerformanceReader {
    fn new() -> Result<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::{
            PdhAddEnglishCounterW, PdhCollectQueryData, PdhOpenQueryW,
        };

        let mut query = 0_isize;
        let mut counter = 0_isize;
        unsafe {
            let open = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
            if open != 0 {
                bail!("PdhOpenQueryW failed: {}", open);
            }
            let path: Vec<u16> = "\\Processor Information(_Total)\\% Processor Performance\0"
                .encode_utf16()
                .collect();
            let add = PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter);
            if add != 0 {
                let _ = windows::Win32::System::Performance::PdhCloseQuery(query);
                bail!("PdhAddEnglishCounterW failed: {}", add);
            }
            let _ = PdhCollectQueryData(query);
        }
        Ok(Self { query, counter })
    }

    fn sample_percent(&mut self) -> Result<f64> {
        use windows::Win32::System::Performance::{
            PdhCollectQueryData, PdhGetFormattedCounterValue, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
        };

        unsafe {
            let collect = PdhCollectQueryData(self.query);
            if collect != 0 {
                bail!("PdhCollectQueryData failed: {}", collect);
            }
            let mut value = PDH_FMT_COUNTERVALUE::default();
            let format =
                PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut value);
            if format != 0 {
                bail!("PdhGetFormattedCounterValue failed: {}", format);
            }
            Ok(value.Anonymous.doubleValue)
        }
    }
}

#[cfg(windows)]
impl Drop for PdhPerformanceReader {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}

#[cfg(windows)]
fn read_effective_cpu_frequency_sample() -> Result<CpuFrequencySample> {
    static READER: OnceLock<Mutex<Option<PdhPerformanceReader>>> = OnceLock::new();

    let base_mhz = WindowsPowerApi.get_cpu_info()?.base_mhz;
    let Some(base_mhz) = base_mhz else {
        bail!("CPU base MHz unavailable");
    };

    let reader = READER.get_or_init(|| Mutex::new(PdhPerformanceReader::new().ok()));
    let mut guard = reader.lock().unwrap();
    let Some(reader) = guard.as_mut() else {
        bail!("PDH processor performance counter unavailable");
    };
    let performance_percent = reader.sample_percent()?;
    if performance_percent <= 0.0 {
        bail!(
            "PDH processor performance counter returned {}",
            performance_percent
        );
    }
    Ok(CpuFrequencySample {
        max_mhz: Some(((base_mhz as f64) * performance_percent / 100.0).round() as u32),
    })
}

#[cfg(windows)]
const GUID_PROCESSOR_SETTINGS_SUBGROUP: windows::core::GUID =
    windows::core::GUID::from_u128(0x54533251_82be_4824_96c1_47b60b740d00);
#[cfg(windows)]
const GUID_PROCESSOR_THROTTLE_MINIMUM: windows::core::GUID =
    windows::core::GUID::from_u128(0x893dee8e_2bef_41e0_89c6_b55d0929964c);
#[cfg(windows)]
const GUID_PROCESSOR_THROTTLE_MAXIMUM: windows::core::GUID =
    windows::core::GUID::from_u128(0xbc5038f7_23e0_4960_96da_33abaf5935ec);

#[cfg(windows)]
fn read_plan_processor_settings(guid: &str) -> Result<PlanProcessorSettings> {
    Ok(PlanProcessorSettings {
        min_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MINIMUM),
        max_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MAXIMUM),
    })
}

#[cfg(windows)]
fn read_processor_limit(guid: &str, setting: &windows::core::GUID) -> ProcessorLimit {
    ProcessorLimit {
        ac: read_processor_value(guid, setting, true).ok(),
        dc: read_processor_value(guid, setting, false).ok(),
    }
}

#[cfg(windows)]
fn read_processor_value(guid: &str, setting: &windows::core::GUID, ac: bool) -> Result<u32> {
    use windows::Win32::System::Power::{PowerReadACValueIndex, PowerReadDCValueIndex};
    use windows::Win32::System::Registry::HKEY;

    let scheme = guid_from_string(guid)?;
    let mut value = 0_u32;
    unsafe {
        let err = if ac {
            PowerReadACValueIndex(
                HKEY::default(),
                Some(&scheme),
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                &mut value,
            )
            .0
        } else {
            PowerReadDCValueIndex(
                HKEY::default(),
                Some(&scheme),
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                &mut value,
            )
        };
        if err != 0 {
            bail!("PowerRead processor value failed: {}", err);
        }
    }
    Ok(value)
}

#[cfg(windows)]
fn write_plan_processor_settings(
    guid: &str,
    recommendation: PlanProcessorRecommendation,
) -> Result<()> {
    write_processor_value(
        guid,
        &GUID_PROCESSOR_THROTTLE_MINIMUM,
        true,
        recommendation.min_percent,
    )?;
    write_processor_value(
        guid,
        &GUID_PROCESSOR_THROTTLE_MINIMUM,
        false,
        recommendation.min_percent,
    )?;
    write_processor_value(
        guid,
        &GUID_PROCESSOR_THROTTLE_MAXIMUM,
        true,
        recommendation.max_percent,
    )?;
    write_processor_value(
        guid,
        &GUID_PROCESSOR_THROTTLE_MAXIMUM,
        false,
        recommendation.max_percent,
    )?;
    Ok(())
}

#[cfg(windows)]
fn write_processor_value(
    guid: &str,
    setting: &windows::core::GUID,
    ac: bool,
    value: u32,
) -> Result<()> {
    use windows::Win32::System::Power::{PowerWriteACValueIndex, PowerWriteDCValueIndex};
    use windows::Win32::System::Registry::HKEY;

    let scheme = guid_from_string(guid)?;
    unsafe {
        let err = if ac {
            PowerWriteACValueIndex(
                HKEY::default(),
                &scheme,
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                value,
            )
            .0
        } else {
            PowerWriteDCValueIndex(
                HKEY::default(),
                &scheme,
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                value,
            )
        };
        if err != 0 {
            bail!("PowerWrite processor value failed: {}", err);
        }
    }
    Ok(())
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
    fn get_cpu_info(&self) -> Result<CpuInfo> {
        Ok(CpuInfo::default())
    }
    fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample> {
        Ok(CpuFrequencySample::default())
    }
    fn read_plan_processor_settings(&self, _guid: &str) -> Result<PlanProcessorSettings> {
        Ok(PlanProcessorSettings::default())
    }
    fn apply_plan_processor_recommendation(
        &self,
        _guid: &str,
        _recommendation: PlanProcessorRecommendation,
    ) -> Result<()> {
        Ok(())
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
        pub cpu_info: CpuInfo,
        pub cpu_frequency: CpuFrequencySample,
        pub processor_settings: Mutex<std::collections::BTreeMap<String, PlanProcessorSettings>>,
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
                cpu_info: CpuInfo {
                    manufacturer: "GenuineIntel".into(),
                    brand: "Test CPU @ 3.50GHz".into(),
                    base_mhz: Some(3500),
                    cores: Some(8),
                    logical_processors: Some(16),
                },
                cpu_frequency: CpuFrequencySample {
                    max_mhz: Some(3500),
                },
                processor_settings: Mutex::new(std::collections::BTreeMap::new()),
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
        fn get_cpu_info(&self) -> Result<CpuInfo> {
            Ok(self.cpu_info.clone())
        }
        fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample> {
            Ok(self.cpu_frequency)
        }
        fn read_plan_processor_settings(&self, guid: &str) -> Result<PlanProcessorSettings> {
            Ok(self
                .processor_settings
                .lock()
                .unwrap()
                .get(guid)
                .copied()
                .unwrap_or_default())
        }
        fn apply_plan_processor_recommendation(
            &self,
            guid: &str,
            recommendation: PlanProcessorRecommendation,
        ) -> Result<()> {
            self.processor_settings.lock().unwrap().insert(
                guid.to_string(),
                PlanProcessorSettings {
                    min_percent: ProcessorLimit {
                        ac: Some(recommendation.min_percent),
                        dc: Some(recommendation.min_percent),
                    },
                    max_percent: ProcessorLimit {
                        ac: Some(recommendation.max_percent),
                        dc: Some(recommendation.max_percent),
                    },
                },
            );
            Ok(())
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

        #[test]
        fn test_mock_apply_recommendation_writes_all_processor_limits() {
            let api = MockPowerApi::new();
            api.apply_plan_processor_recommendation(
                "balanced-guid",
                PlanProcessorRecommendation::standard_default(),
            )
            .unwrap();

            let settings = api.read_plan_processor_settings("balanced-guid").unwrap();

            assert_eq!(settings.min_percent.ac, Some(5));
            assert_eq!(settings.min_percent.dc, Some(5));
            assert_eq!(settings.max_percent.ac, Some(99));
            assert_eq!(settings.max_percent.dc, Some(99));
        }
    }
}
