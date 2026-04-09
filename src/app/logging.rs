use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOGGER: OnceLock<Logger> = OnceLock::new();

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

struct Logger {
    file: Mutex<File>,
    path: PathBuf,
}

pub fn init() -> std::io::Result<PathBuf> {
    if let Some(logger) = LOGGER.get() {
        return Ok(logger.path.clone());
    }

    let log_dir = std::env::current_dir()?.join("log");
    fs::create_dir_all(&log_dir)?;
    let timestamp = unix_timestamp_secs();
    let path = log_dir.join(format!("scratchpad-{timestamp}.log"));
    let file = File::options().create(true).append(true).open(&path)?;
    let logger = Logger {
        file: Mutex::new(file),
        path: path.clone(),
    };

    if LOGGER.set(logger).is_ok() {
        install_panic_hook();
        log(
            LogLevel::Info,
            &format!("Logging initialized at {}", path.display()),
        );
    }

    Ok(path)
}

pub fn log(level: LogLevel, message: &str) {
    let Some(logger) = LOGGER.get() else {
        return;
    };

    let line = format!(
        "[{}][{}] {}\n",
        unix_timestamp_secs(),
        level.as_str(),
        message
    );

    if let Ok(mut file) = logger.file.lock() {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        log(LogLevel::Error, &format!("panic: {panic_info}"));
        default_hook(panic_info);
    }));
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
