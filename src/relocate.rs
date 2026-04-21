// src/relocate.rs
use anyhow::Result;
use std::path::{Path, PathBuf};

pub enum RelocateAction {
    NotNeeded,
    Needed { suggested: PathBuf },
}

pub fn check() -> RelocateAction {
    let exe_dir = match std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        Some(d) => d,
        None => {
            return RelocateAction::Needed {
                suggested: suggested_path(),
            }
        }
    };

    if is_writable(&exe_dir) {
        RelocateAction::NotNeeded
    } else {
        RelocateAction::Needed {
            suggested: suggested_path(),
        }
    }
}

pub fn is_writable(dir: &Path) -> bool {
    let test = dir.join(".powerplanner_write_test");
    match std::fs::write(&test, b"x") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test);
            true
        }
        Err(_) => false,
    }
}

pub fn suggested_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PowerPlanner")
        .join("PowerPlanner.exe")
}

pub fn copy_exe_to(destination: &Path) -> Result<()> {
    let current = std::env::current_exe()?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&current, destination)?;
    Ok(())
}

pub fn launch_detached(path: &Path) -> Result<()> {
    std::process::Command::new(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_is_writable() {
        assert!(is_writable(&std::env::temp_dir()));
    }

    #[test]
    fn test_nonexistent_dir_is_not_writable() {
        assert!(!is_writable(Path::new("Z:\\nonexistent_xyz_123")));
    }

    #[test]
    fn test_suggested_path_ends_with_exe() {
        assert!(suggested_path()
            .to_string_lossy()
            .ends_with("PowerPlanner.exe"));
    }
}
