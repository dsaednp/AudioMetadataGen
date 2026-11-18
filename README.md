# AudioMetadata

一个用 Rust 编写的高性能有声书元数据提取工具。它会递归扫描指定目录，读取音频文件的标签信息，并为每个包含音频的文件夹生成一份符合 [Audiobookshelf](https://www.audiobookshelf.org/) 规范的 `metadata.json`。

## 功能特性

- **并行扫描**：基于 `ignore` 的并行目录遍历 + `rayon` 多核解析，充分利用多核 CPU。
- **自然排序**：使用 `natord` 对文件名和目录名排序，`2.mp3` 会正确排在 `10.mp3` 之前。
- **多格式支持**：识别 `.mp3`、`.m4a`、`.m4b`（扩展名不区分大小写）。
- **多标签格式**：通过 `lofty` 读取 ID3v2、Vorbis Comments、MP4 ilst 等标签。
- **系列识别**：自动兼容多种系列字段写法，并输出 `系列名 #序号` 格式。
- **按目录成书**：每个音频所在的直接父目录被视为一本书，分别输出元数据。
- **跳过封面**：解析时关闭封面读取（`read_cover_art(false)`），减少 IO 与内存开销。

## 环境要求

- Rust 1.85 或更高版本（项目使用 `edition = "2024"`）
- 无需任何外部系统依赖

## 构建与安装

```bash
# 开发构建
cargo build

# 发布构建（推荐，扫描性能更好）
cargo build --release

# 安装到 ~/.cargo/bin
cargo install --path .

# 安装到自定义目录
cargo install --path . --root ~/.cargo/self
```

构建产物为 `target/release/audiomd`。

## 使用方法

```bash
# 扫描当前目录
audiomd .

# 扫描指定目录
audiomd /path/to/audiobooks

# 强制覆盖已存在的 metadata.json
audiomd -f /path/to/audiobooks
```

可用选项：

- `-f` / `--force`：即使目录中已存在 `metadata.json` 也重新生成并覆盖。
- `-h` / `--help`：显示帮助信息。

默认情况下，**已存在 `metadata.json` 的目录会被跳过**，不会被重新解析。

示例输出：

```text
开始扫描目录: /path/to/audiobooks
扫描完成，共发现 12 个有声书目录
跳过 3 个已存在 metadata.json 的目录（使用 -f/--force 可强制覆盖）
全部处理完成！已生成 9 个 metadata.json，跳过 3 个，总耗时: 1.23s
```

每个检测到音频文件的目录下会生成一个 `metadata.json`：

```text
有声书根目录/
├── 凡人修仙传/
│   ├── 001.mp3
│   ├── 002.mp3
│   └── metadata.json   ← 自动生成
└── 三体/
    ├── part-1.m4b
    └── metadata.json   ← 自动生成
```

### 行为说明

- 递归扫描，但**跳过隐藏文件和隐藏目录**（以 `.` 开头）。
- **不**读取 `.gitignore` / `.ignore`，目录下的所有非隐藏音频都会被扫描。
- **不**跟随符号链接。
- 同一目录内的音频按文件名自然排序，并以此顺序生成 `episodes` 的 `id`（从 1 开始）。
- 若目录中已存在 `metadata.json`，**默认跳过该目录**；需要重新生成时加 `-f` / `--force`。
- 单个文件解析失败不会中断整体流程，该集时长记为 `0.0`，标题回退为文件名（不含扩展名）。

## 输出格式

生成的 JSON 使用 camelCase 命名，结构如下：

```json
{
  "title": "凡人修仙传",
  "subtitle": null,
  "authors": ["忘语"],
  "narrators": ["演播者"],
  "series": ["凡人修仙传 #1"],
  "genres": ["玄幻", "修仙"],
  "publishedYear": "2009",
  "publishedDate": null,
  "publisher": null,
  "description": "一个普通山村小子……",
  "isbn": null,
  "asin": null,
  "language": "zh",
  "explicit": false,
  "abridged": false,
  "episodes": [
    {
      "id": 1,
      "filename": "001.mp3",
      "duration": 1832.5,
      "title": "第一章 山边小村"
    }
  ]
}
```

### 字段来源

书籍级字段取自目录中**第一个能成功读取标签**的音频文件：

- **title**：`Album` 标签，缺失时回退为文件夹名。
- **subtitle**：`SetSubtitle`。
- **authors**：优先 `AlbumArtist`，其次 `Artist`。
- **narrators**：优先 `Composer`，其次 `Performer`。
- **series**：按以下顺序尝试：
  1. `Movement` + `MovementNumber`（标准系列字段）；
  2. `ContentGroup`；
  3. 自定义字段 `SERIES` / `SERIES-PART`、`MVNM` / `MVIN`、`----:com.apple.iTunes:SERIES(-PART)`。
- **genres**：`Genre`，支持多值。
- **publishedYear**：`Date` 的年份；缺失时取 `RecordingDate` 前 4 位。
- **publisher**：`Publisher`。
- **description**：优先 `Description`，其次 `Comment`。
- **language**：`Language`。

单集字段：

- **id**：自然排序后的序号，从 1 开始。
- **filename**：音频文件名（含扩展名）。
- **duration**：音频时长，单位秒（浮点数）。
- **title**：音频标签中的标题，缺失时回退为文件名（不含扩展名）。

作者、演播者、流派等多值字段会按 `,`、`，`、`;`、`；`、`/` 自动拆分并去除空白。

系列条目会格式化为 `系列名 #序号`：序号会去掉空白与前导零，`0` 视为未填写；若系列名本身已包含 `#`，则原样保留。

### 当前未解析的字段

以下字段在结构体中保留，但尚未从标签读取，输出恒为默认值：

- `publishedDate`、`isbn`、`asin`：恒为 `null`。
- `explicit`、`abridged`：恒为 `false`。

## 项目结构

```text
src/
├── main.rs      # CLI 入口：参数校验、扫描、并行调度
├── scanner.rs   # 并行遍历目录、格式过滤、分组与自然排序
├── parser.rs    # 标签解析与 BookMetadata 组装
├── models.rs    # BookMetadata / Episode 数据结构
└── exporter.rs  # 将结果写入 metadata.json
```

## 工作原理

1. `scanner` 并行遍历目录，收集所有 `.mp3` / `.m4a` / `.m4b` 文件，按直接父目录分组。
2. 每个目录内的文件按文件名自然排序，目录之间也按自然顺序排序。
3. `parser` 并行解析每一集，并从首个可读标签的文件提取书籍级信息。
4. `exporter` 将结果以美化格式写入对应目录的 `metadata.json`。

## 许可证

本项目尚未在 `Cargo.toml` 中声明许可证。
