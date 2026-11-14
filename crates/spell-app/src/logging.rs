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

pub fn debug(module: &str, message: &str) {
    log(Level::Debug, module, message);
}

pub fn log(level: Level, module: &str, message: &str) {
    log_fields(level, module, message, &[]);
}

/// 结构化日志：消息后追加 `key=value` 字段，值含空白/引号/等号时自动加引号。
///
/// 例：`[42] [ERROR task] task failed task=3 error="RAR 处理失败: ..."`
pub fn log_fields(level: Level, module: &str, message: &str, fields: &[(&str, &str)]) {
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
    let _ = writeln!(
        file,
        "{}",
        format_line_with_fields(seconds, level, module, message, fields)
    );
    let _ = file.flush();
}

pub fn error_fields(module: &str, message: &str, fields: &[(&str, &str)]) {
    log_fields(Level::Error, module, message, fields);
}

pub fn warn_fields(module: &str, message: &str, fields: &[(&str, &str)]) {
    log_fields(Level::Warn, module, message, fields);
}

pub fn info_fields(module: &str, message: &str, fields: &[(&str, &str)]) {
    log_fields(Level::Info, module, message, fields);
}

fn format_line(seconds: u64, level: Level, module: &str, message: &str) -> String {
    format!("[{seconds}] [{} {module}] {message}", level.label())
}

fn format_line_with_fields(
    seconds: u64,
    level: Level,
    module: &str,
    message: &str,
    fields: &[(&str, &str)],
) -> String {
    let mut line = format_line(seconds, level, module, message);
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(&field_value(value));
    }
    line
}

/// 值含空白、引号、反斜杠或 `=` 时加双引号，并转义内部特殊字符。
fn field_value(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\' | '='));
    if !needs_quotes {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::{Level, field_value, format_line, format_line_with_fields};

    #[test]
    fn formats_level_and_module() {
        assert_eq!(
            format_line(42, Level::Warn, "prefs", "save failed"),
            "[42] [WARN prefs] save failed"
        );
    }

    #[test]
    fn appends_structured_fields() {
        assert_eq!(
            format_line_with_fields(
                7,
                Level::Error,
                "task",
                "task failed",
                &[("task", "3"), ("phase", "extract")],
            ),
            "[7] [ERROR task] task failed task=3 phase=extract"
        );
    }

    #[test]
    fn quotes_values_with_spaces_and_escapes() {
        assert_eq!(field_value("plain"), "plain");
        assert_eq!(field_value("C:\\Users\\a b"), "\"C:\\\\Users\\\\a b\"");
        assert_eq!(field_value("say \"hi\"\n"), "\"say \\\"hi\\\"\\n\"");
        assert_eq!(field_value("a=b"), "\"a=b\"");
        assert_eq!(field_value(""), "\"\"");
    }
}
