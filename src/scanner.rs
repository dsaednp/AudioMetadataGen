use ignore::{DirEntry, WalkBuilder, WalkState};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// 扫描根目录，按音频所在的直接父目录分组，并对每个目录内的文件进行并行自然排序
/// 返回: Vec<(目录路径, 该目录下的有序音频文件路径列表)>
pub fn scan_audio_grouped_by_dir<P: AsRef<Path>>(root_dir: P) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let (tx, rx) = mpsc::channel();

    let walker = WalkBuilder::new(root_dir)
        .hidden(true)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .parents(false)
        .follow_links(false)
        .build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |result| {
            if let Ok(entry) = result {
                if is_valid_audio_file(&entry) {
                    let _ = tx.send(entry.into_path());
                }
            }
            WalkState::Continue
        })
    });

    drop(tx);

    // 1. 单线程无锁聚合进 HashMap（内存哈希插入耗时极低，通常几万个文件仅需几毫秒）
    let mut dir_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in rx {
        if let Some(parent_dir) = path.parent() {
            dir_map
                .entry(parent_dir.to_path_buf())
                .or_default()
                .push(path);
        }
    }

    // 2. 转换为 Vec 并利用 Rayon 多核并行对【每个目录内部】的音频进行自然排序
    let mut grouped_dirs: Vec<(PathBuf, Vec<PathBuf>)> = dir_map.into_iter().collect();

    grouped_dirs.par_iter_mut().for_each(|(_dir, files)| {
        files.sort_unstable_by(|a, b| {
            // 只比较文件名部分，速度更快且避免父路径干扰
            let a_name = a.file_name().unwrap_or_default().to_string_lossy();
            let b_name = b.file_name().unwrap_or_default().to_string_lossy();
            natord::compare(&a_name, &b_name)
        });
    });

    // 3. 外层目录本身也按自然顺序排好（如文件夹 1, 2, 10）
    grouped_dirs.par_sort_unstable_by(|(dir_a, _), (dir_b, _)| {
        let a_str = dir_a.to_string_lossy();
        let b_str = dir_b.to_string_lossy();
        natord::compare(&a_str, &b_str)
    });

    grouped_dirs
}

#[inline(always)]
fn is_valid_audio_file(entry: &DirEntry) -> bool {
    let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
    if !is_file {
        return false;
    }

    match entry.path().extension().and_then(|ext| ext.to_str()) {
        Some(ext) => {
            ext.eq_ignore_ascii_case("mp3")
                || ext.eq_ignore_ascii_case("m4a")
                || ext.eq_ignore_ascii_case("m4b")
        }
        None => false,
    }
}
