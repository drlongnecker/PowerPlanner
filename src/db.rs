// src/db.rs
use crate::types::{CpuHistoryEnergyEstimate, CpuHistoryPoint, PowerEvent};
use anyhow::Result;
use chrono::{Local, TimeZone};
use rusqlite::{params, Connection};
use std::path::PathBuf;

const DASHBOARD_SAMPLE_RETENTION_DAYS: i64 = 60;

pub fn db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PowerPlanner")
        .join("history.db")
}

pub fn open() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS power_events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            ts          INTEGER NOT NULL,
            plan_guid   TEXT    NOT NULL,
            plan_name   TEXT    NOT NULL,
            trigger     TEXT    NOT NULL,
            on_battery  INTEGER NOT NULL,
            battery_pct INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_power_events_ts ON power_events(ts);
        CREATE TABLE IF NOT EXISTS dashboard_samples (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            ts              INTEGER NOT NULL,
            average_percent REAL    NOT NULL,
            current_mhz     INTEGER,
            base_mhz        INTEGER,
            plan_kind       INTEGER NOT NULL,
            plan_name       TEXT    NOT NULL,
            trigger         TEXT    NOT NULL,
            estimated_watts REAL,
            estimated_kwh REAL,
            estimated_cost_usd REAL,
            baseline_watts REAL,
            baseline_cost_usd REAL,
            estimated_savings_usd REAL
        );
        CREATE INDEX IF NOT EXISTS idx_dashboard_samples_ts ON dashboard_samples(ts);
    ",
    )?;
    add_dashboard_energy_columns(conn)?;
    Ok(())
}

fn add_dashboard_energy_columns(conn: &Connection) -> Result<()> {
    for (name, ty) in [
        ("current_mhz", "INTEGER"),
        ("base_mhz", "INTEGER"),
        ("estimated_watts", "REAL"),
        ("estimated_kwh", "REAL"),
        ("estimated_cost_usd", "REAL"),
        ("baseline_watts", "REAL"),
        ("baseline_cost_usd", "REAL"),
        ("estimated_savings_usd", "REAL"),
    ] {
        if !dashboard_samples_has_column(conn, name)? {
            conn.execute(
                &format!("ALTER TABLE dashboard_samples ADD COLUMN {} {}", name, ty),
                [],
            )?;
        }
    }
    Ok(())
}

fn dashboard_samples_has_column(conn: &Connection, name: &str) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(dashboard_samples)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == name {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn insert_event(conn: &Connection, event: &PowerEvent) -> Result<()> {
    conn.execute(
        "INSERT INTO power_events (ts, plan_guid, plan_name, trigger, on_battery, battery_pct)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event.ts.timestamp_millis(),
            event.plan_guid,
            event.plan_name,
            event.trigger,
            event.on_battery as i32,
            event.battery_pct.map(|p| p as i32),
        ],
    )?;
    Ok(())
}

pub fn query_recent(conn: &Connection, limit: usize) -> Result<Vec<PowerEvent>> {
    let mut stmt = conn.prepare(
        "SELECT ts, plan_guid, plan_name, trigger, on_battery, battery_pct
         FROM power_events ORDER BY ts DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        let ts_ms: i64 = row.get(0)?;
        let on_battery: i32 = row.get(4)?;
        let battery_pct: Option<i32> = row.get(5)?;
        Ok(PowerEvent {
            ts: Local.timestamp_millis_opt(ts_ms).unwrap(),
            plan_guid: row.get(1)?,
            plan_name: row.get(2)?,
            trigger: row.get(3)?,
            on_battery: on_battery != 0,
            battery_pct: battery_pct.map(|p| p as u8),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn insert_dashboard_sample(conn: &Connection, sample: &CpuHistoryPoint) -> Result<()> {
    conn.execute(
        "INSERT INTO dashboard_samples (
             ts, average_percent, current_mhz, base_mhz, plan_kind, plan_name, trigger,
             estimated_watts, estimated_kwh, estimated_cost_usd,
             baseline_watts, baseline_cost_usd, estimated_savings_usd
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            sample.ts.timestamp_millis(),
            sample.average_percent,
            sample.current_mhz.map(|mhz| mhz as i64),
            sample.base_mhz.map(|mhz| mhz as i64),
            sample.plan_kind.storage_value(),
            sample.plan_name,
            sample.trigger,
            sample.energy.map(|energy| energy.estimated_watts),
            sample.energy.map(|energy| energy.estimated_kwh),
            sample.energy.map(|energy| energy.estimated_cost_usd),
            sample.energy.map(|energy| energy.baseline_watts),
            sample.energy.map(|energy| energy.baseline_cost_usd),
            sample.energy.map(|energy| energy.estimated_savings_usd),
        ],
    )?;
    prune_dashboard_samples(conn, sample.ts)?;
    Ok(())
}

pub fn query_dashboard_samples_recent(
    conn: &Connection,
    minutes: i64,
) -> Result<Vec<CpuHistoryPoint>> {
    let threshold = Local::now() - chrono::Duration::minutes(minutes.max(1));
    let mut stmt = conn.prepare(
        "SELECT ts, average_percent, current_mhz, base_mhz, plan_kind, plan_name, trigger,
                estimated_watts, estimated_kwh, estimated_cost_usd,
                baseline_watts, baseline_cost_usd, estimated_savings_usd
         FROM dashboard_samples
         WHERE ts >= ?1
         ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map(params![threshold.timestamp_millis()], |row| {
        Ok(CpuHistoryPoint {
            ts: Local.timestamp_millis_opt(row.get(0)?).unwrap(),
            average_percent: row.get(1)?,
            current_mhz: optional_mhz(row.get(2)?),
            base_mhz: optional_mhz(row.get(3)?),
            plan_kind: crate::types::CpuHistoryPlanKind::from_storage(row.get(4)?),
            plan_name: row.get(5)?,
            trigger: row.get(6)?,
            energy: row_energy(row)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn query_all_dashboard_samples(conn: &Connection) -> Result<Vec<CpuHistoryPoint>> {
    let mut stmt = conn.prepare(
        "SELECT ts, average_percent, current_mhz, base_mhz, plan_kind, plan_name, trigger,
                estimated_watts, estimated_kwh, estimated_cost_usd,
                baseline_watts, baseline_cost_usd, estimated_savings_usd
         FROM dashboard_samples
         ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CpuHistoryPoint {
            ts: Local.timestamp_millis_opt(row.get(0)?).unwrap(),
            average_percent: row.get(1)?,
            current_mhz: optional_mhz(row.get(2)?),
            base_mhz: optional_mhz(row.get(3)?),
            plan_kind: crate::types::CpuHistoryPlanKind::from_storage(row.get(4)?),
            plan_name: row.get(5)?,
            trigger: row.get(6)?,
            energy: row_energy(row)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_energy(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<CpuHistoryEnergyEstimate>> {
    let estimated_watts: Option<f64> = row.get(7)?;
    let Some(estimated_watts) = estimated_watts else {
        return Ok(None);
    };
    Ok(Some(CpuHistoryEnergyEstimate {
        estimated_watts,
        estimated_kwh: row.get::<_, Option<f64>>(8)?.unwrap_or_default(),
        estimated_cost_usd: row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
        baseline_watts: row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
        baseline_cost_usd: row.get::<_, Option<f64>>(11)?.unwrap_or_default(),
        estimated_savings_usd: row.get::<_, Option<f64>>(12)?.unwrap_or_default(),
    }))
}

fn optional_mhz(value: Option<i64>) -> Option<u32> {
    value.and_then(|mhz| u32::try_from(mhz).ok())
}

fn prune_dashboard_samples(conn: &Connection, now: chrono::DateTime<Local>) -> Result<usize> {
    let cutoff = now - chrono::Duration::days(DASHBOARD_SAMPLE_RETENTION_DAYS);
    conn.execute(
        "DELETE FROM dashboard_samples WHERE ts < ?1",
        params![cutoff.timestamp_millis()],
    )
    .map_err(Into::into)
}

pub fn export_csv(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT ts, plan_name, trigger, on_battery, battery_pct
         FROM power_events ORDER BY ts DESC",
    )?;
    let mut out = String::from("timestamp,plan_name,trigger,on_battery,battery_pct\n");
    let rows = stmt.query_map([], |row| {
        let ts_ms: i64 = row.get(0)?;
        let on_battery: i32 = row.get(3)?;
        let battery_pct: Option<i32> = row.get(4)?;
        Ok((
            ts_ms,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            on_battery,
            battery_pct,
        ))
    })?;
    for row in rows {
        let (ts_ms, plan, trigger, on_bat, pct) = row?;
        let ts = Local.timestamp_millis_opt(ts_ms).unwrap();
        let pct_str = pct.map(|p| p.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            ts.format("%Y-%m-%d %H:%M:%S"),
            plan,
            trigger,
            on_bat,
            pct_str
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CpuHistoryPlanKind;
    use chrono::{Duration, Local};

    fn in_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    fn make_event(plan_name: &str, trigger: &str) -> PowerEvent {
        PowerEvent {
            ts: Local::now(),
            plan_guid: "test-guid".to_string(),
            plan_name: plan_name.to_string(),
            trigger: trigger.to_string(),
            on_battery: false,
            battery_pct: None,
        }
    }

    fn make_dashboard_sample(
        ts: chrono::DateTime<Local>,
        plan_name: &str,
        trigger: &str,
        average_percent: f32,
        plan_kind: CpuHistoryPlanKind,
    ) -> CpuHistoryPoint {
        CpuHistoryPoint {
            ts,
            average_percent,
            current_mhz: None,
            base_mhz: None,
            plan_kind,
            plan_name: plan_name.to_string(),
            trigger: trigger.to_string(),
            energy: None,
        }
    }

    #[test]
    fn test_migrate_creates_table() {
        let conn = in_memory();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM power_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_query_recent() {
        let conn = in_memory();
        insert_event(&conn, &make_event("Balanced", "startup")).unwrap();
        insert_event(&conn, &make_event("High Performance", "cmake.exe")).unwrap();
        let events = query_recent(&conn, 10).unwrap();
        assert_eq!(events.len(), 2);
        // Most recent first
        assert_eq!(events[0].plan_name, "High Performance");
        assert_eq!(events[0].trigger, "cmake.exe");
    }

    #[test]
    fn test_query_recent_respects_limit() {
        let conn = in_memory();
        for i in 0..5 {
            insert_event(&conn, &make_event("Balanced", &format!("t{}", i))).unwrap();
        }
        assert_eq!(query_recent(&conn, 3).unwrap().len(), 3);
    }

    #[test]
    fn test_export_csv_has_header_and_data() {
        let conn = in_memory();
        insert_event(&conn, &make_event("Balanced", "startup")).unwrap();
        let csv = export_csv(&conn).unwrap();
        assert!(csv.starts_with("timestamp,plan_name,trigger,on_battery,battery_pct\n"));
        assert!(csv.contains("Balanced"));
        assert!(csv.contains("startup"));
    }

    #[test]
    fn test_insert_and_query_recent_dashboard_samples() {
        let conn = in_memory();
        let now = Local::now();
        insert_dashboard_sample(
            &conn,
            &make_dashboard_sample(
                now - Duration::minutes(20),
                "Balanced",
                "startup",
                10.0,
                CpuHistoryPlanKind::Standard,
            ),
        )
        .unwrap();
        insert_dashboard_sample(
            &conn,
            &make_dashboard_sample(
                now - Duration::minutes(5),
                "High Performance",
                "rustc.exe",
                33.0,
                CpuHistoryPlanKind::Performance,
            ),
        )
        .unwrap();

        let recent = query_dashboard_samples_recent(&conn, 15).unwrap();

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].plan_name, "High Performance");
        assert_eq!(recent[0].plan_kind, CpuHistoryPlanKind::Performance);
    }

    #[test]
    fn test_query_all_dashboard_samples_returns_ordered_rows() {
        let conn = in_memory();
        let now = Local::now();
        insert_dashboard_sample(
            &conn,
            &make_dashboard_sample(
                now - Duration::minutes(2),
                "Balanced",
                "startup",
                9.0,
                CpuHistoryPlanKind::Standard,
            ),
        )
        .unwrap();
        insert_dashboard_sample(
            &conn,
            &make_dashboard_sample(
                now - Duration::minutes(1),
                "Power Saver",
                "idle + quiet cpu",
                4.0,
                CpuHistoryPlanKind::LowPower,
            ),
        )
        .unwrap();

        let all = query_all_dashboard_samples(&conn).unwrap();

        assert_eq!(all.len(), 2);
        assert!(all[0].ts <= all[1].ts);
        assert_eq!(all[1].plan_kind, CpuHistoryPlanKind::LowPower);
    }

    #[test]
    fn test_insert_dashboard_sample_prunes_rows_older_than_retention_window() {
        let conn = in_memory();
        let now = Local::now();
        conn.execute(
            "INSERT INTO dashboard_samples (ts, average_percent, plan_kind, plan_name, trigger)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                (now - Duration::days(61)).timestamp_millis(),
                3.0_f32,
                CpuHistoryPlanKind::LowPower.storage_value(),
                "Power Saver",
                "idle",
            ],
        )
        .unwrap();

        insert_dashboard_sample(
            &conn,
            &make_dashboard_sample(
                now,
                "Balanced",
                "startup",
                8.0,
                CpuHistoryPlanKind::Standard,
            ),
        )
        .unwrap();

        let all = query_all_dashboard_samples(&conn).unwrap();

        assert_eq!(all.len(), 1);
        assert_eq!(all[0].plan_name, "Balanced");
    }

    #[test]
    fn test_migrate_adds_energy_estimate_columns() {
        let conn = in_memory();

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(dashboard_samples)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(columns.contains(&"estimated_watts".to_string()));
        assert!(columns.contains(&"current_mhz".to_string()));
        assert!(columns.contains(&"base_mhz".to_string()));
        assert!(columns.contains(&"estimated_kwh".to_string()));
        assert!(columns.contains(&"estimated_cost_usd".to_string()));
        assert!(columns.contains(&"baseline_watts".to_string()));
        assert!(columns.contains(&"baseline_cost_usd".to_string()));
        assert!(columns.contains(&"estimated_savings_usd".to_string()));
    }

    #[test]
    fn test_dashboard_samples_roundtrip_cpu_speed_context() {
        let conn = in_memory();
        let mut sample = make_dashboard_sample(
            Local::now(),
            "Balanced",
            "standard",
            16.0,
            CpuHistoryPlanKind::Standard,
        );
        sample.current_mhz = Some(3200);
        sample.base_mhz = Some(3500);

        insert_dashboard_sample(&conn, &sample).unwrap();

        let samples = query_all_dashboard_samples(&conn).unwrap();
        assert_eq!(samples[0].current_mhz, Some(3200));
        assert_eq!(samples[0].base_mhz, Some(3500));
        assert_eq!(samples[0].average_percent, 16.0);
    }
}
