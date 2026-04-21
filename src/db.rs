// src/db.rs
use crate::types::PowerEvent;
use anyhow::Result;
use chrono::{Local, TimeZone};
use rusqlite::{params, Connection};
use std::path::PathBuf;

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
    ",
    )?;
    Ok(())
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
    use chrono::Local;

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
}
