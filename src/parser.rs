use crate::models::{BookMetadata, Episode};
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst};
use lofty::ogg::tag::VorbisComments;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagType};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// 并发解析单个文件夹下的所有音频文件，生成符合 Audiobookshelf 规范的 BookMetadata
pub fn parse_book_folder(dir: &Path, files: &[PathBuf]) -> BookMetadata {
    // 1. 并发提取每一集的元数据（保持自然排序后的顺序）
    let episodes: Vec<Episode> = files
        .par_iter()
        .enumerate()
        .map(|(idx, path)| parse_single_episode(idx + 1, path))
        .collect();

    // 2. 初始化书籍级默认元数据（标题默认回退为文件夹名）
    let folder_name = dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut metadata = BookMetadata {
        title: folder_name,
        subtitle: None,
        authors: Vec::new(),
        narrators: Vec::new(),
        series: Vec::new(),
        genres: Vec::new(),
        published_year: None,
        published_date: None,
        publisher: None,
        description: None,
        isbn: None,
        asin: None,
        language: None,
        explicit: false,
        abridged: false,
        episodes,
    };

    // 3. 从首个可读取标签的音频文件中提取书籍级元信息
    let options = ParseOptions::new().read_cover_art(false);
    for path in files {
        if let Ok(tagged_file) = Probe::open(path).and_then(|p| p.options(options).read()) {
            if let Some(tag) = tagged_file
                .primary_tag()
                .or_else(|| tagged_file.first_tag())
            {
                fill_book_details(&mut metadata, &tagged_file, tag);
                break;
            }
        }
    }

    metadata
}

/// 提取单个音频文件作为 Episode
fn parse_single_episode(id: usize, path: &Path) -> Episode {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let fallback_title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let options = ParseOptions::new().read_cover_art(false);

    if let Ok(tagged_file) = Probe::open(path).and_then(|p| p.options(options).read()) {
        let duration = tagged_file.properties().duration().as_secs_f64();

        let title = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())
            .and_then(|tag| tag.title().map(|s| s.trim().to_owned()))
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_title);

        Episode {
            id,
            filename,
            duration,
            title,
        }
    } else {
        Episode {
            id,
            filename,
            duration: 0.0,
            title: fallback_title,
        }
    }
}

/// 从标签中填充书籍级字段
fn fill_book_details(meta: &mut BookMetadata, tagged_file: &TaggedFile, tag: &Tag) {
    // 书名：优先取 Album
    if let Some(album) = tag.album() {
        let trimmed = album.trim();
        if !trimmed.is_empty() {
            meta.title = trimmed.to_owned();
        }
    }

    // 副标题
    meta.subtitle = get_string_item(tag, ItemKey::SetSubtitle);

    // 作者：优先 AlbumArtist，其次 Artist
    let raw_author =
        get_string_item(tag, ItemKey::AlbumArtist).or_else(|| tag.artist().map(|s| s.into_owned()));
    if let Some(author_str) = raw_author {
        meta.authors = split_multi_values(&author_str);
    }

    // 演播者：优先 Composer，其次 Performer
    let raw_narrator = get_string_item(tag, ItemKey::Composer)
        .or_else(|| get_string_item(tag, ItemKey::Performer));
    if let Some(narrator_str) = raw_narrator {
        meta.narrators = split_multi_values(&narrator_str);
    }

    // 系列信息（兼容 "凡人修仙传 #1" 格式）
    meta.series = extract_series(tagged_file, tag);

    // 流派/标签
    let mut genres = Vec::new();
    for item in tag.get_strings(ItemKey::Genre) {
        genres.extend(split_multi_values(item));
    }
    meta.genres = genres;

    // 发行年份
    if let Some(date) = tag.date() {
        meta.published_year = Some(date.year.to_string());
    } else if let Some(date_str) = get_string_item(tag, ItemKey::RecordingDate) {
        if date_str.len() >= 4 {
            meta.published_year = Some(date_str[..4].to_string());
        }
    }

    // 发行商
    meta.publisher = get_string_item(tag, ItemKey::Publisher);

    // 简介：优先 Description，其次 Comment
    meta.description = get_string_item(tag, ItemKey::Description).or_else(|| {
        tag.comment()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    });

    // 语言
    meta.language = get_string_item(tag, ItemKey::Language);
}

/// 提取系列信息，自动兼容 "系列名 #序号" 格式
fn extract_series(tagged_file: &TaggedFile, tag: &Tag) -> Vec<String> {
    let mut series_list = Vec::new();

    // 情况 A: 标准 Movement (系列名) + MovementNumber (系列序号)
    if let Some(series_name) = get_string_item(tag, ItemKey::Movement) {
        let series_num = get_string_item(tag, ItemKey::MovementNumber);
        series_list.push(format_series_entry(series_name, series_num));
        return series_list;
    }

    // 情况 B: 回退检查 ContentGroup (常见于存放系列名称)
    if let Some(group_str) = get_string_item(tag, ItemKey::ContentGroup) {
        series_list.extend(split_multi_values(&group_str));
        return series_list;
    }

    // 情况 C: 检查自定义标签键 (Tone / Mp3tag / iTunes freeform)
    let (custom_series, custom_part) = read_raw_custom_tags(tagged_file);

    if let Some(name) = custom_series {
        series_list.push(format_series_entry(name, custom_part));
    }

    series_list
}

/// 拼接 Audiobookshelf 风格的系列条目，如 "凡人修仙传 #1"
///
/// - 系列名本身已包含 '#'（如 "凡人修仙传 #1"）时原样保留；
/// - 序号为空或为 0 时只输出系列名。
fn format_series_entry(name: String, part: Option<String>) -> String {
    if name.contains('#') {
        return name;
    }
    match part.as_deref().map(normalize_series_part) {
        Some(part) if !part.is_empty() => format!("{} #{}", name, part),
        _ => name,
    }
}

/// 规整系列序号：去空白与前导零，"0" 视为未填写
fn normalize_series_part(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.parse::<u64>() {
        Ok(0) => String::new(),
        Ok(num) => num.to_string(),
        Err(_) => trimmed.to_owned(),
    }
}

/// 从各格式底层标签中读取自定义系列字段（SERIES / SERIES-PART / MVNM / MVIN）
fn read_raw_custom_tags(tagged_file: &TaggedFile) -> (Option<String>, Option<String>) {
    let mut custom_series = None;
    let mut custom_part = None;

    let mut match_kv = |key: &str, val: &str| {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            return;
        }
        if key.eq_ignore_ascii_case("SERIES")
            || key.eq_ignore_ascii_case("----:com.apple.iTunes:SERIES")
            || key.eq_ignore_ascii_case("MVNM")
            || key.eq_ignore_ascii_case("MOVEMENTNAME")
        {
            custom_series = Some(trimmed.to_owned());
        } else if key.eq_ignore_ascii_case("SERIES-PART")
            || key.eq_ignore_ascii_case("PART")
            || key.eq_ignore_ascii_case("----:com.apple.iTunes:SERIES-PART")
            || key.eq_ignore_ascii_case("MVIN")
            || key.eq_ignore_ascii_case("MOVEMENT")
        {
            custom_part = Some(trimmed.to_owned());
        }
    };

    for tag in tagged_file.tags() {
        match tag.tag_type() {
            TagType::VorbisComments => {
                let vorbis: VorbisComments = tag.clone().into();
                for (k, v) in vorbis.items() {
                    match_kv(k, v);
                }
            }
            TagType::Id3v2 => {
                let id3: Id3v2Tag = tag.clone().into();
                for frame in &id3 {
                    if let Frame::UserText(txxx) = frame {
                        match_kv(&txxx.description, &txxx.content);
                    }
                }
            }
            TagType::Mp4Ilst => {
                let ilst: Ilst = tag.clone().into();
                for atom in &ilst {
                    if let AtomIdent::Freeform { mean, name } = atom.ident() {
                        let full_key = format!("----:{}:{}", mean, name);
                        // 一个 atom 可能携带多个数据项，逐一读取其中的文本值
                        for data in atom.data() {
                            match data {
                                AtomData::UTF8(text) | AtomData::UTF16(text) => {
                                    match_kv(&full_key, text);
                                    match_kv(name.as_ref(), text);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (custom_series, custom_part)
}

#[inline]
fn get_string_item(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn split_multi_values(raw: &str) -> Vec<String> {
    raw.split(&[',', '，', ';', '；', '/'][..])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}
