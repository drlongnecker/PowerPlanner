// src/tray.rs
use anyhow::Result;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

const LOGO_PNG: &[u8] = include_bytes!("../planner.png");
const STARTUP_TRAY_ATTEMPTS: usize = 15;
const STARTUP_TRAY_RETRY_DELAY: Duration = Duration::from_secs(1);

pub struct Tray {
    pub show_item_id: tray_icon::menu::MenuId,
    pub balanced_item_id: tray_icon::menu::MenuId,
    pub perf_item_id: tray_icon::menu::MenuId,
    pub resume_item_id: tray_icon::menu::MenuId,
    pub exit_item_id: tray_icon::menu::MenuId,
    _icon: TrayIcon,
}

impl Tray {
    pub fn new_with_startup_retry() -> Result<Self> {
        retry_with_delay(
            STARTUP_TRAY_ATTEMPTS,
            STARTUP_TRAY_RETRY_DELAY,
            Self::new,
            std::thread::sleep,
        )
    }

    pub fn new() -> Result<Self> {
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
            _icon: tray,
        })
    }

    pub fn set_tooltip(&self, text: &str) {
        let _ = self._icon.set_tooltip(Some(text));
    }
}

fn retry_with_delay<T, E>(
    max_attempts: usize,
    delay: Duration,
    mut operation: impl FnMut() -> std::result::Result<T, E>,
    mut sleep: impl FnMut(Duration),
) -> std::result::Result<T, E> {
    assert!(max_attempts > 0);

    for _ in 1..max_attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(_) => sleep(delay),
        }
    }

    operation()
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
    use std::cell::Cell;

    #[test]
    fn tray_startup_retry_returns_immediately_on_success() {
        let attempts = Cell::new(0);
        let sleeps = Cell::new(0);

        let result = retry_with_delay(
            3,
            Duration::from_secs(1),
            || {
                attempts.set(attempts.get() + 1);
                Ok::<_, &'static str>("tray")
            },
            |_| sleeps.set(sleeps.get() + 1),
        );

        assert_eq!(result, Ok("tray"));
        assert_eq!(attempts.get(), 1);
        assert_eq!(sleeps.get(), 0);
    }

    #[test]
    fn tray_startup_retry_recovers_from_transient_failures() {
        let attempts = Cell::new(0);
        let sleeps = Cell::new(0);

        let result = retry_with_delay(
            4,
            Duration::from_secs(1),
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 3 {
                    Err("shell unavailable")
                } else {
                    Ok("tray")
                }
            },
            |_| sleeps.set(sleeps.get() + 1),
        );

        assert_eq!(result, Ok("tray"));
        assert_eq!(attempts.get(), 3);
        assert_eq!(sleeps.get(), 2);
    }

    #[test]
    fn tray_startup_retry_returns_the_final_error() {
        let attempts = Cell::new(0);
        let sleeps = Cell::new(0);

        let result = retry_with_delay(
            3,
            Duration::from_secs(1),
            || {
                attempts.set(attempts.get() + 1);
                Err::<(), _>(attempts.get())
            },
            |_| sleeps.set(sleeps.get() + 1),
        );

        assert_eq!(result, Err(3));
        assert_eq!(attempts.get(), 3);
        assert_eq!(sleeps.get(), 2);
    }
}
