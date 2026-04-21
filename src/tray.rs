// src/tray.rs
use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

const LOGO_PNG: &[u8] = include_bytes!("../planner.png");

pub struct Tray {
    pub show_item_id: tray_icon::menu::MenuId,
    pub balanced_item_id: tray_icon::menu::MenuId,
    pub perf_item_id: tray_icon::menu::MenuId,
    pub resume_item_id: tray_icon::menu::MenuId,
    pub exit_item_id: tray_icon::menu::MenuId,
    _icon: TrayIcon,
}

impl Tray {
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
