use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

pub fn init() {
    let file = crate::prefs::log_path().and_then(|path| {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::metadata(&path)
            .map(|meta| meta.len() > MAX_LOG_BYTES)
            .unwrap_or(false)
        {
            let _ = File::create(&path);
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
    });
    let _ = LOG.set(Mutex::new(file));

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(Level::Error, "panic", &format!("{info}"));
        default_hook(info);
    }));

    log(
        Level::Info,
        "startup",
        &format!(
            "version={} pid={}",
            env!("CARGO_PKG_VERSION"),
            std::process::id()
        ),
    );
}

/// 兼容旧调用：默认 INFO / app。
pub fn write(message: &str) {
    log(Level::Info, "app", message);
}

pub fn warn(module: &str, message: &str) {
    log(Level::Warn, module, message);
}

pub fn error(module: &str, message: &str) {
    log(Level::Error, module, message);
}

pub fn debug(module: &str, message: &str) {
    log(Level::Debug, module, message);
}

pub fn log(level: Level, module: &str, message: &str) {
    let Some(lock) = LOG.get() else {
        return;
    };
    let Ok(mut guard) = lock.lock() else {
        return;
    };
    let Some(file) = guard.as_mut() else {
        return;
    };
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = writeln!(file, "{}", format_line(seconds, level, module, message));
    let _ = file.flush();
}

fn format_line(seconds: u64, level: Level, module: &str, message: &str) -> String {
    format!("[{seconds}] [{} {module}] {message}", level.label())
}

#[cfg(test)]
mod tests {
    use super::{Level, format_line};

    #[test]
    fn formats_level_and_module() {
        assert_eq!(
            format_line(42, Level::Warn, "prefs", "save failed"),
            "[42] [WARN prefs] save failed"
        );
    }
}
