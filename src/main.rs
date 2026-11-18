mod exporter;
mod models;
mod parser;
mod scanner;

use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn print_usage(program: &str) {
    eprintln!("用法: {} [选项] <有声书目录路径>", program);
    eprintln!("选项:");
    eprintln!("  -f, --force   强制覆盖已存在的 metadata.json");
    eprintln!("  -h, --help    显示此帮助信息");
    eprintln!("示例:");
    eprintln!("  {} .", program);
    eprintln!("  {} -f /path/to/audiobook", program);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut force = false;
    let mut positional: Option<PathBuf> = None;
    let mut only_positional = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            _ if only_positional => {
                if positional.replace(PathBuf::from(arg)).is_some() {
                    eprintln!("错误: 只能指定一个目录路径");
                    process::exit(1);
                }
            }
            "--" => only_positional = true,
            "-f" | "--force" => force = true,
            "-h" | "--help" => {
                print_usage(&args[0]);
                return;
            }
            other if other.starts_with('-') && other != "-" => {
                eprintln!("错误: 未知选项 '{}'", other);
                print_usage(&args[0]);
                process::exit(1);
            }
            _ => {
                if positional.replace(PathBuf::from(arg)).is_some() {
                    eprintln!("错误: 只能指定一个目录路径");
                    process::exit(1);
                }
            }
        }
    }

    let Some(input_path) = positional else {
        print_usage(&args[0]);
        process::exit(1);
    };

    let target_dir = match fs::canonicalize(&input_path) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("无法访问路径 '{}': {}", input_path.display(), e);
            process::exit(1);
        }
    };

    if !target_dir.is_dir() {
        eprintln!("错误: '{}' 不是一个有效的目录", target_dir.display());
        process::exit(1);
    }

    println!("开始扫描目录: {}", target_dir.display());
    let total_start = Instant::now();

    // 1. 极速并行扫描并按文件夹分组
    let grouped_books = scanner::scan_audio_grouped_by_dir(&target_dir);
    println!("扫描完成，共发现 {} 个有声书目录", grouped_books.len());

    // 2. 默认跳过已存在 metadata.json 的目录；-f/--force 时全部重新生成
    let (pending, skipped): (Vec<_>, Vec<_>) = grouped_books
        .into_par_iter()
        .partition(|(dir, _)| force || !dir.join("metadata.json").is_file());

    if !skipped.is_empty() {
        println!(
            "跳过 {} 个已存在 metadata.json 的目录（使用 -f/--force 可强制覆盖）",
            skipped.len()
        );
    }

    // 3. 多核并行解析每个目录并直接写入各自的 metadata.json
    let failed = AtomicUsize::new(0);
    pending.par_iter().for_each(|(dir, files)| {
        let book_meta = parser::parse_book_folder(dir, files);
        if let Err(e) = exporter::write_metadata_json(dir, &book_meta) {
            eprintln!("写入失败 [{}]: {}", dir.display(), e);
            failed.fetch_add(1, Ordering::Relaxed);
        }
    });

    let written = pending.len() - failed.load(Ordering::Relaxed);
    println!(
        "全部处理完成！已生成 {} 个 metadata.json，跳过 {} 个，总耗时: {:?}",
        written,
        skipped.len(),
        total_start.elapsed()
    );
}
