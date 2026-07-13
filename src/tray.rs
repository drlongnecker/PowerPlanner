// src/tray.rs
use anyhow::Result;
use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

const LOGO_PNG: &[u8] = include_bytes!("../planner.png");
const STARTUP_TRAY_CHECK_INTERVAL: Duration = Duration::from_millis(500);
const STARTUP_TRAY_SETTLE_DURATION: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupTrayDecision {
    Wait,
    Build,
}

pub(crate) struct TrayStartup {
    next_check: Instant,
    ready_since: Option<Instant>,
    finished: bool,
}

impl TrayStartup {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            next_check: now,
            ready_since: None,
            finished: false,
        }
    }

    pub(crate) fn finished() -> Self {
        Self {
            next_check: Instant::now(),
            ready_since: None,
            finished: true,
        }
    }

    pub(crate) fn try_build(&mut self, now: Instant) -> Option<Result<Tray>> {
        if self.finished || now < self.next_check {
            return None;
        }

        self.next_check = now + STARTUP_TRAY_CHECK_INTERVAL;
        if startup_tray_decision(
            now,
            notification_area_ready(),
            &mut self.ready_since,
            STARTUP_TRAY_SETTLE_DURATION,
        ) == StartupTrayDecision::Wait
        {
            return None;
        }

        self.finished = true;
        Some(Tray::new())
    }
}

pub(crate) struct Tray {
    pub show_item_id: tray_icon::menu::MenuId,
    pub balanced_item_id: tray_icon::menu::MenuId,
    pub perf_item_id: tray_icon::menu::MenuId,
    pub resume_item_id: tray_icon::menu::MenuId,
    pub exit_item_id: tray_icon::menu::MenuId,
    icon: TrayIcon,
}

impl Tray {
    pub(crate) fn new() -> Result<Self> {
        let show = MenuItem::new("Show Window", true, None);
        let balanced = MenuItem::new("Force Balanced", true, None);
        let perf = MenuItem::new("Force High Performance", true, None);
        let resume = MenuItem::new("Resume Auto", true, None);
        let exit = MenuItem::new("Exit", true, None);

        let show_id = show.id().clone();
        let balanced_id = balanced.id().clone();
        let perf_id = perf.id().clone();
        let resume_id = resume.id().clone();
        let exit_id = exit.id().clone();

        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();

        let menu = Menu::with_items(&[&show, &sep1, &balanced, &perf, &resume, &sep2, &exit])?;

        let icon = load_icon();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("PowerPlanner")
            .with_icon(icon)
            .build()?;

        Ok(Self {
            show_item_id: show_id,
            balanced_item_id: balanced_id,
            perf_item_id: perf_id,
            resume_item_id: resume_id,
            exit_item_id: exit_id,
            icon: tray,
        })
    }

    pub(crate) fn set_tooltip(&self, text: &str) {
        let _ = self.icon.set_tooltip(Some(text));
    }
}

fn startup_tray_decision(
    now: Instant,
    ready: bool,
    ready_since: &mut Option<Instant>,
    settle_duration: Duration,
) -> StartupTrayDecision {
    if !ready {
        *ready_since = None;
        return StartupTrayDecision::Wait;
    }

    let since = *ready_since.get_or_insert(now);
    if now.duration_since(since) >= settle_duration {
        StartupTrayDecision::Build
    } else {
        StartupTrayDecision::Wait
    }
}

#[cfg(windows)]
fn notification_area_ready() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW};

    let taskbar_class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
    let notification_area_class: Vec<u16> = "TrayNotifyWnd\0".encode_utf16().collect();

    unsafe {
        let Ok(taskbar) = FindWindowW(PCWSTR(taskbar_class.as_ptr()), PCWSTR::null()) else {
            return false;
        };

        FindWindowExW(
            taskbar,
            HWND::default(),
            PCWSTR(notification_area_class.as_ptr()),
            PCWSTR::null(),
        )
        .is_ok()
    }
}

#[cfg(not(windows))]
fn notification_area_ready() -> bool {
    true
}

fn load_icon() -> tray_icon::Icon {
    if let Ok(img) = image::load_from_memory(LOGO_PNG) {
        let img = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h) {
            return icon;
        }
    }
    // Fallback: plain gray square
    let rgba: Vec<u8> = (0..32 * 32)
        .flat_map(|_| [120u8, 120u8, 120u8, 255u8])
        .collect();
    tray_icon::Icon::from_rgba(rgba, 32, 32).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_startup_waits_for_stable_notification_area() {
        let start = Instant::now();
        let settle = Duration::from_secs(2);
        let mut ready_since = None;

        assert_eq!(
            startup_tray_decision(start, true, &mut ready_since, settle),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(
                start + Duration::from_millis(1_999),
                true,
                &mut ready_since,
                settle
            ),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(start + settle, true, &mut ready_since, settle),
            StartupTrayDecision::Build
        );
    }

    #[test]
    fn tray_startup_stability_resets_when_notification_area_disappears() {
        let start = Instant::now();
        let settle = Duration::from_secs(2);
        let mut ready_since = None;

        assert_eq!(
            startup_tray_decision(start, true, &mut ready_since, settle),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(
                start + Duration::from_secs(1),
                false,
                &mut ready_since,
                settle
            ),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(
                start + Duration::from_secs(2),
                true,
                &mut ready_since,
                settle
            ),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(
                start + Duration::from_secs(3),
                true,
                &mut ready_since,
                settle
            ),
            StartupTrayDecision::Wait
        );
        assert_eq!(
            startup_tray_decision(
                start + Duration::from_secs(4),
                true,
                &mut ready_since,
                settle
            ),
            StartupTrayDecision::Build
        );
    }

    #[test]
    fn tray_startup_finished_state_never_builds() {
        let mut startup = TrayStartup::finished();

        assert!(startup.try_build(Instant::now()).is_none());
    }
}
