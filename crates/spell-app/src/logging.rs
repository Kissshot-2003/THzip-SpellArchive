use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();

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
        write(&format!("PANIC {info}"));
        default_hook(info);
    }));

    write(&format!(
        "startup version={} pid={}",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));
}

pub fn write(message: &str) {
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
    let _ = writeln!(file, "[{seconds}] {message}");
    let _ = file.flush();
}
