//! 与 7-Zip 的对比基准：验证设计文档的三条硬红线。
//!
//! - 解压 ≥ 7-Zip 的 80%
//! - 创建 zip ≥ 7-Zip 的 90%
//! - 创建 7z ≥ 7-Zip 的 50%
//!
//! 用 7z.exe 命令行而不是库：这是用户实际会拿来做对比的东西。路径从
//! `THZIP_7Z` 环境变量取，未设置时依次尝试常见安装位置；找不到就打印跳过。
//!
//! 运行：
//! ```text
//! cargo bench -p thzip-core --bench vs_7zip
//! THZIP_ENFORCE_REDLINE=1 cargo bench -p thzip-core --bench vs_7zip   # 未过线则失败
//! ```
//!
//! 结果不写进 criterion 汇总，因为对比对象是外部进程，统计口径不同。
//! 这里对每项跑 `REPEATS` 次取最小值：进程启动、磁盘缓存抖动的噪声都在上方，
//! 取最小值更接近真实吞吐。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::{ExtractDirSink, TempDir};
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveKind, Method};
use thzip_core::writer::{scan_inputs, write_entries};

const REPEATS: usize = 3;
/// 数据集规模。太小时进程启动开销会淹没差异，太大则基准跑太久。
const RANDOM_MIB: usize = 12;
const TEXT_MIB: usize = 12;
const SMALL_FILES: usize = 200;

fn main() {
    let Some(seven_zip) = find_seven_zip() else {
        println!("未找到 7z.exe（设置 THZIP_7Z 指向它），跳过对比基准。");
        return;
    };
    println!("7-Zip: {}", seven_zip.display());

    let dir = TempDir::create("thzip-bench-vs7z").expect("temp dir");
    let data = dir.path().join("data");
    build_dataset(&data);
    let total = dir_size(&data);
    println!("数据集: {} ({:.1} MiB)", data.display(), mib(total));

    let limits = Limits::default();
    let cancel = CancelToken::new();
    let entries = scan_inputs(std::slice::from_ref(&data), &limits).expect("scan");
    println!("条目数: {}", entries.len());

    let mut failures = Vec::new();

    // ---- 创建 zip ----
    let ours_zip = best_of(|| {
        let out = dir.path().join("ours.zip");
        let _ = std::fs::remove_file(&out);
        let mut writer =
            backend::create_writer(&out, ArchiveKind::Zip, Method::Deflate).expect("writer");
        write_entries(
            writer.as_mut(),
            &entries,
            Some(Method::Deflate),
            &cancel,
            &|_| {},
        )
        .expect("write");
        writer.finish().expect("finish");
        out
    });
    let theirs_zip = best_of(|| {
        let out = dir.path().join("theirs.zip");
        let _ = std::fs::remove_file(&out);
        run_7z(
            &seven_zip,
            &["a", "-tzip", "-mx5", path_arg(&out), path_arg(&data)],
        );
        out
    });
    report("创建 zip", ours_zip, theirs_zip, 0.90, &mut failures);

    // ---- 创建 7z ----
    let ours_7z = best_of(|| {
        let out = dir.path().join("ours.7z");
        let _ = std::fs::remove_file(&out);
        let mut writer =
            backend::create_writer(&out, ArchiveKind::SevenZ, Method::Lzma2).expect("writer");
        write_entries(
            writer.as_mut(),
            &entries,
            Some(Method::Lzma2),
            &cancel,
            &|_| {},
        )
        .expect("write");
        writer.finish().expect("finish");
        out
    });
    let theirs_7z = best_of(|| {
        let out = dir.path().join("theirs.7z");
        let _ = std::fs::remove_file(&out);
        run_7z(
            &seven_zip,
            // 与本项目 SevenZOptions::default() 的 level 3 保持同口径。
            &["a", "-t7z", "-mx3", path_arg(&out), path_arg(&data)],
        );
        out
    });
    report("创建 7z", ours_7z, theirs_7z, 0.50, &mut failures);

    // ---- 解压 zip ----
    // 两边都解同一个包（我们的 zip），保证输入一致。
    let extract_src = dir.path().join("theirs.zip");
    assert!(extract_src.exists(), "解压基准需要先产出 zip");

    let ours_extract = best_of(|| {
        let out = dir.path().join("out-ours");
        let _ = std::fs::remove_dir_all(&out);
        extract_with_core(&extract_src, &out, &limits);
        out
    });
    let theirs_extract = best_of(|| {
        let out = dir.path().join("out-7z");
        let _ = std::fs::remove_dir_all(&out);
        run_7z(
            &seven_zip,
            &[
                "x",
                "-y",
                &format!("-o{}", out.display()),
                path_arg(&extract_src),
            ],
        );
        out
    });
    report(
        "解压 zip",
        ours_extract,
        theirs_extract,
        0.80,
        &mut failures,
    );

    println!();
    if failures.is_empty() {
        println!("三条红线全部达成。");
    } else {
        for line in &failures {
            println!("未达标: {line}");
        }
        if std::env::var("THZIP_ENFORCE_REDLINE").is_ok() {
            std::process::exit(1);
        }
        println!("（设置 THZIP_ENFORCE_REDLINE=1 可让未达标变为失败）");
    }
}

fn report(label: &str, ours: Duration, theirs: Duration, redline: f64, failures: &mut Vec<String>) {
    let ratio = theirs.as_secs_f64() / ours.as_secs_f64();
    let status = if ratio >= redline { "OK  " } else { "FAIL" };
    println!(
        "{status} {label:<10} 本项目 {:>8.2}s   7-Zip {:>8.2}s   比值 {:>5.2}x（红线 {:.2}x）",
        ours.as_secs_f64(),
        theirs.as_secs_f64(),
        ratio,
        redline
    );
    if ratio < redline {
        failures.push(format!("{label}: {ratio:.2}x < {redline:.2}x"));
    }
}

fn best_of(run: impl Fn() -> PathBuf) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _ = run();
        best = best.min(start.elapsed());
    }
    best
}

fn extract_with_core(archive: &Path, target: &Path, limits: &Limits) {
    let mut reader = backend::open(archive, None).expect("open");
    let ids: Vec<_> = reader.entries().iter().map(|entry| entry.id).collect();
    let plan = reader.plan(&ids, target, limits).expect("plan");
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let mut sink = ExtractDirSink::new(&plan, &decisions, *limits);
    reader
        .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
        .expect("extract");
}

fn run_7z(exe: &Path, args: &[&str]) {
    let status = Command::new(exe)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("运行 7z 失败");
    assert!(status.success(), "7z 返回失败: {args:?}");
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("基准路径必须是 UTF-8")
}

fn find_seven_zip() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("THZIP_7Z") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }
    [
        "C:/Program Files/7-Zip/7z.exe",
        "C:/Program Files (x86)/7-Zip/7z.exe",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

/// 混合数据集：可压缩文本 + 不可压缩随机数据 + 大量小文件。
/// 只用一种数据会让某一方的算法优势被放大，对比失去意义。
fn build_dataset(dir: &Path) {
    std::fs::create_dir_all(dir).expect("mkdir");

    let text = "thzip-spellarchive benchmark line, the quick brown fox jumps over the lazy dog\n"
        .repeat(TEXT_MIB * 1024 * 1024 / 78);
    std::fs::write(dir.join("text.txt"), text.as_bytes()).expect("write text");

    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut random = Vec::with_capacity(RANDOM_MIB * 1024 * 1024);
    for _ in 0..RANDOM_MIB * 1024 * 1024 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        random.push((state & 0xFF) as u8);
    }
    std::fs::write(dir.join("random.bin"), &random).expect("write random");

    let small_dir = dir.join("small");
    std::fs::create_dir_all(&small_dir).expect("mkdir small");
    for index in 0..SMALL_FILES {
        let body = format!("file {index}: {}", "x".repeat(index % 2048));
        std::fs::write(small_dir.join(format!("f{index:03}.txt")), body).expect("write small");
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
