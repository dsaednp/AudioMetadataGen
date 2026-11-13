use crate::models::BookMetadata;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// 使用带缓冲的写入器将 BookMetadata 格式化输出到指定目录的 metadata.json
pub fn write_metadata_json(dir: &Path, metadata: &BookMetadata) -> std::io::Result<()> {
    let output_path = dir.join("metadata.json");
    let file = File::create(output_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, metadata)?;
    Ok(())
}
