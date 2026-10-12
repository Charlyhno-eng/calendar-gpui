use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use super::{Calendar, CalendarError, Result};

// Atomic saves include a disk sync, which can exceed half a second under load.
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Single-writer, atomic local storage. Keep this handle alive while using the document.
pub struct CalendarStore {
    path: PathBuf,
    _lock: File,
}

impl CalendarStore {
    /// Load a fresh snapshot and release the lock immediately.
    pub fn load(path: impl AsRef<Path>) -> Result<Calendar> {
        Self::open_wait(path.as_ref(), true).map(|(_, calendar)| calendar)
    }

    /// Read, change and save under one lock. Never write a stale snapshot.
    pub fn transact<T>(
        path: impl AsRef<Path>,
        change: impl FnOnce(&mut Calendar) -> Result<T>,
    ) -> Result<(Calendar, T)> {
        let (store, mut calendar) = Self::open_wait(path.as_ref(), true)?;
        let value = change(&mut calendar)?;
        store.save(&calendar)?;
        Ok((calendar, value))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<(Self, Calendar)> {
        Self::open_wait(path.as_ref(), false)
    }

    fn open_wait(path: &Path, wait: bool) -> Result<(Self, Calendar)> {
        let path = path.to_owned();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| CalendarError(format!("Cannot create data directory: {error}")))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .map_err(|error| CalendarError(format!("Cannot open calendar lock: {error}")))?;
        let deadline = std::time::Instant::now() + LOCK_TIMEOUT;
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock)
                    if wait && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => {
                    return Err(CalendarError(format!(
                        "Cannot lock the calendar; retry the operation: {error}"
                    )));
                }
            }
        }
        let calendar = match fs::read(&path) {
            Ok(bytes) => {
                let calendar: Calendar = serde_json::from_slice(&bytes).map_err(|error| {
                    CalendarError(format!(
                        "Cannot read calendar file {}: {error}. The file was not changed.",
                        path.display()
                    ))
                })?;
                calendar.validate()?;
                calendar
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Calendar::default(),
            Err(error) => return Err(CalendarError(format!("Cannot read calendar file: {error}"))),
        };
        Ok((Self { path, _lock: lock }, calendar))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, calendar: &Calendar) -> Result<()> {
        calendar.validate()?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
            CalendarError(format!("Cannot create temporary calendar file: {error}"))
        })?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, calendar)
                .map_err(|error| CalendarError(format!("Cannot write calendar file: {error}")))?;
            writer
                .flush()
                .map_err(|error| CalendarError(format!("Cannot write calendar file: {error}")))?;
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| CalendarError(format!("Cannot write calendar file: {error}")))?;
        temporary
            .persist(&self.path)
            .map_err(|error| CalendarError(format!("Cannot replace calendar file: {error}")))?;
        Ok(())
    }
}

/// Override with CALENDAR_GPUI_DATA_FILE; otherwise follow the platform data directory.
pub fn default_data_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CALENDAR_GPUI_DATA_FILE") {
        if path.is_empty() {
            return Err(CalendarError(
                "CALENDAR_GPUI_DATA_FILE cannot be empty.".into(),
            ));
        }
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| Path::new(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    base.map(|base| base.join("calendar-gpui/calendar.json"))
        .ok_or_else(|| {
            CalendarError("Cannot find the data directory. Set CALENDAR_GPUI_DATA_FILE.".into())
        })
}
