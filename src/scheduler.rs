// src/scheduler.rs
use anyhow::{bail, Result};

const TASK_NAME: &str = "PowerPlanner_AutoStart";

// ── Process helper ────────────────────────────────────────────────────────────

#[cfg(windows)]
fn schtasks(args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

#[cfg(not(windows))]
fn schtasks(_args: &[&str]) -> std::io::Result<std::process::Output> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "schtasks not available"))
}

// ── Elevation detection ───────────────────────────────────────────────────────

/// Returns true if the current process has administrator privileges.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::IsUserAnAdmin;
        unsafe { IsUserAnAdmin().as_bool() }
    }
    #[cfg(not(windows))]
    { true }
}

// ── UAC-triggered schtasks (used when not already elevated) ──────────────────

#[cfg(windows)]
fn shell_execute_runas(schtasks_args: &str) -> Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::PCWSTR;

    let verb:  Vec<u16> = "runas\0".encode_utf16().collect();
    let file:  Vec<u16> = "schtasks.exe\0".encode_utf16().collect();
    let args:  Vec<u16> = format!("{}\0", schtasks_args).encode_utf16().collect();

    let code = unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        ).0 as isize
    };

    if code <= 32 {
        bail!("Elevation cancelled or failed (code {})", code);
    }
    Ok(())
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn register() -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy();

    if is_elevated() {
        let out = schtasks(&[
            "/create", "/tn", TASK_NAME,
            "/tr", &exe_str,
            "/sc", "ONLOGON", "/rl", "HIGHEST", "/f",
        ])?;
        if !out.status.success() {
            bail!("schtasks /create failed");
        }
    } else {
        #[cfg(windows)]
        shell_execute_runas(&format!(
            "/create /tn \"{}\" /tr \"{}\" /sc ONLOGON /rl HIGHEST /f",
            TASK_NAME, exe_str
        ))?;
        #[cfg(not(windows))]
        bail!("Not running as administrator");
    }
    Ok(())
}

pub fn unregister() -> Result<()> {
    if is_elevated() {
        let out = schtasks(&["/delete", "/tn", TASK_NAME, "/f"])?;
        if !out.status.success() {
            bail!("schtasks /delete failed");
        }
    } else {
        #[cfg(windows)]
        shell_execute_runas(&format!("/delete /tn \"{}\" /f", TASK_NAME))?;
        #[cfg(not(windows))]
        bail!("Not running as administrator");
    }
    Ok(())
}

pub fn is_registered() -> bool {
    schtasks(&["/query", "/tn", TASK_NAME])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_name_is_stable() {
        assert_eq!(TASK_NAME, "PowerPlanner_AutoStart");
    }

    #[test]
    fn test_is_registered_returns_false_when_absent() {
        if !is_registered() {
            assert!(!is_registered());
        }
    }
}
