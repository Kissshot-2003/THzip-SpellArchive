use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use thzip_core::ArchiveError;
use thzip_core::backend::{self, NameOptions, name_codec};
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, ExtractPlan, OverwritePolicy, PlanWarning};
use thzip_core::reader::ExtractReport;
use thzip_core::safety::{ExtractDirSink, VerifySink};
use thzip_core::secret::SecretString;
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveKind, EntryId, EntryKind, Method};
use thzip_core::writer::{SevenZOptions, scan_inputs, write_entries};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--help" | "-h") | None => {
            usage();
            if args.is_empty() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Some("--version" | "-V") => {
            println!("thzip {} (THzip-SpellArchive)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("sniff") => with_path(&args, |path| sniff(path, has_json(&args))),
        Some("list") => with_path(&args, |path| {
            list(
                path,
                password_from(&args).or_else(|| password_file_from(&args)),
                name_options_from(&args),
                has_json(&args),
            )
        }),
        Some("test") => with_path(&args, |path| {
            test(
                path,
                password_from(&args).or_else(|| password_file_from(&args)),
                name_options_from(&args),
                has_json(&args),
            )
        }),
        Some("extract") => extract(&args),
        Some("pack") => pack(&args),
        Some("checksum") => checksum(&args),
        Some("verify") => verify(&args),
        Some(other) => {
            eprintln!("未知命令: {other}");
            usage();
            ExitCode::FAILURE
        }
    }
}

fn with_path(args: &[String], run: impl FnOnce(&Path) -> ExitCode) -> ExitCode {
    match args.get(1) {
        Some(path) => run(Path::new(path)),
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn password_from(args: &[String]) -> Option<SecretString> {
    args.iter()
        .position(|arg| arg == "--password" || arg == "-p")
        .and_then(|index| args.get(index + 1))
        .map(|pw| SecretString::new(pw.as_str()))
}

#[derive(Clone, Copy)]
enum HashAlgo {
    Sha256,
    Sha1,
    Md5,
}

impl HashAlgo {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sha256" => Some(Self::Sha256),
            "sha1" => Some(Self::Sha1),
            "md5" => Some(Self::Md5),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha1 => "sha1",
            Self::Md5 => "md5",
        }
    }

    fn hasher(self) -> Hasher {
        match self {
            Self::Sha256 => Hasher::Sha256(<sha2::Sha256 as sha2::Digest>::new()),
            Self::Sha1 => Hasher::Sha1(<sha1::Sha1 as sha1::Digest>::new()),
            Self::Md5 => Hasher::Md5(<md5::Md5 as md5::Digest>::new()),
        }
    }
}

enum Hasher {
    Sha256(sha2::Sha256),
    Sha1(sha1::Sha1),
    Md5(md5::Md5),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(h) => sha2::Digest::update(h, data),
            Self::Sha1(h) => sha1::Digest::update(h, data),
            Self::Md5(h) => md5::Digest::update(h, data),
        }
    }

    fn finish_hex(self) -> String {
        match self {
            Self::Sha256(h) => hex::encode(sha2::Digest::finalize(h)),
            Self::Sha1(h) => hex::encode(sha1::Digest::finalize(h)),
            Self::Md5(h) => hex::encode(md5::Digest::finalize(h)),
        }
    }
}

fn hash_file(path: &Path, algo: HashAlgo) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = algo.hasher();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher.finish_hex())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn algo_from_args(args: &[String]) -> Option<HashAlgo> {
    args.iter()
        .position(|arg| arg == "--algo")
        .and_then(|index| args.get(index + 1))
        .and_then(|value| HashAlgo::parse(value))
}

fn checksum(args: &[String]) -> ExitCode {
    let Some(target) = args.get(1) else {
        usage();
        return ExitCode::FAILURE;
    };
    if args.iter().any(|arg| arg == "--algo")
        && args
            .iter()
            .position(|arg| arg == "--algo")
            .and_then(|index| args.get(index + 1))
            .is_some_and(|value| HashAlgo::parse(value).is_none())
    {
        eprintln!("--algo 只支持 sha256 / sha1 / md5");
        return ExitCode::FAILURE;
    }
    let algo = algo_from_args(args).unwrap_or(HashAlgo::Sha256);
    let json = has_json(args);
    let path = Path::new(target);
    let mut lines: Vec<(String, String)> = Vec::new();

    if path.is_dir() {
        let mut files = Vec::new();
        if let Err(err) = collect_files(path, &mut files) {
            eprintln!("无法遍历目录: {err}");
            return ExitCode::FAILURE;
        }
        files.sort();
        for file in &files {
            let Ok(rel) = file.strip_prefix(path) else {
                continue;
            };
            match hash_file(file, algo) {
                Ok(hash) => lines.push((hash, rel.to_string_lossy().replace('\\', "/"))),
                Err(err) => {
                    eprintln!("无法读取 {}: {err}", file.display());
                    return ExitCode::FAILURE;
                }
            }
        }
    } else {
        match hash_file(path, algo) {
            Ok(hash) => lines.push((
                hash,
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
            )),
            Err(err) => {
                eprintln!("无法读取 {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }

    if json {
        let mut out = format!(
            "{{\"op\":\"checksum\",\"path\":{},\"algo\":{},\"entries\":[",
            json_escape(&path.display().to_string()),
            json_escape(algo.label())
        );
        for (index, (hash, name)) in lines.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"hash\":{},\"path\":{}}}",
                json_escape(hash),
                json_escape(name)
            ));
        }
        out.push_str("]}");
        println!("{out}");
    } else {
        for (hash, name) in &lines {
            println!("{hash}  {name}");
        }
    }
    ExitCode::SUCCESS
}

fn parse_sums(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((hash, path)) = line.split_once("  ") else {
            continue;
        };
        let path = path
            .trim_start_matches('*')
            .trim_start_matches("./")
            .replace('\\', "/");
        map.insert(path, hash.to_ascii_lowercase());
    }
    map
}

fn normalize_entry_name(name: &str) -> String {
    name.trim_start_matches("./").replace('\\', "/")
}

struct HashSink {
    algo: HashAlgo,
    hashers: HashMap<EntryId, Hasher>,
}

impl thzip_core::ExtractSink for HashSink {
    fn on_entry_begin(
        &mut self,
        id: EntryId,
        _entry: &thzip_core::ArchiveEntry,
    ) -> Result<(), thzip_core::SinkError> {
        self.hashers.insert(id, self.algo.hasher());
        Ok(())
    }

    fn on_data(&mut self, id: EntryId, chunk: &[u8]) -> Result<(), thzip_core::SinkError> {
        if let Some(hasher) = self.hashers.get_mut(&id) {
            hasher.update(chunk);
        }
        Ok(())
    }

    fn on_entry_end(&mut self, _id: EntryId, _ok: bool) -> Result<(), thzip_core::SinkError> {
        Ok(())
    }
}

fn verify(args: &[String]) -> ExitCode {
    let Some(archive) = args.get(1) else {
        usage();
        return ExitCode::FAILURE;
    };
    let sum_path = args
        .iter()
        .position(|arg| arg == "--checksum-file")
        .and_then(|index| args.get(index + 1));
    let Some(sum_path) = sum_path else {
        eprintln!("verify 需要 --checksum-file <SHA256SUMS.txt>");
        return ExitCode::FAILURE;
    };
    let text = match std::fs::read_to_string(sum_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("无法读取校验和文件: {err}");
            return ExitCode::FAILURE;
        }
    };
    let expected = parse_sums(&text);
    let algo = algo_from_args(args).unwrap_or(HashAlgo::Sha256);
    let json = has_json(args);
    let archive = Path::new(archive);

    let mut reader = match backend::open(archive, password_from(args).as_ref()) {
        Ok(reader) => reader,
        Err(err) => {
            report_open_error(&err);
            return ExitCode::FAILURE;
        }
    };
    let names: Vec<(EntryId, String)> = reader
        .entries()
        .iter()
        .filter(|entry| !entry.kind.is_dir())
        .map(|entry| (entry.id, normalize_entry_name(&entry.display)))
        .collect();
    let ids: Vec<EntryId> = names.iter().map(|(id, _)| *id).collect();
    let target = PathBuf::from("__thzip_verify__");
    let limits = Limits::default();
    let plan = match reader.plan(&ids, &target, &limits) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("预检失败: {err}");
            return ExitCode::FAILURE;
        }
    };
    let decisions = DecisionTable::default();
    let cancel = CancelToken::new();
    let mut sink = HashSink {
        algo,
        hashers: HashMap::new(),
    };
    if let Err(err) = reader.extract(&plan, &decisions, &cancel, &mut sink) {
        eprintln!("读取归档失败: {err}");
        return ExitCode::FAILURE;
    }

    let mut actual = BTreeMap::new();
    for (id, name) in names {
        if let Some(hasher) = sink.hashers.remove(&id) {
            actual.insert(name, hasher.finish_hex());
        }
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut mismatch = Vec::new();
    for (path, hash) in &expected {
        match actual.get(path) {
            None => missing.push(path.clone()),
            Some(actual_hash) if actual_hash != hash => mismatch.push(path.clone()),
            _ => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            extra.push(path.clone());
        }
    }

    let ok = missing.is_empty() && extra.is_empty() && mismatch.is_empty();
    if json {
        println!(
            "{{\"op\":\"verify\",\"path\":{},\"ok\":{},\"missing\":{},\"extra\":{},\"mismatch\":{}}}",
            json_escape(&archive.display().to_string()),
            ok,
            json_string_array(&missing),
            json_string_array(&extra),
            json_string_array(&mismatch)
        );
    } else {
        if !missing.is_empty() {
            println!("缺失 {} 项：{}", missing.len(), missing.join("、"));
        }
        if !extra.is_empty() {
            println!("多出 {} 项：{}", extra.len(), extra.join("、"));
        }
        if !mismatch.is_empty() {
            println!("不符 {} 项：{}", mismatch.len(), mismatch.join("、"));
        }
        if ok {
            println!("校验通过：{} 项一致", actual.len());
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn json_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_escape(item));
    }
    out.push(']');
    out
}

/// 读取密码文件的首个非空行，支持 UTF-8 / UTF-16 LE / UTF-16 BE（含 BOM）。
fn read_password_file(path: &Path) -> Option<SecretString> {
    let bytes = std::fs::read(path).ok()?;
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16(&units).ok()?
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16(&units).ok()?
    } else {
        String::from_utf8(bytes.to_vec())
            .ok()?
            .trim_start_matches('\u{FEFF}')
            .to_owned()
    };
    let line = text.lines().find(|line| !line.trim().is_empty())?.trim();
    (!line.is_empty()).then(|| SecretString::new(line))
}

fn password_file_from(args: &[String]) -> Option<SecretString> {
    args.iter()
        .position(|arg| arg == "--password-file")
        .and_then(|index| args.get(index + 1))
        .and_then(|path| read_password_file(Path::new(path)))
}

/// 同目录密码约定：只尝试一次，失败由调用方静默跳过。
fn auto_password(archive: &Path) -> Option<SecretString> {
    let dir = archive.parent()?;
    ["password.txt", "解压密码.txt"]
        .iter()
        .find_map(|name| read_password_file(&dir.join(name)))
}

fn name_options_from(args: &[String]) -> NameOptions {
    let encoding = args
        .iter()
        .position(|arg| arg == "--name-encoding")
        .and_then(|index| args.get(index + 1))
        .and_then(|value| name_codec::NameOverride::parse(value))
        .unwrap_or_default();
    NameOptions { encoding }
}

fn usage() {
    println!(
        "thzip — THzip-SpellArchive 命令行

用法:
  thzip sniff   <文件> [--json]           识别归档格式（magic bytes）
  thzip list    <文件> [--password <pw> | --password-file <path>] [--name-encoding <enc>] [--json]
                                          列出条目
  thzip test    <文件> [--password <pw> | --password-file <path>] [--name-encoding <enc>] [--json]
                                          完整性测试（CRC 校验）
  thzip extract <文件> [目标目录] [--force|--rename|--number] [--password <pw> | --password-file <path>] [--name-encoding <enc>] [--json]
                                          安全解压；默认跳过已有文件
  thzip pack    <输出> <输入...> [--store] [--method <deflate|bzip2|zstd>]
                [--7z-level <0-9>] [--7z-dict <MiB>] [--volume-size <MiB>]
                 [--volume-first-index <0-999>]
                [--comment <文本>] [--force] [--append] [--password <pw>] [--json]
                                          创建 zip / 7z / tar 家族；--password 用于 zip / 7z 加密
  thzip checksum <文件或目录> [--algo sha256|sha1|md5] [--json]
                                           生成 SHA256SUMS 兼容格式的校验和
  thzip verify   <归档> --checksum-file <SHA256SUMS.txt> [--password <pw>] [--algo <algo>] [--json]
                                           比对归档内文件与校验和；不一致时退出码非零
  thzip --version

  --json 把结果以单行 JSON 写到 stdout；错误仍写 stderr 并返回非零退出码。

说明:
  - 不带目标目录时解压到「文件所在目录/去扩展名」。
  - 冲突策略：--force 覆盖；--rename 保留已有并改名 a~1.txt；--number 改为 a (1).txt；
    都不加则跳过冲突条目。
  - 支持 zip / 7z / tar 家族读写；7z 与 zip 可加密。
  - --method 只影响 zip：deflate（默认）/ bzip2 / zstd；--store 优先。
  - --7z-level / --7z-dict 只影响 7z：级别 0–9（默认 6），字典 MiB（0 = 自动）。
  - pack 的输出扩展名决定容器：.zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4。
  - --volume-size 只用于新建 .7z：按 MiB 切成 .7z.001 / .002 …；已存在分卷需 --force。
  - --comment 只用于新建 zip：写入归档注释。
  - --append 支持 zip / 7z / tar 家族；7z 与 tar 会先解包重建，完成后原子替换。
  - pack 会先写入同目录临时文件，成功后原子替换输出。"
    );
}

fn sniff(path: &Path, json: bool) -> ExitCode {
    match backend::sniff_file(path) {
        Ok(kind) => {
            if json {
                println!(
                    "{{\"path\":{},\"format\":{},\"single_stream\":{}}}",
                    json_escape(&path.display().to_string()),
                    json_escape(kind.label()),
                    kind.is_single_stream()
                );
            } else {
                println!("{}: {}", path.display(), kind.label());
                if kind.is_single_stream() {
                    println!("  （单文件流容器，会解压为单个文件）");
                }
                if kind == ArchiveKind::Unknown {
                    println!("  提示：magic bytes 未命中，可能是加密头或未知格式");
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("读取失败: {err}");
            ExitCode::FAILURE
        }
    }
}

fn list(
    path: &Path,
    password: Option<SecretString>,
    name_options: NameOptions,
    json: bool,
) -> ExitCode {
    match backend::open_with_options(path, password.as_ref(), name_options) {
        Ok(reader) => {
            if json {
                println!("{}", list_json(path, reader.as_ref()));
                return ExitCode::SUCCESS;
            }
            println!(
                "{}（{}）共 {} 条",
                path.display(),
                reader.kind().label(),
                reader.entries().len()
            );
            if let Some(comment) = reader.comment() {
                println!("注释: {comment}");
            }
            for entry in reader.entries() {
                let flag = if entry.encrypted { " 🔒" } else { "" };
                let kind = if entry.kind.is_dir() { "d" } else { "-" };
                println!(
                    "{kind} {:>12}  {:>12}  {:<8}  {}{}",
                    entry.size,
                    entry
                        .packed
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                    entry.method.label(),
                    entry.display,
                    flag
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            report_open_error(&err);
            ExitCode::FAILURE
        }
    }
}

fn test(
    path: &Path,
    password: Option<SecretString>,
    name_options: NameOptions,
    json: bool,
) -> ExitCode {
    match backend::open_with_options(path, password.as_ref(), name_options) {
        Ok(mut reader) => {
            let kind = reader.kind();
            let cancel = CancelToken::new();
            let mut sink = VerifySink::with_limits(Limits::default());
            match reader.test(&Limits::default(), &cancel, &mut sink) {
                Ok(report) => {
                    if json {
                        println!("{}", report_json("test", path, kind, None, &report));
                    } else {
                        println!(
                            "测试完成：成功 {} 条，失败 {} 条，CRC 不符 {} 条，读取 {} 字节",
                            report.succeeded.len(),
                            report.failed.len(),
                            report.crc_mismatch.len(),
                            report.bytes_written
                        );
                        for (id, error) in &report.failed {
                            println!("  条目 {id} 失败: {error}");
                        }
                        for id in &report.crc_mismatch {
                            println!("  条目 {id} CRC 不符");
                        }
                    }
                    if report.failed.is_empty() && report.crc_mismatch.is_empty() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(err) => {
                    eprintln!("测试失败: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(err) => {
            report_open_error(&err);
            ExitCode::FAILURE
        }
    }
}

fn extract(args: &[String]) -> ExitCode {
    let Some(input) = args.get(1) else {
        usage();
        return ExitCode::FAILURE;
    };
    let force = args.iter().any(|arg| arg == "--force" || arg == "-f");
    let rename = args.iter().any(|arg| arg == "--rename");
    let auto_number = args
        .iter()
        .any(|arg| arg == "--number" || arg == "--auto-number");
    if force && (rename || auto_number) || (rename && auto_number) {
        eprintln!("--force / --rename / --number 只能选一个");
        return ExitCode::FAILURE;
    }
    let input = Path::new(input);
    let password = password_from(args)
        .or_else(|| password_file_from(args))
        .or_else(|| auto_password(input));
    let json = has_json(args);

    let target = match args.get(2) {
        Some(arg) if !arg.starts_with('-') => PathBuf::from(arg),
        _ => default_target(input),
    };

    let mut reader =
        match backend::open_with_options(input, password.as_ref(), name_options_from(args)) {
            Ok(reader) => reader,
            Err(err) => {
                report_open_error(&err);
                return ExitCode::FAILURE;
            }
        };

    let limits = Limits::default();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = match reader.plan(&ids, &target, &limits) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("预检失败: {err}");
            return ExitCode::FAILURE;
        }
    };

    if !json {
        print_plan(&plan, &target);
    }

    let decisions = DecisionTable {
        default: if force {
            OverwritePolicy::Overwrite
        } else if rename {
            OverwritePolicy::Rename
        } else if auto_number {
            OverwritePolicy::AutoNumber
        } else {
            OverwritePolicy::Skip
        },
        per_entry: Default::default(),
    };

    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let kind = reader.kind();

    match reader.extract(&plan, &decisions, &cancel, &mut sink) {
        Ok(report) => {
            if json {
                println!(
                    "{}",
                    report_json("extract", input, kind, Some(&target), &report)
                );
            } else {
                println!(
                    "解压完成：成功 {} 条，跳过 {} 条，失败 {} 条，CRC 不符 {} 条，写出 {} 字节，耗时 {:.2?}",
                    report.succeeded.len(),
                    report.skipped.len(),
                    report.failed.len(),
                    report.crc_mismatch.len(),
                    report.bytes_written,
                    report.elapsed
                );
                for (id, error) in &report.failed {
                    println!("  条目 {id} 失败: {error}");
                }
                for id in &report.crc_mismatch {
                    println!("  条目 {id} CRC 不符（文件已保留）");
                }
            }
            if report.failed.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("解压中止: {err}");
            ExitCode::FAILURE
        }
    }
}

fn print_plan(plan: &ExtractPlan, target: &Path) {
    println!(
        "预检：{} 个条目，解压后 {} 字节，最大单文件 {} 字节",
        plan.items.len(),
        plan.total_bytes,
        plan.largest_entry
    );
    println!("目标目录: {}", target.display());
    if let Some(free) = plan.free_space {
        println!("目标盘剩余: {free} 字节");
    }
    if plan.needs_password {
        println!("注意：包含加密条目，需要 --password");
    }
    if !plan.conflicts.is_empty() {
        println!(
            "冲突: {} 个（默认跳过；可用 --force / --rename / --number）",
            plan.conflicts.len()
        );
        for conflict in plan.conflicts.iter().take(10) {
            println!("  {:?}: {}", conflict.kind, conflict.path.display());
        }
    }
    for warning in &plan.warnings {
        match warning {
            PlanWarning::UnsafePath { id, issue } => {
                println!("  不安全路径（条目 {id}）: {issue}")
            }
            PlanWarning::SuspiciousDisplayName { id, detail } => {
                println!("  可疑文件名（条目 {id}）: {detail}")
            }
            PlanWarning::NameEncodingAmbiguous { id, confidence } => {
                println!(
                    "  文件名编码不确定（条目 {id}，置信 {confidence}），可加 --name-encoding 指定"
                )
            }
            PlanWarning::SymlinkSkipped { id } => println!("  跳过链接（条目 {id}）"),
            PlanWarning::BombSuspected { hit } => println!("  疑似炸弹: {hit}"),
            PlanWarning::UnsupportedMethod { id, method } => {
                println!("  不支持的压缩方法（条目 {id}）: {}", method.label())
            }
            PlanWarning::InsufficientSpace {
                required,
                reserved,
                available,
            } => println!(
                "  目标盘空间不足: 需要 {required} 字节，需保留 {reserved} 字节，可用 {available} 字节"
            ),
            PlanWarning::NestedArchive { id, depth } => {
                println!("  包含嵌套归档（条目 {id}，第 {depth} 层）")
            }
            PlanWarning::LargeFileOnFat32 { id, size } => {
                println!("  单文件 {size} 字节超过 FAT32 的 4 GiB 上限（条目 {id}）")
            }
        }
    }
}

fn pack(args: &[String]) -> ExitCode {
    let Some(output) = args.get(1).filter(|arg| !arg.starts_with('-')) else {
        usage();
        return ExitCode::FAILURE;
    };
    let output = Path::new(output);
    let json = has_json(args);
    let store = args.iter().any(|arg| arg == "--store" || arg == "-s");
    let force = args.iter().any(|arg| arg == "--force" || arg == "-f");
    let append = args.iter().any(|arg| arg == "--append" || arg == "-a");
    let method_name = args
        .iter()
        .position(|arg| arg == "--method" || arg == "-m")
        .and_then(|index| args.get(index + 1))
        .map(|value| value.to_ascii_lowercase());
    let password = password_from(args);
    let comment = args
        .iter()
        .position(|arg| arg == "--comment")
        .and_then(|index| args.get(index + 1))
        .cloned();
    let mut sevenz_level = 6u32;
    if let Some(index) = args.iter().position(|arg| arg == "--7z-level") {
        match args
            .get(index + 1)
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(level) if level <= 9 => sevenz_level = level,
            _ => {
                eprintln!("--7z-level 需要 0–9 的整数");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut volume_mib = 0u32;
    if let Some(index) = args.iter().position(|arg| arg == "--volume-size") {
        match args
            .get(index + 1)
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(mib) if (1..=65536).contains(&mib) => volume_mib = mib,
            _ => {
                eprintln!("--volume-size 需要 1–65536 的整数（MiB）");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut volume_first_index = 1u32;
    if let Some(index) = args.iter().position(|arg| arg == "--volume-first-index") {
        match args
            .get(index + 1)
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(first) if first <= 999 => volume_first_index = first,
            _ => {
                eprintln!("--volume-first-index 需要 0–999 的整数");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut sevenz_dict_mib = 0u32;
    if let Some(index) = args.iter().position(|arg| arg == "--7z-dict") {
        match args
            .get(index + 1)
            .and_then(|value| value.parse::<u32>().ok())
        {
            Some(mib) if mib <= 1024 => sevenz_dict_mib = mib,
            _ => {
                eprintln!("--7z-dict 需要 0–1024 的整数（MiB，0 = 自动）");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut inputs = Vec::new();
    let mut skip_value = false;
    for arg in args.iter().skip(2) {
        if skip_value {
            skip_value = false;
            continue;
        }
        if arg == "--password"
            || arg == "-p"
            || arg == "--password-file"
            || arg == "--method"
            || arg == "-m"
            || arg == "--7z-level"
            || arg == "--7z-dict"
            || arg == "--volume-size"
            || arg == "--volume-first-index"
            || arg == "--comment"
        {
            skip_value = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        inputs.push(PathBuf::from(arg));
    }

    if inputs.is_empty() {
        eprintln!("请至少指定一个要打包的文件或目录");
        usage();
        return ExitCode::FAILURE;
    }

    let Some(kind) = kind_for_output(output) else {
        eprintln!(
            "不支持的输出格式；可用 .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
        );
        return ExitCode::FAILURE;
    };

    if volume_mib > 0 && (kind != ArchiveKind::SevenZ || append) {
        eprintln!("--volume-size 只支持新建 .7z");
        return ExitCode::FAILURE;
    }
    if comment.is_some() && (kind != ArchiveKind::Zip || append) {
        eprintln!("--comment 只支持新建 zip");
        return ExitCode::FAILURE;
    }
    if append && !backend::supports_append(kind) {
        eprintln!("追加模式只支持 zip / 7z / tar 家族");
        return ExitCode::FAILURE;
    }
    if password.is_some() && !matches!(kind, ArchiveKind::Zip | ArchiveKind::SevenZ) {
        eprintln!("加密写入只支持 zip / 7z");
        return ExitCode::FAILURE;
    }
    if append && password.is_some() {
        eprintln!("追加模式暂不支持设置新密码");
        return ExitCode::FAILURE;
    }
    if append {
        if !output.is_file() {
            eprintln!("追加模式要求目标归档已存在：{}", output.display());
            return ExitCode::FAILURE;
        }
    } else if output.exists() && !force {
        eprintln!(
            "目标已存在：{}（加 --force 覆盖，或 --append 追加）",
            output.display()
        );
        return ExitCode::FAILURE;
    }

    let limits = Limits::default();
    let entries = match scan_inputs(&inputs, &limits) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("扫描输入失败: {err}");
            return ExitCode::FAILURE;
        }
    };
    let total: u64 = entries.iter().map(|entry| entry.size).sum();

    let method = if store {
        Method::Store
    } else if kind == ArchiveKind::Zip {
        match method_name.as_deref() {
            None | Some("deflate") => Method::Deflate,
            Some("bzip2") => Method::Bzip2,
            Some("zstd") | Some("zstandard") => Method::Zstd,
            Some(other) => {
                eprintln!("不支持的 zip 压缩方法：{other}（可用 deflate / bzip2 / zstd）");
                return ExitCode::FAILURE;
            }
        }
    } else {
        if method_name.is_some() {
            eprintln!("--method 目前只支持 zip");
            return ExitCode::FAILURE;
        }
        Method::Deflate
    };

    let sevenz = SevenZOptions {
        level: sevenz_level,
        dict_size: sevenz_dict_mib.saturating_mul(1024 * 1024),
        threads: 0,
    };
    let detail = match kind {
        ArchiveKind::Zip => {
            let mut detail = format!("zip · {}", method.label());
            if password.is_some() && !append {
                detail.push_str(" + AES-256");
            }
            detail
        }
        ArchiveKind::SevenZ => {
            let mut detail = format!("7z · {}", if store { "COPY" } else { "LZMA2" });
            if !store {
                detail.push_str(&format!(" · L{sevenz_level}"));
                if sevenz_dict_mib > 0 {
                    detail.push_str(&format!(" · 字典 {sevenz_dict_mib} MiB"));
                }
            }
            if password.is_some() {
                detail.push_str(" + AES-256");
            }
            detail
        }
        other => other.label().to_owned(),
    };
    if !json {
        println!(
            "{}：{} 个条目，原始 {} 字节 → {}（{}）",
            if append { "追加" } else { "打包" },
            entries.len(),
            total,
            output.display(),
            detail
        );
    }

    let cancel = CancelToken::new();
    let created = if append {
        backend::create_writer_append(output, kind, method, sevenz, limits, &cancel)
    } else {
        backend::create_writer_with_options(output, kind, method, password.as_ref(), sevenz)
    };
    let mut writer = match created {
        Ok(writer) => writer,
        Err(err) => {
            eprintln!("创建写入器失败: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(text) = comment.as_deref()
        && let Err(err) = writer.set_comment(text)
    {
        eprintln!("写入注释失败: {err}");
        return ExitCode::FAILURE;
    }

    let last_report = std::cell::Cell::new(0u64);
    let progress = |done: u64| {
        if !json && done.saturating_sub(last_report.get()) >= 64 * 1024 * 1024 {
            last_report.set(done);
            println!("  已读 {done} / {total} 字节");
        }
    };

    if let Err(err) = write_entries(writer.as_mut(), &entries, Some(method), &cancel, &progress) {
        eprintln!("打包中止: {err}");
        return ExitCode::FAILURE;
    }

    match writer.finish() {
        Ok(report) => {
            let mut volumes = 0usize;
            if volume_mib > 0 {
                match backend::volume::split_archive_with_first(
                    output,
                    u64::from(volume_mib) * 1024 * 1024,
                    force,
                    volume_first_index,
                ) {
                    Ok(parts) => volumes = parts.len(),
                    Err(err) => {
                        eprintln!("分卷失败: {err}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            if json {
                println!(
                    "{{\"op\":{},\"path\":{},\"format\":{},\"appended\":{},\"entries\":{},\"bytes_in\":{},\"bytes_out\":{},\"volumes\":{},\"elapsed_ms\":{}}}",
                    json_escape(if append { "append" } else { "pack" }),
                    json_escape(&output.display().to_string()),
                    json_escape(kind.label()),
                    append,
                    report.entries,
                    report.bytes_in,
                    report.bytes_out,
                    volumes,
                    report.elapsed.as_millis()
                );
            } else {
                println!(
                    "完成：{} 个条目，读入 {} 字节，写出 {} 字节，耗时 {:.2?}",
                    report.entries, report.bytes_in, report.bytes_out, report.elapsed
                );
                if volumes > 0 {
                    println!("已切分为 {volumes} 个分卷（.001 起）");
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("收尾失败: {err}");
            ExitCode::FAILURE
        }
    }
}

fn kind_for_output(path: &Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    for (suffix, kind) in [
        (".tar.gz", ArchiveKind::TarGz),
        (".tgz", ArchiveKind::TarGz),
        (".tar.bz2", ArchiveKind::TarBz2),
        (".tbz2", ArchiveKind::TarBz2),
        (".tbz", ArchiveKind::TarBz2),
        (".tar.xz", ArchiveKind::TarXz),
        (".txz", ArchiveKind::TarXz),
        (".tar.zst", ArchiveKind::TarZst),
        (".tzst", ArchiveKind::TarZst),
        (".tar.lz4", ArchiveKind::TarLz4),
        (".tlz4", ArchiveKind::TarLz4),
        (".zip", ArchiveKind::Zip),
        (".7z", ArchiveKind::SevenZ),
        (".tar", ArchiveKind::Tar),
    ] {
        if name.ends_with(suffix) {
            return Some(kind);
        }
    }
    None
}

fn default_target(input: &Path) -> PathBuf {
    let parent = input.parent().filter(|p| !p.as_os_str().is_empty());
    let stem = input
        .file_stem()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output"));
    match parent {
        Some(parent) => parent.join(stem),
        None => stem,
    }
}

fn has_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn opt_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn opt_time(value: Option<std::time::SystemTime>) -> String {
    value
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(|| "null".to_owned(), |delta| delta.as_secs().to_string())
}

fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Dir => "dir",
        EntryKind::Symlink => "symlink",
        EntryKind::Hardlink => "hardlink",
        EntryKind::Other => "other",
    }
}

fn list_json(path: &Path, reader: &dyn thzip_core::ArchiveReader) -> String {
    let mut out = String::from("{\"op\":\"list\",\"path\":");
    out.push_str(&json_escape(&path.display().to_string()));
    out.push_str(",\"format\":");
    out.push_str(&json_escape(reader.kind().label()));
    out.push_str(",\"comment\":");
    out.push_str(
        &reader
            .comment()
            .map(|comment| json_escape(&comment))
            .unwrap_or_else(|| "null".to_owned()),
    );
    out.push_str(",\"entries\":[");
    for (index, entry) in reader.entries().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        out.push_str(&entry.id.to_string());
        out.push_str(",\"name\":");
        out.push_str(&json_escape(&entry.display));
        out.push_str(",\"kind\":");
        out.push_str(&json_escape(kind_label(entry.kind)));
        out.push_str(",\"size\":");
        out.push_str(&entry.size.to_string());
        out.push_str(",\"packed\":");
        out.push_str(&opt_u64(entry.packed));
        out.push_str(",\"method\":");
        out.push_str(&json_escape(entry.method.label()));
        out.push_str(",\"encrypted\":");
        out.push_str(if entry.encrypted { "true" } else { "false" });
        out.push_str(",\"crc32\":");
        out.push_str(&opt_u32(entry.crc32));
        out.push_str(",\"mtime\":");
        out.push_str(&opt_time(entry.mtime));
        out.push('}');
    }
    out.push_str("]}");
    out
}

fn report_json(
    op: &str,
    path: &Path,
    kind: ArchiveKind,
    target: Option<&Path>,
    report: &ExtractReport,
) -> String {
    let mut out = String::from("{\"op\":");
    out.push_str(&json_escape(op));
    out.push_str(",\"path\":");
    out.push_str(&json_escape(&path.display().to_string()));
    out.push_str(",\"format\":");
    out.push_str(&json_escape(kind.label()));
    out.push_str(",\"target\":");
    out.push_str(
        &target
            .map(|target| json_escape(&target.display().to_string()))
            .unwrap_or_else(|| "null".to_owned()),
    );
    out.push_str(",\"ok\":");
    out.push_str(
        if report.failed.is_empty() && report.crc_mismatch.is_empty() {
            "true"
        } else {
            "false"
        },
    );
    out.push_str(&format!(
        ",\"name_encoding\":{},\"name_confidence\":{},\"name_renamed\":{}",
        json_escape(report.name.encoding.label()),
        report.name.confidence,
        report.name.renamed
    ));
    out.push_str(&format!(
        ",\"succeeded\":{},\"skipped\":{},\"bytes_written\":{},\"elapsed_ms\":{},\"failed\":[",
        report.succeeded.len(),
        report.skipped.len(),
        report.bytes_written,
        report.elapsed.as_millis()
    ));
    for (index, (id, reason)) in report.failed.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        out.push_str(&id.to_string());
        out.push_str(",\"reason\":");
        out.push_str(&json_escape(&reason.to_string()));
        out.push('}');
    }
    out.push_str("],\"crc_mismatch\":[");
    for (index, id) in report.crc_mismatch.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&id.to_string());
    }
    out.push_str("]}");
    out
}

fn report_open_error(err: &ArchiveError) {
    match err {
        ArchiveError::PasswordRequired => {
            eprintln!("需要密码：请加 --password <密码>");
        }
        ArchiveError::BadPassword => eprintln!("密码错误"),
        other => eprintln!("打开失败: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::json_escape;

    #[test]
    fn escapes_quotes_backslashes_and_controls() {
        assert_eq!(json_escape("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(json_escape("灵梦"), "\"灵梦\"");
    }
}
