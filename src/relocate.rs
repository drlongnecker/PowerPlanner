// src/relocate.rs
use anyhow::Result;
use std::path::{Path, PathBuf};

pub(crate) enum RelocateAction {
    NotNeeded,
    Needed { suggested: PathBuf },
}

pub(crate) fn check() -> RelocateAction {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
    else {
        return RelocateAction::Needed {
            suggested: suggested_path(),
        };
    };

    if is_writable(&exe_dir) {
        RelocateAction::NotNeeded
    } else {
        RelocateAction::Needed {
            suggested: suggested_path(),
        }
    }
}

pub(crate) fn is_writable(dir: &Path) -> bool {
    let test = dir.join(".powerplanner_write_test");
    match std::fs::write(&test, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&test);
            true
        }
        Err(_) => false,
    }
}

pub(crate) fn suggested_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PowerPlanner")
        .join("PowerPlanner.exe")
}

pub(crate) fn copy_exe_to(destination: &Path) -> Result<()> {
    let current = std::env::current_exe()?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&current, destination)?;
    Ok(())
}

pub(crate) fn launch_detached(path: &Path) -> Result<()> {
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
