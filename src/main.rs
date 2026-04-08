// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod db;
mod monitor;
mod power;
mod relocate;
mod scheduler;
mod tray;
mod types;
mod ui;

use std::sync::{Arc, OnceLock, RwLock, mpsc};
use power::{PowerApi, WindowsPowerApi};

fn main() {
    setup_logging();
    log::info!("PowerPlanner starting");

    // Step 1: Relocate check
    if let relocate::RelocateAction::Needed { suggested } = relocate::check() {
        if prompt_relocate(&suggested) {
            if relocate::copy_exe_to(&suggested).is_ok() {
                let _ = relocate::launch_detached(&suggested);
            }
        }
        return;
    }

    // Step 2: Config
    let (mut config, is_first_run) = config::load_or_default();

    // Step 3: Database
    let db_conn = db::open().expect("Failed to open history database");

    // Step 4: Power API
    let power: Arc<dyn PowerApi> = Arc::new(WindowsPowerApi);

    // Step 5: Enumerate plans; detect initial plan
    let available_plans = power.enumerate_plans().unwrap_or_default();
    if is_first_run {
        if let Ok(active) = power.get_active_plan() {
            config.general.idle_plan_guid = active.guid;
        }
        let _ = config::save(&config);
        log::info!("First run — config written with current plan as idle plan");
    }

    // Step 5b: Sync autostart registration state (once at startup, avoids per-frame subprocess)
    config.autostart.registered = scheduler::is_registered();

    // Step 6: Validate stored plan GUIDs
    let guids: Vec<&str> = available_plans.iter().map(|p| p.guid.as_str()).collect();
    if !guids.contains(&config.general.idle_plan_guid.as_str()) {
        log::warn!("Idle plan GUID not found — falling back to Balanced");
        config.general.idle_plan_guid = "381b4222-f694-41f0-9685-ff5bb260df2e".into();
    }

    // Step 7: Build shared AppState; pre-populate history from DB so it survives restarts
    let initial_plan = power.get_active_plan().ok();
    let initial_events: std::collections::VecDeque<types::PowerEvent> =
        db::query_recent(&db_conn, 50).unwrap_or_default().into_iter().collect();
    let app_state = Arc::new(RwLock::new(types::AppState {
        available_plans: available_plans.clone(),
        current_plan: initial_plan,
        recent_events: initial_events,
        ..Default::default()
    }));

    // Step 8: Spawn monitor thread
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let repaint_ctx: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
    {
        let state = Arc::clone(&app_state);
        let power_clone = Arc::clone(&power);
        let cfg = config.clone();
        let ctx_holder = Arc::clone(&repaint_ctx);
        std::thread::spawn(move || {
            monitor::run(cfg, state, cmd_rx, db_conn, power_clone, ctx_holder);
        });
    }

    // Step 9: Log startup event
    if let Ok(conn) = db::open() {
        let plan_name = power.get_active_plan()
            .map(|p| p.name)
            .unwrap_or_else(|_| "Unknown".into());
        let _ = db::insert_event(&conn, &types::PowerEvent {
            ts: chrono::Local::now(),
            plan_guid: config.general.idle_plan_guid.clone(),
            plan_name,
            trigger: "startup".into(),
            on_battery: false,
            battery_pct: None,
        });
    }

    // Step 10: Build tray
    let tray = tray::Tray::new().ok();

    // Step 11: Run egui
    const LOGO_PNG: &[u8] = include_bytes!("../planner.png");
    let icon = image::load_from_memory(LOGO_PNG).ok().map(|img| {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        std::sync::Arc::new(egui::IconData { rgba: rgba.into_raw(), width, height })
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PowerPlanner")
        .with_inner_size([800.0, 500.0])
        .with_min_inner_size([600.0, 400.0]);
    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "PowerPlanner",
        options,
        Box::new(move |cc| {
            let _ = repaint_ctx.set(cc.egui_ctx.clone());
            Ok(Box::new(app::PowerPlannerApp::new(app_state, cmd_tx, config, tray)))
        }),
    ).unwrap();
}

fn setup_logging() {
    let log_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("PowerPlanner")
        .join("powerplanner.log");

    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stdout())
        .chain({
            match fern::log_file(&log_path) {
                Ok(f) => Box::new(f) as Box<dyn std::io::Write + Send>,
                Err(_) => Box::new(std::io::sink()) as Box<dyn std::io::Write + Send>,
            }
        })
        .apply()
        .ok();
}

fn prompt_relocate(suggested: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
        };
        use windows::core::PCWSTR;

        let msg = format!(
            "PowerPlanner needs a writable location for settings.\n\nMove to {}?",
            suggested.display()
        );
        let msg_w: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
        let title_w: Vec<u16> = "PowerPlanner".encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            MessageBoxW(
                None,
                PCWSTR(msg_w.as_ptr()),
                PCWSTR(title_w.as_ptr()),
                MB_YESNO | MB_ICONQUESTION,
            ) == IDYES
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("Would relocate to: {:?}", suggested);
        false
    }
}
