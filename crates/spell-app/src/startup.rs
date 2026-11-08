use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupIntent {
    Open,
    ExtractHere,
    ExtractTo,
    Add,
}

impl StartupIntent {
    pub fn as_flag(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ExtractHere => "extract-here",
            Self::ExtractTo => "extract-to",
            Self::Add => "add",
        }
    }

    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            "open" => Some(Self::Open),
            "extract-here" => Some(Self::ExtractHere),
            "extract-to" => Some(Self::ExtractTo),
            "add" => Some(Self::Add),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartupRequest {
    pub intent: StartupIntent,
    pub paths: Vec<PathBuf>,
}

impl StartupRequest {
    /// 解析命令行：`--extract-here` / `--extract-to` / `--add`，其余参数都是路径。
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Self {
        let mut args: Vec<String> = args.into_iter().collect();
        let intent = match args.first().map(String::as_str) {
            Some("--extract-here") | Some("-x") => {
                args.remove(0);
                StartupIntent::ExtractHere
            }
            Some("--extract-to") => {
                args.remove(0);
                StartupIntent::ExtractTo
            }
            Some("--add") | Some("--compress") => {
                args.remove(0);
                StartupIntent::Add
            }
            _ => StartupIntent::Open,
        };
        let paths = args
            .into_iter()
            .filter(|arg| !arg.starts_with('-'))
            .map(PathBuf::from)
            .collect();
        Self { intent, paths }
    }

    /// handoff.txt 的序列化：第一行是意图，后面一行一个路径。
    pub fn to_handoff(&self) -> String {
        let mut text = String::from(self.intent.as_flag());
        for path in &self.paths {
            text.push('\n');
            text.push_str(&path.display().to_string());
        }
        text
    }

    /// 解析 handoff.txt；未知首行按 Open 处理，兼容旧格式。
    pub fn from_handoff(text: &str) -> Self {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        let first = match lines.next() {
            Some(line) => line,
            None => {
                return Self {
                    intent: StartupIntent::Open,
                    paths: Vec::new(),
                };
            }
        };
        let (intent, first_path) = match StartupIntent::from_flag(first) {
            Some(intent) => (intent, None),
            None => (StartupIntent::Open, Some(first)),
        };
        let mut paths: Vec<PathBuf> = first_path.into_iter().map(PathBuf::from).collect();
        paths.extend(lines.map(PathBuf::from));
        Self { intent, paths }
    }
}

/// “解压到当前文件夹”返回父目录；“解压到 <名字>\\”返回新建子目录。
pub fn extract_target(archive: &Path, separate: bool) -> PathBuf {
    let parent = archive
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if !separate {
        return parent;
    }
    let stem = archive
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted".to_owned());
    parent.join(stem)
}

/// “添加到压缩包”的默认输出：单文件用文件名，多选用 SpellArchive.zip。
pub fn add_output(first: &Path, count: usize) -> PathBuf {
    let parent = first
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = if count == 1 {
        if first.is_dir() {
            first
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        } else {
            first
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        }
        .unwrap_or_else(|| "archive".to_owned())
    } else {
        "SpellArchive".to_owned()
    };
    parent.join(format!("{name}.zip"))
}

#[cfg(test)]
mod tests {
    use super::{StartupIntent, StartupRequest, add_output, extract_target};
    use std::path::{Path, PathBuf};

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn parses_extract_flags() {
        let request = StartupRequest::parse(args(&["--extract-here", "C:/a.zip"]));
        assert_eq!(request.intent, StartupIntent::ExtractHere);
        assert_eq!(request.paths, vec![PathBuf::from("C:/a.zip")]);

        let request = StartupRequest::parse(args(&["C:/b.zip"]));
        assert_eq!(request.intent, StartupIntent::Open);
        assert_eq!(request.paths, vec![PathBuf::from("C:/b.zip")]);
    }

    #[test]
    fn handoff_roundtrip_preserves_intent_and_paths() {
        let request = StartupRequest {
            intent: StartupIntent::Add,
            paths: vec![PathBuf::from("C:/a.txt"), PathBuf::from("C:/b.txt")],
        };
        let parsed = StartupRequest::from_handoff(&request.to_handoff());
        assert_eq!(parsed.intent, StartupIntent::Add);
        assert_eq!(parsed.paths, request.paths);

        let parsed = StartupRequest::from_handoff("C:/legacy.zip");
        assert_eq!(parsed.intent, StartupIntent::Open);
        assert_eq!(parsed.paths, vec![PathBuf::from("C:/legacy.zip")]);
    }

    #[test]
    fn computes_extract_targets_and_pack_output() {
        let archive = Path::new("C:/downloads/a.zip");
        assert_eq!(
            extract_target(archive, false),
            PathBuf::from("C:/downloads")
        );
        assert_eq!(
            extract_target(archive, true),
            PathBuf::from("C:/downloads/a")
        );
        assert_eq!(
            add_output(Path::new("C:/downloads/file.txt"), 1),
            PathBuf::from("C:/downloads/file.zip")
        );
        assert_eq!(
            add_output(Path::new("C:/downloads/a.txt"), 3),
            PathBuf::from("C:/downloads/SpellArchive.zip")
        );
    }
}
