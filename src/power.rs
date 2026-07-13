// src/power.rs
use crate::types::{
    BatteryStatus, CpuEfficiencyClass, CpuFrequencySample, CpuInfo, PlanProcessorSettings,
    PowerPlan, ProcessorLimit, ProcessorPresetRecommendation,
};
use anyhow::{bail, Result};
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};

pub(crate) trait PowerApi: Send + Sync {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>>;
    fn duplicate_ultimate_performance(&self) -> Result<PowerPlan>;
    fn delete_plan(&self, guid: &str) -> Result<()>;
    fn get_active_plan(&self) -> Result<PowerPlan>;
    fn set_active_plan(&self, guid: &str) -> Result<()>;
    fn get_battery_status(&self) -> Result<BatteryStatus>;
    fn get_cpu_info(&self) -> Result<CpuInfo>;
    fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample>;
    fn read_plan_processor_settings(&self, guid: &str) -> Result<PlanProcessorSettings>;
    fn apply_processor_preset(
        &self,
        guid: &str,
        recommendation: ProcessorPresetRecommendation,
    ) -> Result<()>;
}

pub(crate) struct WindowsPowerApi;

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

    fn duplicate_ultimate_performance(&self) -> Result<PowerPlan> {
        duplicate_ultimate_performance()
    }

    fn delete_plan(&self, guid: &str) -> Result<()> {
        delete_plan(guid)
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
            GetSystemPowerStatus(&raw mut s)?;
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
        let efficiency_classes = cpu_efficiency_classes();
        Ok(CpuInfo {
            manufacturer,
            brand,
            base_mhz,
            cores: sys.physical_core_count().map(|cores| cores as u32),
            logical_processors: Some(sys.cpus().len() as u32).filter(|count| *count > 0),
            efficiency_classes,
        })
    }

    fn get_cpu_frequency_sample(&self) -> Result<CpuFrequencySample> {
        read_effective_cpu_frequency_sample().or_else(|_| read_cpu_frequency_sample())
    }

    fn read_plan_processor_settings(&self, guid: &str) -> Result<PlanProcessorSettings> {
        Ok(read_plan_processor_settings(guid))
    }

    fn apply_processor_preset(
        &self,
        guid: &str,
        recommendation: ProcessorPresetRecommendation,
    ) -> Result<()> {
        write_processor_preset(guid, recommendation)
    }
}

fn parse_cpu_efficiency_classes(data: &[u8]) -> Result<Vec<CpuEfficiencyClass>> {
    const HEADER_SIZE: usize = 8;
    const EFFICIENCY_CLASS_OFFSET: usize = 18;

    let mut offset = 0;
    let mut counts = std::collections::BTreeMap::<u8, u32>::new();
    while offset < data.len() {
        let remaining = &data[offset..];
        if remaining.len() < HEADER_SIZE {
            bail!("CPU set information ended with a truncated record header");
        }

        let size = u32::from_le_bytes(remaining[0..4].try_into().unwrap()) as usize;
        if size < HEADER_SIZE {
            bail!("CPU set information record has invalid size {size}");
        }
        if size > remaining.len() {
            bail!(
                "CPU set information record size {} exceeds {} remaining bytes",
                size,
                remaining.len()
            );
        }

        let information_type = i32::from_le_bytes(remaining[4..8].try_into().unwrap());
        if information_type == 0 {
            if size <= EFFICIENCY_CLASS_OFFSET {
                bail!("CPU set information record is too small to contain EfficiencyClass");
            }
            let efficiency_class = remaining[EFFICIENCY_CLASS_OFFSET];
            let count = counts.entry(efficiency_class).or_default();
            *count = count.saturating_add(1);
        }

        offset += size;
    }

    Ok(counts
        .into_iter()
        .map(|(value, logical_processors)| CpuEfficiencyClass {
            value,
            logical_processors,
        })
        .collect())
}

#[cfg(windows)]
fn cpu_efficiency_classes() -> Vec<CpuEfficiencyClass> {
    static CLASSES: OnceLock<Vec<CpuEfficiencyClass>> = OnceLock::new();
    CLASSES
        .get_or_init(|| match read_cpu_efficiency_classes() {
            Ok(classes) => classes,
            Err(err) => {
                log::warn!("CPU efficiency topology unavailable: {err}");
                Vec::new()
            }
        })
        .clone()
}

#[cfg(windows)]
fn read_cpu_efficiency_classes() -> Result<Vec<CpuEfficiencyClass>> {
    use std::mem::{size_of, MaybeUninit};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::SystemInformation::{
        GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
    };

    let mut required = 0_u32;
    let sizing_result =
        unsafe { GetSystemCpuSetInformation(None, 0, &raw mut required, HANDLE::default(), 0) };
    if required == 0 {
        if sizing_result.as_bool() {
            return Ok(Vec::new());
        }
        return Err(windows::core::Error::from_win32().into());
    }

    for _ in 0..3 {
        let element_size = size_of::<SYSTEM_CPU_SET_INFORMATION>();
        let element_count = (required as usize)
            .checked_add(element_size - 1)
            .ok_or_else(|| anyhow::anyhow!("CPU set information buffer size overflow"))?
            / element_size;
        let mut buffer = vec![MaybeUninit::<SYSTEM_CPU_SET_INFORMATION>::uninit(); element_count];
        let buffer_bytes = buffer
            .len()
            .checked_mul(element_size)
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| anyhow::anyhow!("CPU set information buffer is too large"))?;
        let mut returned = required;
        let result = unsafe {
            GetSystemCpuSetInformation(
                Some(buffer.as_mut_ptr().cast()),
                buffer_bytes,
                &raw mut returned,
                HANDLE::default(),
                0,
            )
        };
        if result.as_bool() {
            if returned > buffer_bytes {
                bail!(
                    "GetSystemCpuSetInformation returned {returned} bytes for a {buffer_bytes} byte buffer"
                );
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), returned as usize)
            };
            return parse_cpu_efficiency_classes(bytes);
        }

        let error = windows::core::Error::from_win32();
        if returned > buffer_bytes {
            required = returned;
            continue;
        }
        return Err(error.into());
    }

    bail!("CPU set information changed size repeatedly while being queried")
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
fn guid_from_string(guid: &str) -> windows::core::GUID {
    windows::core::GUID::from(guid)
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
fn duplicate_ultimate_performance() -> Result<PowerPlan> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::System::Power::PowerDuplicateScheme;
    use windows::Win32::System::Registry::HKEY;

    const ULTIMATE_PERFORMANCE_TEMPLATE: windows::core::GUID =
        windows::core::GUID::from_u128(0xe9a42b02_d5df_448d_aa00_03f14749eb61);

    unsafe {
        let mut raw: *mut windows::core::GUID = std::ptr::null_mut();
        let err = PowerDuplicateScheme(
            HKEY::default(),
            &ULTIMATE_PERFORMANCE_TEMPLATE,
            &raw mut raw,
        );
        if err.0 != 0 {
            bail!("PowerDuplicateScheme failed: {}", err.0);
        }
        if raw.is_null() {
            bail!("PowerDuplicateScheme returned no destination GUID");
        }

        let guid = guid_to_string(*raw);
        let _ = LocalFree(HLOCAL(raw.cast()));
        Ok(PowerPlan {
            guid,
            name: "Ultimate Performance".to_string(),
        })
    }
}

#[cfg(windows)]
fn delete_plan(guid: &str) -> Result<()> {
    use windows::Win32::System::Power::PowerDeleteScheme;
    use windows::Win32::System::Registry::HKEY;

    let scheme = guid_from_string(guid);
    unsafe {
        let err = PowerDeleteScheme(HKEY::default(), &raw const scheme);
        if err.0 != 0 {
            bail!("PowerDeleteScheme failed: {}", err.0);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn get_active_scheme_guid() -> Result<String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::System::Power::PowerGetActiveScheme;
    use windows::Win32::System::Registry::HKEY;

    unsafe {
        let mut raw: *mut windows::core::GUID = std::ptr::null_mut();
        let err = PowerGetActiveScheme(HKEY::default(), &raw mut raw);
        if err.0 != 0 {
            bail!("PowerGetActiveScheme failed: {}", err.0);
        }
        let guid = *raw;
        let _ = LocalFree(HLOCAL(raw.cast()));
        Ok(guid_to_string(guid))
    }
}

#[cfg(windows)]
fn set_active_scheme_guid(guid: &str) -> Result<()> {
    use windows::Win32::System::Power::PowerSetActiveScheme;
    use windows::Win32::System::Registry::HKEY;

    let guid = guid_from_string(guid);
    unsafe {
        let err = PowerSetActiveScheme(HKEY::default(), Some(&raw const guid));
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

    let logical_processors = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
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
            let open = PdhOpenQueryW(PCWSTR::null(), 0, &raw mut query);
            if open != 0 {
                bail!("PdhOpenQueryW failed: {open}");
            }
            let path: Vec<u16> = "\\Processor Information(_Total)\\% Processor Performance\0"
                .encode_utf16()
                .collect();
            let add = PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &raw mut counter);
            if add != 0 {
                let _ = windows::Win32::System::Performance::PdhCloseQuery(query);
                bail!("PdhAddEnglishCounterW failed: {add}");
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
                bail!("PdhCollectQueryData failed: {collect}");
            }
            let mut value = PDH_FMT_COUNTERVALUE::default();
            let format =
                PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &raw mut value);
            if format != 0 {
                bail!("PdhGetFormattedCounterValue failed: {format}");
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
        bail!("PDH processor performance counter returned {performance_percent}");
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
const GUID_PROCESSOR_THROTTLE_MINIMUM_CLASS1: windows::core::GUID =
    windows::core::GUID::from_u128(0x893dee8e_2bef_41e0_89c6_b55d0929964d);
#[cfg(windows)]
const GUID_PROCESSOR_THROTTLE_MAXIMUM_CLASS1: windows::core::GUID =
    windows::core::GUID::from_u128(0xbc5038f7_23e0_4960_96da_33abaf5935ed);
#[cfg(windows)]
const GUID_PROCESSOR_PERFORMANCE_BOOST_MODE: windows::core::GUID =
    windows::core::GUID::from_u128(0xbe337238_0d82_4146_a960_4f3749d470c7);
#[cfg(windows)]
const GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES: windows::core::GUID =
    windows::core::GUID::from_u128(0x0cc5b647_c1df_4637_891a_dec35c318583);
#[cfg(windows)]
const GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES_CLASS1: windows::core::GUID =
    windows::core::GUID::from_u128(0x0cc5b647_c1df_4637_891a_dec35c318584);
#[cfg(windows)]
const GUID_PROCESSOR_LATENCY_HINT_PERF: windows::core::GUID =
    windows::core::GUID::from_u128(0x619b7505_003b_4e82_b7a6_4dd29c300971);
#[cfg(windows)]
const GUID_PROCESSOR_IDLE_PROMOTE_THRESHOLD: windows::core::GUID =
    windows::core::GUID::from_u128(0x7b224883_b3cc_4d79_819f_8374152cbe7c);

#[cfg(windows)]
fn read_plan_processor_settings(guid: &str) -> PlanProcessorSettings {
    PlanProcessorSettings {
        min_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MINIMUM),
        max_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MAXIMUM),
        boost_mode: read_processor_limit(guid, &GUID_PROCESSOR_PERFORMANCE_BOOST_MODE),
        core_parking_min_cores_percent: read_processor_limit(
            guid,
            &GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES,
        ),
        latency_hint_perf: read_processor_limit(guid, &GUID_PROCESSOR_LATENCY_HINT_PERF),
        idle_promote_threshold: read_processor_limit(guid, &GUID_PROCESSOR_IDLE_PROMOTE_THRESHOLD),
        class1_min_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MINIMUM_CLASS1),
        class1_max_percent: read_processor_limit(guid, &GUID_PROCESSOR_THROTTLE_MAXIMUM_CLASS1),
        class1_core_parking_min_cores_percent: read_processor_limit(
            guid,
            &GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES_CLASS1,
        ),
    }
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

    let scheme = guid_from_string(guid);
    let mut value = 0_u32;
    unsafe {
        let err = if ac {
            PowerReadACValueIndex(
                HKEY::default(),
                Some(&raw const scheme),
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                &raw mut value,
            )
            .0
        } else {
            PowerReadDCValueIndex(
                HKEY::default(),
                Some(&raw const scheme),
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                &raw mut value,
            )
        };
        if err != 0 {
            bail!("PowerRead processor value failed: {err}");
        }
    }
    Ok(value)
}

#[cfg(windows)]
fn write_plan_processor_settings(guid: &str, min_percent: u32, max_percent: u32) -> Result<()> {
    write_processor_value(guid, &GUID_PROCESSOR_THROTTLE_MINIMUM, true, min_percent)?;
    write_processor_value(guid, &GUID_PROCESSOR_THROTTLE_MINIMUM, false, min_percent)?;
    write_processor_value(guid, &GUID_PROCESSOR_THROTTLE_MAXIMUM, true, max_percent)?;
    write_processor_value(guid, &GUID_PROCESSOR_THROTTLE_MAXIMUM, false, max_percent)?;
    Ok(())
}

#[cfg(windows)]
fn write_processor_preset(guid: &str, recommendation: ProcessorPresetRecommendation) -> Result<()> {
    write_plan_processor_settings(guid, recommendation.min_percent, recommendation.max_percent)?;
    if let Some(boost_mode) = recommendation.boost_mode {
        write_processor_value_both(guid, &GUID_PROCESSOR_PERFORMANCE_BOOST_MODE, boost_mode)?;
    }
    if let Some(core_parking) = recommendation.core_parking_min_cores_percent {
        write_processor_value_both(
            guid,
            &GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES,
            core_parking,
        )?;
    }
    if let Some(latency_hint_perf) = recommendation.latency_hint_perf {
        write_processor_value_both(guid, &GUID_PROCESSOR_LATENCY_HINT_PERF, latency_hint_perf)?;
    }
    if let Some(idle_promote) = recommendation.idle_promote_threshold {
        write_processor_value_both(guid, &GUID_PROCESSOR_IDLE_PROMOTE_THRESHOLD, idle_promote)?;
    }
    if let Some(class1) = recommendation.class1 {
        write_processor_value_both(
            guid,
            &GUID_PROCESSOR_THROTTLE_MINIMUM_CLASS1,
            class1.min_percent,
        )?;
        write_processor_value_both(
            guid,
            &GUID_PROCESSOR_THROTTLE_MAXIMUM_CLASS1,
            class1.max_percent,
        )?;
        if let Some(core_parking) = class1.core_parking_min_cores_percent {
            write_processor_value_both(
                guid,
                &GUID_PROCESSOR_CORE_PARKING_MINIMUM_CORES_CLASS1,
                core_parking,
            )?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn write_processor_value_both(guid: &str, setting: &windows::core::GUID, value: u32) -> Result<()> {
    write_processor_value(guid, setting, true, value)?;
    write_processor_value(guid, setting, false, value)
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

    let scheme = guid_from_string(guid);
    unsafe {
        let err = if ac {
            PowerWriteACValueIndex(
                HKEY::default(),
                &raw const scheme,
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                value,
            )
            .0
        } else {
            PowerWriteDCValueIndex(
                HKEY::default(),
                &raw const scheme,
                Some(&GUID_PROCESSOR_SETTINGS_SUBGROUP),
                Some(setting),
                value,
            )
        };
        if err != 0 {
            bail!("PowerWrite processor value failed: {err}");
        }
    }
    Ok(())
}

#[cfg(not(windows))]
impl PowerApi for WindowsPowerApi {
    fn enumerate_plans(&self) -> Result<Vec<PowerPlan>> {
        Ok(vec![])
    }
    fn duplicate_ultimate_performance(&self) -> Result<PowerPlan> {
        Ok(PowerPlan {
            guid: "ultimate-performance-stub".into(),
            name: "Ultimate Performance".into(),
        })
    }
    fn delete_plan(&self, _guid: &str) -> Result<()> {
        Ok(())
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
    fn apply_processor_preset(
        &self,
        _guid: &str,
        _recommendation: ProcessorPresetRecommendation,
    ) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod cpu_topology_tests {
    use super::*;

    fn record(size: usize, information_type: i32, efficiency_class: u8) -> Vec<u8> {
        let mut record = vec![0_u8; size];
        record[0..4].copy_from_slice(&(size as u32).to_le_bytes());
        record[4..8].copy_from_slice(&information_type.to_le_bytes());
        if information_type == 0 && size > 18 {
            record[18] = efficiency_class;
        }
        record
    }

    #[test]
    fn parser_aggregates_sorted_efficiency_classes() {
        let mut data = record(32, 0, 2);
        data.extend(record(32, 0, 0));
        data.extend(record(32, 0, 2));

        assert_eq!(
            parse_cpu_efficiency_classes(&data).unwrap(),
            vec![
                CpuEfficiencyClass {
                    value: 0,
                    logical_processors: 1,
                },
                CpuEfficiencyClass {
                    value: 2,
                    logical_processors: 2,
                },
            ]
        );
    }

    #[test]
    fn parser_uses_each_record_size_and_skips_unknown_types() {
        let mut data = record(40, 0, 7);
        data.extend(record(12, 19, 0));
        data.extend(record(24, 0, 7));

        assert_eq!(
            parse_cpu_efficiency_classes(&data).unwrap(),
            vec![CpuEfficiencyClass {
                value: 7,
                logical_processors: 2,
            }]
        );
    }

    #[test]
    fn parser_accepts_an_empty_result() {
        assert!(parse_cpu_efficiency_classes(&[]).unwrap().is_empty());
    }

    #[test]
    fn parser_rejects_malformed_records() {
        assert!(parse_cpu_efficiency_classes(&[0; 7]).is_err());
        let mut invalid_size = vec![0_u8; 8];
        invalid_size[0..4].copy_from_slice(&7_u32.to_le_bytes());
        assert!(parse_cpu_efficiency_classes(&invalid_size).is_err());
        assert!(parse_cpu_efficiency_classes(&record(18, 0, 0)).is_err());

        let mut oversized = record(32, 0, 0);
        oversized[0..4].copy_from_slice(&64_u32.to_le_bytes());
        assert!(parse_cpu_efficiency_classes(&oversized).is_err());

        let mut trailing = record(32, 0, 0);
        trailing.extend([0; 3]);
        assert!(parse_cpu_efficiency_classes(&trailing).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn live_cpu_topology_query_returns_logical_processors() {
        let classes = read_cpu_efficiency_classes().unwrap();

        assert!(!classes.is_empty());
        assert!(classes.iter().all(|class| class.logical_processors > 0));
        assert!(classes.windows(2).all(|pair| pair[0].value < pair[1].value));
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::sync::Mutex;

    pub(crate) struct MockPowerApi {
        pub plans: Mutex<Vec<PowerPlan>>,
        pub active_guid: Mutex<String>,
        pub battery: BatteryStatus,
        pub cpu_info: CpuInfo,
        pub cpu_frequency: CpuFrequencySample,
        pub processor_settings: Mutex<std::collections::BTreeMap<String, PlanProcessorSettings>>,
    }

    impl MockPowerApi {
        pub(crate) fn new() -> Self {
            Self {
                plans: Mutex::new(vec![
                    PowerPlan {
                        guid: "balanced-guid".into(),
                        name: "Balanced".into(),
                    },
                    PowerPlan {
                        guid: "perf-guid".into(),
                        name: "High Performance".into(),
                    },
                ]),
                active_guid: Mutex::new("balanced-guid".into()),
                battery: BatteryStatus::default(),
                cpu_info: CpuInfo {
                    manufacturer: "GenuineIntel".into(),
                    brand: "Test CPU @ 3.50GHz".into(),
                    base_mhz: Some(3500),
                    cores: Some(8),
                    logical_processors: Some(16),
                    efficiency_classes: vec![CpuEfficiencyClass {
                        value: 0,
                        logical_processors: 16,
                    }],
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
            Ok(self.plans.lock().unwrap().clone())
        }
        fn duplicate_ultimate_performance(&self) -> Result<PowerPlan> {
            let plan = PowerPlan {
                guid: "ultimate-perf-guid".into(),
                name: "Ultimate Performance".into(),
            };
            self.plans.lock().unwrap().push(plan.clone());
            Ok(plan)
        }
        fn delete_plan(&self, guid: &str) -> Result<()> {
            self.plans.lock().unwrap().retain(|plan| plan.guid != guid);
            self.processor_settings.lock().unwrap().remove(guid);
            Ok(())
        }
        fn get_active_plan(&self) -> Result<PowerPlan> {
            let guid = self.active_guid.lock().unwrap().clone();
            let plan = self
                .plans
                .lock()
                .unwrap()
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
        fn apply_processor_preset(
            &self,
            guid: &str,
            recommendation: ProcessorPresetRecommendation,
        ) -> Result<()> {
            let mut settings = self.processor_settings.lock().unwrap();
            let entry = settings.entry(guid.to_string()).or_default();
            entry.min_percent = present_limit(recommendation.min_percent);
            entry.max_percent = present_limit(recommendation.max_percent);
            if let Some(boost_mode) = recommendation.boost_mode {
                entry.boost_mode = present_limit(boost_mode);
            }
            if let Some(core_parking) = recommendation.core_parking_min_cores_percent {
                entry.core_parking_min_cores_percent = present_limit(core_parking);
            }
            if let Some(latency_hint_perf) = recommendation.latency_hint_perf {
                entry.latency_hint_perf = present_limit(latency_hint_perf);
            }
            if let Some(idle_promote) = recommendation.idle_promote_threshold {
                entry.idle_promote_threshold = present_limit(idle_promote);
            }
            if let Some(class1) = recommendation.class1 {
                entry.class1_min_percent = present_limit(class1.min_percent);
                entry.class1_max_percent = present_limit(class1.max_percent);
                if let Some(core_parking) = class1.core_parking_min_cores_percent {
                    entry.class1_core_parking_min_cores_percent = present_limit(core_parking);
                }
            }
            Ok(())
        }
    }

    fn present_limit(value: u32) -> ProcessorLimit {
        ProcessorLimit {
            ac: Some(value),
            dc: Some(value),
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
        fn test_mock_duplicate_and_delete_ultimate_performance() {
            let api = MockPowerApi::new();

            let plan = api.duplicate_ultimate_performance().unwrap();
            assert_eq!(plan.guid, "ultimate-perf-guid");
            assert_eq!(plan.name, "Ultimate Performance");
            assert!(api
                .enumerate_plans()
                .unwrap()
                .iter()
                .any(|candidate| candidate == &plan));

            api.delete_plan(&plan.guid).unwrap();
            assert!(!api
                .enumerate_plans()
                .unwrap()
                .iter()
                .any(|candidate| candidate.guid == plan.guid));
        }

        #[test]
        fn test_mock_apply_processor_preset_writes_advanced_settings() {
            let api = MockPowerApi::new();
            let recommendation = ProcessorPresetRecommendation {
                min_percent: 100,
                max_percent: 100,
                boost_mode: Some(2),
                core_parking_min_cores_percent: Some(100),
                latency_hint_perf: None,
                idle_promote_threshold: None,
                class1: Some(crate::types::ProcessorClassRecommendation {
                    min_percent: 80,
                    max_percent: 100,
                    core_parking_min_cores_percent: Some(100),
                }),
            };

            api.apply_processor_preset("perf-guid", recommendation)
                .unwrap();
            let settings = api.read_plan_processor_settings("perf-guid").unwrap();

            assert_eq!(settings.min_percent.ac, Some(100));
            assert_eq!(settings.max_percent.dc, Some(100));
            assert_eq!(settings.boost_mode.ac, Some(2));
            assert_eq!(settings.boost_mode.dc, Some(2));
            assert_eq!(settings.core_parking_min_cores_percent.ac, Some(100));
            assert_eq!(settings.core_parking_min_cores_percent.dc, Some(100));
            assert_eq!(settings.class1_min_percent.ac, Some(80));
            assert_eq!(settings.class1_max_percent.dc, Some(100));
            assert_eq!(settings.class1_core_parking_min_cores_percent.ac, Some(100));
        }

        #[test]
        fn test_mock_apply_processor_preset_preserves_omitted_optional_settings() {
            let api = MockPowerApi::new();
            api.processor_settings.lock().unwrap().insert(
                "balanced-guid".into(),
                PlanProcessorSettings {
                    boost_mode: present_limit(4),
                    core_parking_min_cores_percent: present_limit(35),
                    class1_min_percent: present_limit(11),
                    ..PlanProcessorSettings::default()
                },
            );

            api.apply_processor_preset(
                "balanced-guid",
                ProcessorPresetRecommendation {
                    min_percent: 5,
                    max_percent: 99,
                    boost_mode: None,
                    core_parking_min_cores_percent: None,
                    latency_hint_perf: None,
                    idle_promote_threshold: None,
                    class1: None,
                },
            )
            .unwrap();

            let settings = api.read_plan_processor_settings("balanced-guid").unwrap();
            assert_eq!(settings.boost_mode.ac, Some(4));
            assert_eq!(settings.core_parking_min_cores_percent.dc, Some(35));
            assert_eq!(settings.class1_min_percent.ac, Some(11));
        }
    }
}
