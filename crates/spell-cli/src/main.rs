use std::path::Path;
use std::process::ExitCode;

use spell_core::backend;
use spell_core::task::CancelToken;
use spell_core::types::ArchiveKind;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        [flag] if flag == "--version" || flag == "-V" => {
            println!("spell {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [command, path] if command == "sniff" => sniff(Path::new(path)),
        [command, path] if command == "list" => list(Path::new(path)),
        [command, path] if command == "test" => test(Path::new(path)),
        [command, path] if command == "extract" => {
            println!("骨架阶段：{} 的解压后端尚未接入", path);
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    println!(
        "spell — SpellArchive 命令行（骨架阶段）

用法:
  spell sniff <文件>     识别归档格式（magic bytes）
  spell list  <文件>     列出条目（需格式后端）
  spell test  <文件>     完整性测试（需格式后端）
  spell extract <文件>   解压（需格式后端）
  spell --version

说明: 格式后端（zip/7z/tar）尚未接入，list/test/extract 目前只验证接口与错误路径。"
    );
}

fn sniff(path: &Path) -> ExitCode {
    match backend::sniff_file(path) {
        Ok(kind) => {
            println!("{}: {}", path.display(), kind.label());
            if kind.is_single_stream() {
                println!("  （单文件流容器，不是归档）");
            }
            if kind == ArchiveKind::Unknown {
                println!("  提示：magic bytes 未命中，可能是加密头或未知格式");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("读取失败: {err}");
            ExitCode::FAILURE
        }
    }
}

fn list(path: &Path) -> ExitCode {
    match backend::open(path, None) {
        Ok(reader) => {
            println!(
                "{}（{}）共 {} 条",
                path.display(),
                reader.kind().label(),
                reader.entries().len()
            );
            for entry in reader.entries() {
                println!(
                    "{:>12}  {}  {}",
                    entry.size,
                    entry.method.label(),
                    entry.display
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("打开失败: {err}");
            ExitCode::FAILURE
        }
    }
}

fn test(path: &Path) -> ExitCode {
    match backend::open(path, None) {
        Ok(mut reader) => {
            let cancel = CancelToken::new();
            let mut sink = CountingSink::default();
            match reader.test(&cancel, &mut sink) {
                Ok(report) => {
                    println!(
                        "测试完成：成功 {} 条，失败 {} 条，写出 {} 字节",
                        report.succeeded.len(),
                        report.failed.len(),
                        report.bytes_written
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("测试失败: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(err) => {
            eprintln!("打开失败: {err}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct CountingSink {
    bytes: u64,
}

impl spell_core::ExtractSink for CountingSink {
    fn on_entry_begin(
        &mut self,
        _id: spell_core::EntryId,
        _entry: &spell_core::ArchiveEntry,
    ) -> Result<(), spell_core::SinkError> {
        Ok(())
    }

    fn on_data(
        &mut self,
        _id: spell_core::EntryId,
        chunk: &[u8],
    ) -> Result<(), spell_core::SinkError> {
        self.bytes = self.bytes.saturating_add(chunk.len() as u64);
        Ok(())
    }

    fn on_entry_end(
        &mut self,
        _id: spell_core::EntryId,
        _ok: bool,
    ) -> Result<(), spell_core::SinkError> {
        Ok(())
    }
}
