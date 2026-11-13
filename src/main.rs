mod exporter;
mod models;
mod parser;
mod scanner;

use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: {} <有声书目录路径>", args[0]);
        eprintln!("示例:");
        eprintln!("  {} .", args[0]);
        eprintln!("  {} /path/to/audiobook", args[0]);
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
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

    // 2. 多核并行解析每个目录并直接写入各自的 metadata.json
    grouped_books.par_iter().for_each(|(dir, files)| {
        let book_meta = parser::parse_book_folder(dir, files);
        if let Err(e) = exporter::write_metadata_json(dir, &book_meta) {
            eprintln!("写入失败 [{}]: {}", dir.display(), e);
        }
    });

    println!("全部处理完成！总耗时: {:?}", total_start.elapsed());
}
