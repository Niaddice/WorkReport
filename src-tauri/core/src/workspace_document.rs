use crate::{directory_ops, excel, workspace::Workspace};
use calamine::Reader;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
};

const TEXT_LIMIT: u64 = 4 * 1024 * 1024;
const TABLE_LIMIT: u64 = 20 * 1024 * 1024;
const ROWS: usize = 240;
const COLUMNS: usize = 80;
const TEXT_EXTENSIONS: &[&str] = &[
    "md",
    "markdown",
    "txt",
    "text",
    "log",
    "sql",
    "java",
    "py",
    "xml",
    "yml",
    "yaml",
    "json",
    "jsonc",
    "js",
    "jsx",
    "ts",
    "tsx",
    "mjs",
    "cjs",
    "vue",
    "svelte",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "cs",
    "go",
    "rs",
    "rb",
    "php",
    "sh",
    "bash",
    "zsh",
    "ps1",
    "bat",
    "cmd",
    "toml",
    "ini",
    "conf",
    "config",
    "cfg",
    "properties",
    "svg",
    "lock",
    "r",
    "kt",
    "kts",
    "swift",
    "gradle",
    "lua",
    "pl",
];
const COMPLEX: &[&str] = &[
    "doc", "docx", "docm", "dot", "dotx", "rtf", "odt", "ppt", "pptx", "pptm", "odp", "pdf",
    "pages", "key", "numbers", "ods", "zip", "rar", "7z", "gz", "tar", "bz2", "xz", "jar", "war",
    "class", "pyc", "exe", "dll", "so", "dylib", "app", "png", "jpg", "jpeg", "gif", "webp", "bmp",
    "tif", "tiff", "ico", "heic", "mp3", "wav", "mp4", "mov", "avi", "mkv", "sqlite", "sqlite3",
    "db", "parquet", "feather", "bin", "wasm",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub path: String,
    pub filename: String,
    pub kind: &'static str,
    pub content: String,
    pub revision: String,
    pub encoding: String,
    pub markdown: bool,
    pub journal_date: Option<String>,
    pub sheets: Vec<excel::ExcelSheetPreview>,
    pub message: String,
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn table(extension: &str) -> bool {
    ["xlsx", "xlsm", "xls", "xlsb", "csv", "tsv"].contains(&extension)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| format!("无法读取文件：{error}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("请选择普通文件".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "文件超过 {} MB，暂不支持在应用内打开",
            limit / 1024 / 1024
        ));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8], legacy: bool) -> Result<(String, String), String> {
    let (text, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let decoder = if little {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let (value, errors) = decoder.decode_without_bom_handling(&bytes[2..]);
        if errors {
            return Err("文件编码无效，无法安全编辑".into());
        }
        (
            value.into_owned(),
            if little { "utf-16le" } else { "utf-16be" }.into(),
        )
    } else if let Ok(value) = std::str::from_utf8(bytes) {
        (
            value.strip_prefix('\u{feff}').unwrap_or(value).into(),
            if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                "utf-8-bom"
            } else {
                "utf-8"
            }
            .into(),
        )
    } else if legacy {
        let (value, errors) = encoding_rs::GB18030.decode_without_bom_handling(bytes);
        if errors {
            return Err("此文件不是支持的文本编码（UTF-8、UTF-16 或 GB18030）".into());
        }
        (value.into_owned(), "gb18030".into())
    } else {
        return Err("此文件不是可安全编辑的文本文件".into());
    };
    if text
        .chars()
        .any(|ch| ch.is_control() && !['\n', '\r', '\t', '\u{c}'].contains(&ch))
    {
        return Err("文件包含二进制内容，暂不支持查看或编辑".into());
    }
    Ok((text, encoding))
}
fn encode(content: &str, encoding: &str) -> Result<Vec<u8>, String> {
    match encoding {
        "utf-8" => Ok(content.as_bytes().to_vec()),
        "utf-8-bom" => Ok([&[0xef, 0xbb, 0xbf][..], content.as_bytes()].concat()),
        "utf-16le" | "utf-16be" => {
            let little = encoding == "utf-16le";
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            Ok(bytes)
        }
        "gb18030" => {
            let (bytes, _, errors) = encoding_rs::GB18030.encode(content);
            if errors {
                Err("当前编码无法保存部分字符，草稿已保留".into())
            } else {
                Ok(bytes.into_owned())
            }
        }
        _ => Err("不支持的文本编码".into()),
    }
}
fn journal_date(workspace: &Workspace, relative: &str) -> Result<Option<String>, String> {
    Ok(workspace
        .managed_journal_files()?
        .into_iter()
        .find(|record| record.relative_path == relative)
        .map(|record| record.date))
}
pub fn read(workspace: &Workspace, relative: &str) -> Result<Document, String> {
    let target = directory_ops::visible_path(workspace.root(), relative, false)?;
    let metadata = fs::metadata(&target).map_err(|_| "文件已不存在，请刷新目录后重试")?;
    if !metadata.is_file() {
        return Err("请选择文件，文件夹可在目录中展开".into());
    }
    let ext = extension(&target);
    let mut document = Document {
        path: relative.into(),
        filename: target.file_name().unwrap().to_string_lossy().into_owned(),
        kind: "unsupported",
        content: String::new(),
        revision: String::new(),
        encoding: String::new(),
        markdown: ["md", "markdown"].contains(&ext.as_str()),
        journal_date: None,
        sheets: Vec::new(),
        message: String::new(),
    };
    if COMPLEX.contains(&ext.as_str()) || directory_ops::is_protected(workspace, relative) {
        document.message =
            "暂不支持此文件格式。支持文本文件查看和编辑，以及 Excel、CSV 表格只读预览。".into();
        return Ok(document);
    }
    if table(&ext) {
        if metadata.len() > TABLE_LIMIT {
            document.message = "表格文件超过 20 MB，暂不支持预览".into();
            return Ok(document);
        }
        document.kind = "table";
        document.sheets = if ["xlsx", "xlsm"].contains(&ext.as_str()) {
            excel::read_export(&target)?
        } else if ["csv", "tsv"].contains(&ext.as_str()) {
            let bytes = read_limited(&target, TABLE_LIMIT)?;
            let (text, _) = decode(&bytes, true)?;
            let (rows, truncated) = csv_rows(&text, ext == "tsv")?;
            vec![excel::plain_sheet(
                &document.filename,
                rows,
                1,
                1,
                truncated,
            )]
        } else {
            document.message = "旧版 Excel 以单元格内容为主显示，部分样式可能与原文件不同。".into();
            legacy_sheets(&target)?
        };
        return Ok(document);
    }
    if metadata.len() > TEXT_LIMIT {
        document.message = "文本文件超过 4 MB，暂不支持在应用内编辑".into();
        return Ok(document);
    }
    let bytes = read_limited(&target, TEXT_LIMIT)?;
    match decode(&bytes, TEXT_EXTENSIONS.contains(&ext.as_str())) {
        Ok((content, encoding)) => {
            document.kind = "text";
            document.content = content;
            document.encoding = encoding;
            document.revision = hash(&bytes);
            document.journal_date = journal_date(workspace, relative)?;
        }
        Err(message) => document.message = message,
    }
    Ok(document)
}
pub fn save(
    workspace: &Workspace,
    relative: &str,
    content: &str,
    expected: &str,
) -> Result<Document, String> {
    let target = directory_ops::visible_path(workspace.root(), relative, false)?;
    let ext = extension(&target);
    if table(&ext)
        || COMPLEX.contains(&ext.as_str())
        || directory_ops::is_protected(workspace, relative)
    {
        return Err("此文件只支持预览或暂不支持打开，不能编辑保存".into());
    }
    if journal_date(workspace, relative)?.is_some() {
        return Err("文件已关联日报，请重新从目录打开，以使用日报保存流程".into());
    }
    let before = read_limited(&target, TEXT_LIMIT)?;
    if hash(&before) != expected {
        return Err("文件已被其他程序修改或替换。草稿已保留，请先复制草稿，再重新载入。".into());
    }
    let (old, encoding) = decode(&before, TEXT_EXTENSIONS.contains(&ext.as_str()))?;
    let content = if old.contains("\r\n") && !old.replace("\r\n", "").contains('\n') {
        content.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        content.into()
    };
    let bytes = encode(&content, &encoding)?;
    if bytes.len() as u64 > TEXT_LIMIT {
        return Err("内容超过 4 MB，草稿已保留".into());
    }
    let permissions = fs::metadata(&target)
        .map_err(|e| e.to_string())?
        .permissions();
    if permissions.readonly() {
        return Err("此文件为只读文件，无法保存，草稿已保留".into());
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(target.parent().unwrap()).map_err(|e| e.to_string())?;
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    directory_ops::visible_path(workspace.root(), relative, false)?;
    if fs::read(&target).map_err(|e| e.to_string())? != before {
        return Err("保存期间文件发生外部修改，未覆盖文件，草稿已保留".into());
    }
    temporary
        .persist(&target)
        .map_err(|e| format!("无法保存文件，原文件保持不变：{}", e.error))?;
    read(workspace, relative)
}
fn csv_rows(text: &str, tsv: bool) -> Result<(Vec<Vec<String>>, bool), String> {
    let mut counts = [0usize; 3];
    let mut quote = false;
    let mut probe = text.chars().peekable();
    while let Some(ch) = probe.next() {
        if ch == '"' {
            if quote && probe.peek() == Some(&'"') {
                probe.next();
            } else {
                quote = !quote;
            }
        }
        if !quote {
            if ch == '\n' || ch == '\r' {
                break;
            }
            for (index, delimiter) in [',', ';', '\t'].iter().enumerate() {
                if ch == *delimiter {
                    counts[index] += 1;
                }
            }
        }
    }
    let delimiter = if tsv {
        '\t'
    } else {
        [',', ';', '\t'][counts
            .iter()
            .enumerate()
            .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
            .filter(|(_, count)| **count > 0)
            .map(|(index, _)| index)
            .unwrap_or(0)]
    };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut truncated = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    if field.len() < 128_000 {
                        field.push('"');
                    } else {
                        truncated = true;
                    }
                } else {
                    quoted = false;
                    closed = true;
                }
            } else if field.len() < 128_000 {
                field.push(ch);
            } else {
                truncated = true;
            }
            continue;
        }
        if ch == delimiter || ch == '\n' || ch == '\r' {
            if row.len() < COLUMNS {
                row.push(field.chars().take(32_000).collect());
            } else {
                truncated = true;
            }
            if field.chars().count() > 32_000 {
                truncated = true;
            }
            field.clear();
            closed = false;
            if ch != delimiter {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                rows.push(std::mem::take(&mut row));
                if rows.len() == ROWS && chars.peek().is_some() {
                    return Ok((rows, true));
                }
            }
        } else if ch == '"' && field.is_empty() && !closed {
            quoted = true;
        } else if closed {
            if !ch.is_whitespace() {
                return Err("CSV 引号或分隔符格式不正确，无法预览".into());
            }
        } else if ch == '"' {
            return Err("CSV 引号格式不正确，无法预览".into());
        } else if field.len() < 128_000 {
            field.push(ch);
        } else {
            truncated = true;
        }
    }
    if quoted {
        return Err("CSV 中有未闭合的引号，无法预览".into());
    }
    if !field.is_empty() || !row.is_empty() || closed || text.ends_with(delimiter) {
        if row.len() < COLUMNS {
            if field.chars().count() > 32_000 {
                truncated = true;
            }
            row.push(field.chars().take(32_000).collect());
        } else {
            truncated = true;
        }
        rows.push(row);
    }
    Ok((rows, truncated))
}
fn legacy_sheets(path: &Path) -> Result<Vec<excel::ExcelSheetPreview>, String> {
    catch_unwind(AssertUnwindSafe(|| {
        let mut book = calamine::open_workbook_auto(path)
            .map_err(|_| "无法解析 Excel 文件，文件可能已损坏或加密")?;
        let names = book.sheet_names();
        let too_many = names.len() > 16;
        let mut sheets = Vec::new();
        for name in names.into_iter().take(16) {
            let range = book
                .worksheet_range(&name)
                .map_err(|_| "无法读取 Excel 工作表")?;
            let start = range.start().unwrap_or((0, 0));
            let (height, width) = range.get_size();
            let mut truncated = too_many || height > ROWS || width > COLUMNS;
            let values = range
                .rows()
                .take(ROWS)
                .map(|row| {
                    row.iter()
                        .take(COLUMNS)
                        .map(|value| {
                            let value = match value {
                                calamine::Data::DateTime(value) if !value.is_duration() => value
                                    .as_datetime()
                                    .map(|date| date.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| value.to_string()),
                                _ => value.to_string(),
                            };
                            if value.chars().count() > 32_000 {
                                truncated = true;
                            }
                            value.chars().take(32_000).collect()
                        })
                        .collect()
                })
                .collect();
            sheets.push(excel::plain_sheet(
                &name,
                values,
                start.0 + 1,
                start.1 + 1,
                truncated,
            ));
        }
        Ok(sheets)
    }))
    .map_err(|_| "Excel 文件结构无效，无法预览".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Workspace) {
        let root = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(root.path().to_str().unwrap()).unwrap();
        (root, workspace)
    }
    #[test]
    fn source_files_and_unknown_plain_text_are_editable_without_creating_journals() {
        let (root, mut workspace) = fixture();
        for name in [
            "notes.md",
            "notes.txt",
            "query.sql",
            "Main.java",
            "main.py",
            "app.xml",
            "app.yml",
            "app.yaml",
            "Dockerfile",
            "custom.other",
        ] {
            fs::write(root.path().join(name), "初始文本\n").unwrap();
            let document = read(&workspace, name).unwrap();
            assert_eq!(document.kind, "text", "{name}");
            assert!(document.journal_date.is_none());
            let saved = save(&workspace, name, "修改后\n", &document.revision).unwrap();
            assert_eq!(saved.content, "修改后\n");
            assert_eq!(
                fs::read_to_string(root.path().join(name)).unwrap(),
                "修改后\n"
            );
        }
        assert!(workspace.list().unwrap().is_empty());
    }
    #[test]
    fn office_documents_and_binary_payloads_are_never_editable() {
        let (root, workspace) = fixture();
        for name in [
            "report.docx",
            "deck.pptx",
            "legacy.doc",
            "legacy.ppt",
            "document.pdf",
            "archive.zip",
            "database.sqlite",
        ] {
            fs::write(root.path().join(name), "even if content looks like text").unwrap();
            assert_eq!(read(&workspace, name).unwrap().kind, "unsupported");
            assert!(save(&workspace, name, "bad", "anything").is_err());
        }
        fs::write(root.path().join("binary.txt"), b"hello\0world").unwrap();
        assert_eq!(read(&workspace, "binary.txt").unwrap().kind, "unsupported");
        fs::write(root.path().join("unknown.dat"), [0xff, 0x01, 0x0, 0x10]).unwrap();
        assert_eq!(read(&workspace, "unknown.dat").unwrap().kind, "unsupported");
    }
    #[test]
    fn original_encodings_bom_line_endings_and_external_changes_are_preserved() {
        let (root, workspace) = fixture();
        for encoding in ["utf-8-bom", "utf-16le", "utf-16be", "gb18030"] {
            let original = encode("中文\r\n第二行\r\n", encoding).unwrap();
            fs::write(root.path().join("source.txt"), &original).unwrap();
            let document = read(&workspace, "source.txt").unwrap();
            assert_eq!(document.encoding, encoding);
            save(
                &workspace,
                "source.txt",
                "修改\n第二行\n",
                &document.revision,
            )
            .unwrap();
            assert_eq!(
                fs::read(root.path().join("source.txt")).unwrap(),
                encode("修改\r\n第二行\r\n", encoding).unwrap()
            );
        }
        let document = read(&workspace, "source.txt").unwrap();
        fs::write(root.path().join("source.txt"), "external").unwrap();
        assert!(
            save(&workspace, "source.txt", "overwrite", &document.revision)
                .unwrap_err()
                .contains("其他程序")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("source.txt")).unwrap(),
            "external"
        );
    }
    #[test]
    fn csv_handles_quotes_multiline_fields_delimiters_and_bounded_readonly_preview() {
        let (root, workspace) = fixture();
        let path = root.path().join("report.csv");
        let bytes = b"name,description,amount\r\n\"A,B\",\"line1\nline2 \"\"quoted\"\"\",42\r\n";
        fs::write(&path, bytes).unwrap();
        let document = read(&workspace, "report.csv").unwrap();
        assert_eq!(document.kind, "table");
        let cells = &document.sheets[0].regions[0].cells;
        assert_eq!(
            cells.iter().find(|cell| cell.address == "A2").unwrap().text,
            "A,B"
        );
        assert_eq!(
            cells.iter().find(|cell| cell.address == "B2").unwrap().text,
            "line1\nline2 \"quoted\""
        );
        assert!(save(&workspace, "report.csv", "overwrite", "").is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(csv_rows("a;b\n1;2", false).unwrap().0[1], ["1", "2"]);
        assert_eq!(csv_rows("a\tb\n1\t2", true).unwrap().0[1], ["1", "2"]);
        assert!(csv_rows("a,\"bad", false).is_err());
        let long = (0..300)
            .map(|index| format!("{index},value\n"))
            .collect::<String>();
        let (rows, truncated) = csv_rows(&long, false).unwrap();
        assert!(truncated);
        assert_eq!(rows.len(), ROWS);
    }
    #[test]
    fn xlsx_is_previewed_with_styles_and_never_written() {
        let (root, workspace) = fixture();
        let path = root.path().join("report.xlsx");
        let mut book = umya_spreadsheet::new_file();
        book.sheet_by_name_mut("Sheet1")
            .unwrap()
            .cell_mut("A1")
            .set_value_string("表格内容");
        umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();
        let original = fs::read(&path).unwrap();
        let document = read(&workspace, "report.xlsx").unwrap();
        assert_eq!(document.kind, "table");
        assert_eq!(document.sheets[0].regions[0].cells[0].text, "表格内容");
        assert!(save(&workspace, "report.xlsx", "bad", "").is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn legacy_xls_shows_text_numbers_and_dates_without_writing() {
        let (root, workspace) = fixture();
        let original = include_bytes!("../test-fixtures/legacy.xls");
        fs::write(root.path().join("legacy.xls"), original).unwrap();
        let document = read(&workspace, "legacy.xls").unwrap();
        assert_eq!(document.kind, "table");
        assert_eq!(document.sheets[0].name, "数据");
        let cells = &document.sheets[0].regions[0].cells;
        assert_eq!(
            cells.iter().find(|cell| cell.address == "A1").unwrap().text,
            "中文内容"
        );
        assert_eq!(
            cells.iter().find(|cell| cell.address == "B2").unwrap().text,
            "42.5"
        );
        assert_eq!(
            cells.iter().find(|cell| cell.address == "C2").unwrap().text,
            "2026-11-01 09:30:00"
        );
        assert!(save(&workspace, "legacy.xls", "bad", "").is_err());
        assert_eq!(fs::read(root.path().join("legacy.xls")).unwrap(), original);
    }
    #[test]
    fn managed_journals_keep_their_domain_save_path_and_workspace_boundaries_hold() {
        let (root, mut workspace) = fixture();
        workspace
            .create_with_content("20261101", crate::workspace::Format::Md, "journal")
            .unwrap();
        let document = read(&workspace, "2026/20261101.md").unwrap();
        assert_eq!(document.journal_date.as_deref(), Some("20261101"));
        assert!(save(&workspace, "2026/20261101.md", "bad", &document.revision).is_err());
        for path in [
            "../outside.txt",
            "/etc/hosts",
            ".workreport-trash/item",
            "workreport.json",
        ] {
            assert!(read(&workspace, path).is_err());
        }
        let big = root.path().join("big.txt");
        fs::File::create(&big)
            .unwrap()
            .set_len(TEXT_LIMIT + 1)
            .unwrap();
        assert_eq!(read(&workspace, "big.txt").unwrap().kind, "unsupported");
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_rejected_and_executable_permissions_survive_text_edits() {
        use std::os::unix::{fs::symlink, fs::PermissionsExt};
        let (root, workspace) = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("outside.txt"), "outside").unwrap();
        symlink(
            outside.path().join("outside.txt"),
            root.path().join("link.txt"),
        )
        .unwrap();
        assert!(read(&workspace, "link.txt").is_err());
        let target = root.path().join("run.py");
        fs::write(&target, "print('old')\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        let document = read(&workspace, "run.py").unwrap();
        save(&workspace, "run.py", "print('new')\n", &document.revision).unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
        let document = read(&workspace, "run.py").unwrap();
        assert!(save(&workspace, "run.py", "bad", &document.revision)
            .unwrap_err()
            .contains("只读"));
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
