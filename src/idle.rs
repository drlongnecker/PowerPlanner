use anyhow::{anyhow, Result};
use std::time::Duration;

pub trait IdleReader: Send + Sync {
    fn idle_duration(&self) -> Result<Duration>;
}

pub struct WindowsIdleReader;

#[cfg(windows)]
impl IdleReader for WindowsIdleReader {
    fn idle_duration(&self) -> Result<Duration> {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };

        unsafe {
            if !GetLastInputInfo(&mut info).as_bool() {
                return Err(anyhow!("GetLastInputInfo failed"));
            }
        }

        let now = unsafe { GetTickCount() };
        let elapsed_ms = now.saturating_sub(info.dwTime);
        Ok(Duration::from_millis(elapsed_ms as u64))
    }
}

#[cfg(not(windows))]
impl IdleReader for WindowsIdleReader {
    fn idle_duration(&self) -> Result<Duration> {
        Ok(Duration::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubIdleReader(Duration);

    impl IdleReader for StubIdleReader {
        fn idle_duration(&self) -> Result<Duration> {
            Ok(self.0)
        }
    }

    #[test]
    fn test_stub_idle_reader_returns_duration() {
        let reader = StubIdleReader(Duration::from_secs(42));
        assert_eq!(reader.idle_duration().unwrap(), Duration::from_secs(42));
    }
}
