use chrono::{Datelike, NaiveDate};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    fs,
    io::{Cursor, Read, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use umya_spreadsheet::{Border, CellValue, Color, Workbook, Worksheet};

#[path = "excel_preview.rs"]
mod preview;
pub use preview::ExcelSheetPreview;

const MAX_TEMPLATE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SHEETS: usize = 32;
const MAX_TEMPLATE_CELLS: usize = 50_000;
const MAX_CONTEXT_CELLS: usize = 2_000;
const MAX_CONTEXT_BYTES: usize = 180_000;
const MAX_PLAN_CELLS: usize = 500;
const MAX_STYLE_REGIONS: usize = 32;
const MAX_AFFECTED_CELLS: u64 = 5000;
const MAX_ROW_HEIGHT: f64 = 409.5;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CELL_TEXT: usize = 32_767;
const MAX_COLUMN: u32 = 16_384;
const MAX_ROW: u32 = 1_048_576;
const PREVIEW_LIFETIME: Duration = Duration::from_secs(30 * 60);
const MAX_PENDING_PREVIEWS: usize = 4;
const MAX_PENDING_BYTES: usize = 64 * 1024 * 1024;
const STALE_PREVIEW: &str = "Excel 预览已失效，请重新生成预览";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub enabled: bool,
    pub template_path: String,
    pub prompt: String,
    pub output_directory: String,
}

pub struct Template {
    workbook: Workbook,
    fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreview {
    pub id: String,
    pub template_name: String,
    pub output_directory: String,
    pub start: String,
    pub end: String,
    pub cells: Vec<ExcelPreviewCell>,
    pub styles: Vec<ExcelPreviewStyle>,
    pub sheets: Vec<ExcelSheetPreview>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreviewCell {
    pub sheet: String,
    pub cell: String,
    pub value: Value,
    pub previous_value: String,
    pub replaces_formula: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreviewStyle {
    pub sheet: String,
    pub range: String,
    pub border: Option<&'static str>,
    pub wrap_text: Option<bool>,
    pub auto_row_height: Option<bool>,
}

pub struct Provenance {
    pub workspace_path: String,
    pub dates: Vec<String>,
    pub settings: Settings,
    pub source_revision: String,
    pub template_fingerprint: String,
}

pub struct PendingPreview {
    preview: ExcelPreview,
    bytes: Vec<u8>,
    provenance: Provenance,
    output_path: PathBuf,
    created: Instant,
    cancel: CancellationToken,
}

#[derive(Clone, Default)]
pub struct Runtime(Arc<Mutex<VecDeque<PendingPreview>>>);

impl Runtime {
    pub fn insert(&self, pending: PendingPreview) -> Result<ExcelPreview, String> {
        check_cancelled(Some(&pending.cancel))?;
        if pending.bytes.len() > MAX_PENDING_BYTES {
            return Err("Excel 预览文件过大，请简化模板后重试".into());
        }
        let mut previews = self.0.lock().map_err(|_| "Excel 预览暂时无法保存")?;
        previews.retain(|preview| !preview.expired());
        while previews.len() >= MAX_PENDING_PREVIEWS
            || previews.iter().map(|preview| preview.bytes.len()).sum::<usize>() + pending.bytes.len() > MAX_PENDING_BYTES
        {
            previews.pop_front();
        }
        check_cancelled(Some(&pending.cancel))?;
        let result = pending.preview.clone();
        previews.push_back(pending);
        Ok(result)
    }

    pub fn discard(&self, id: &str) {
        if let Ok(mut previews) = self.0.lock() {
            previews.retain(|pending| pending.preview.id != id);
        }
    }

    pub fn confirm(
        &self,
        id: &str,
        validate: impl FnOnce(&Provenance) -> Result<(), String>,
    ) -> Result<ExportResult, String> {
        // Keep the cache locked through publication. A concurrent confirm can
        // never reuse these bytes or create a second file with the same token.
        let mut previews = self.0.try_lock().map_err(|_| "Excel 正在输出，请稍后重试")?;
        previews.retain(|preview| !preview.expired());
        let index = previews.iter().position(|pending| pending.preview.id == id).ok_or(STALE_PREVIEW)?;
        let pending = previews.remove(index).ok_or(STALE_PREVIEW)?;
        validate(&pending.provenance)?;
        let directory = Path::new(pending.provenance.settings.output_directory.trim());
        if fs::canonicalize(directory).ok().as_ref() != Some(&pending.output_path) {
            return Err("Excel 输出目录已变化，请重新生成预览".into());
        }
        let result = publish_bytes(&pending.bytes, directory, &pending.preview.start, &pending.preview.end, pending.preview.cells.len(), None);
        if result.is_err() {
            // No final file exists on publication failure. Keep the approved
            // plan available so a temporary disk/permission issue can be retried.
            previews.push_back(pending);
        }
        result
    }
}

impl PendingPreview {
    fn expired(&self) -> bool {
        self.created.elapsed() >= PREVIEW_LIFETIME || self.cancel.is_cancelled()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub filename: String,
    pub cells: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub filename: String,
    pub path: String,
    pub modified: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    pub filename: String,
    pub path: String,
    pub sheets: Vec<ExcelSheetPreview>,
}

/// 列出输出目录下已导出的 .xlsx，按修改时间从新到旧。
pub fn list_exports(directory: &Path) -> Result<Vec<ExportFile>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("无法读取 Excel 输出目录".into()),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_xlsx = path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("xlsx"));
        if !is_xlsx {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as u64)
            .unwrap_or(0);
        files.push(ExportFile {
            filename,
            path: path.to_string_lossy().into_owned(),
            modified,
            size: metadata.len(),
        });
    }
    files.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.filename.cmp(&left.filename))
    });
    Ok(files)
}

/// 读取已导出的工作簿，生成只读预览。
pub fn read_export(path: &Path) -> Result<Vec<ExcelSheetPreview>, String> {
    let bytes = fs::read(path).map_err(|_| "无法读取 Excel 文件".to_string())?;
    if bytes.len() as u64 > MAX_TEMPLATE_BYTES {
        return Err("Excel 文件过大，无法预览".into());
    }
    preflight(&bytes)?;
    let workbook = catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::reader::xlsx::read_reader(Cursor::new(bytes), true)
    }))
    .map_err(|_| "Excel 文件结构无效".to_string())?
    .map_err(|_| "无法解析 Excel 文件".to_string())?;
    if workbook.sheet_collection().is_empty() {
        return Err("Excel 文件没有工作表".into());
    }
    preview::extract_static(&workbook)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FillPlan {
    #[serde(default)]
    cells: Vec<FillCell>,
    #[serde(default)]
    styles: Vec<FillStyle>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FillCell {
    sheet: String,
    cell: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FillStyle {
    sheet: String,
    range: String,
    border: Option<BorderColor>,
    wrap_text: Option<bool>,
    auto_row_height: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BorderColor {
    Black,
}

#[derive(Clone, Copy)]
struct CellRange {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl CellRange {
    fn area(self) -> u64 {
        u64::from(self.right - self.left + 1) * u64::from(self.bottom - self.top + 1)
    }

    fn contains(self, column: u32, row: u32) -> bool {
        (self.left..=self.right).contains(&column) && (self.top..=self.bottom).contains(&row)
    }
}

pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    if settings.prompt.chars().count() > 6000 {
        return Err("Excel 输出提示词最多 6000 字".into());
    }
    if settings.template_path.len() > 4096 || settings.output_directory.len() > 4096 {
        return Err("Excel 模板或输出目录路径过长".into());
    }
    if !settings.enabled {
        return Ok(());
    }
    if settings.template_path.trim().is_empty() {
        return Err("请选择 Excel 模板".into());
    }
    require_xlsx(Path::new(settings.template_path.trim()))?;
    if settings.prompt.trim().is_empty() {
        return Err("请填写 Excel 输出提示词".into());
    }
    if settings.output_directory.trim().is_empty() {
        return Err("请选择 Excel 输出目录".into());
    }
    Ok(())
}

fn require_xlsx(path: &Path) -> Result<(), String> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    {
        return Err("Excel 模板仅支持 .xlsx 文件，请先将旧版或带宏工作簿另存为 .xlsx".into());
    }
    Ok(())
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("日期格式无效，请使用 YYYYMMDD".into());
    }
    let date = NaiveDate::parse_from_str(value, "%Y%m%d")
        .map_err(|_| "日期无效，请重新选择日历范围".to_string())?;
    if date.year() < 1 || date.format("%Y%m%d").to_string() != value {
        return Err("日期无效，请重新选择日历范围".into());
    }
    Ok(date)
}

pub fn validate_dates(dates: &[String]) -> Result<(String, String), String> {
    if dates.is_empty() {
        return Err("请先选择本周内需要输出的日期".into());
    }
    if dates.len() > 7 {
        return Err("Excel 输出只能选择同一周（周一至周日）内的日期".into());
    }
    let mut parsed = dates
        .iter()
        .map(|date| parse_date(date))
        .collect::<Result<Vec<_>, _>>()?;
    parsed.sort_unstable();
    parsed.dedup();
    if parsed
        .iter()
        .any(|date| date.iso_week() != parsed[0].iso_week())
    {
        return Err("Excel 输出只能选择同一周（周一至周日）内的日期".into());
    }
    Ok((
        parsed.first().unwrap().format("%Y%m%d").to_string(),
        parsed.last().unwrap().format("%Y%m%d").to_string(),
    ))
}

/// Check archive sizes and worksheet item counts before the workbook library allocates objects.
fn preflight(bytes: &[u8]) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "无法读取 Excel 模板，请选择有效的 .xlsx 工作簿".to_string())?;
    if archive.len() > 2048 {
        return Err("Excel 模板包含过多内部文件，请简化模板".into());
    }
    let mut unpacked = 0_u64;
    let mut sheets = 0;
    let mut cells = 0;
    let mut rows = 0;
    let mut merges = 0;
    let mut formatted_columns = 0_u32;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Excel 模板内部文件损坏".to_string())?;
        unpacked = unpacked.saturating_add(entry.size());
        if unpacked > MAX_UNPACKED_BYTES {
            return Err("Excel 模板解压后过大，请使用较小的模板".into());
        }
        let name = entry.name().to_string();
        if !(name.starts_with("xl/worksheets/") && name.ends_with(".xml")) {
            continue;
        }
        sheets += 1;
        if sheets > MAX_SHEETS {
            return Err("Excel 模板最多支持 32 个工作表".into());
        }
        let mut xml = Vec::new();
        entry
            .by_ref()
            .take(MAX_UNPACKED_BYTES + 1)
            .read_to_end(&mut xml)
            .map_err(|_| "Excel 模板内部文件损坏".to_string())?;
        if xml.len() as u64 > MAX_UNPACKED_BYTES {
            return Err("Excel 模板解压后过大，请使用较小的模板".into());
        }
        let mut reader = Reader::from_reader(xml.as_slice());
        loop {
            match reader.read_event() {
                Ok(Event::Start(element) | Event::Empty(element)) => {
                    match element.local_name().as_ref() {
                        b"c" => cells += 1,
                        b"row" => rows += 1,
                        b"mergeCell" => merges += 1,
                        b"col" => {
                            // The workbook reader expands <col min="..." max="..."> ranges.
                            let mut minimum = 0;
                            let mut maximum = 0;
                            for attribute in element.attributes() {
                                let attribute = attribute.map_err(|_| "Excel 模板列格式无效")?;
                                if matches!(attribute.key.as_ref(), b"min" | b"max") {
                                    let value = std::str::from_utf8(attribute.value.as_ref())
                                        .ok()
                                        .and_then(|value| value.parse::<u32>().ok())
                                        .ok_or("Excel 模板列格式无效")?;
                                    if attribute.key.as_ref() == b"min" {
                                        minimum = value;
                                    } else {
                                        maximum = value;
                                    }
                                }
                            }
                            if minimum == 0 || minimum > maximum || maximum > MAX_COLUMN {
                                return Err("Excel 模板列格式超出有效范围".into());
                            }
                            formatted_columns =
                                formatted_columns.saturating_add(maximum - minimum + 1);
                            if formatted_columns > MAX_TEMPLATE_CELLS as u32 {
                                return Err("Excel 模板列格式范围过多，请简化模板".into());
                            }
                        }
                        _ => {}
                    }
                    if cells > MAX_TEMPLATE_CELLS || rows > MAX_TEMPLATE_CELLS || merges > 5000 {
                        return Err("Excel 模板单元格或格式范围过多，请简化模板".into());
                    }
                }
                Ok(Event::DocType(_)) => return Err("Excel 模板包含不支持的 XML 定义".into()),
                Ok(Event::Eof) => break,
                Err(_) => return Err("Excel 模板内部工作表格式无效".into()),
                _ => {}
            }
        }
    }
    Ok(())
}

fn template_bytes(path: &Path) -> Result<Vec<u8>, String> {
    require_xlsx(path)?;
    let metadata =
        fs::metadata(path).map_err(|_| "无法读取 Excel 模板，请重新选择模板".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_TEMPLATE_BYTES {
        return Err("Excel 模板须为不超过 20 MB 的 .xlsx 文件".into());
    }
    let bytes = fs::File::open(path)
        .and_then(|file| {
            let mut bytes = Vec::new();
            file.take(MAX_TEMPLATE_BYTES + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(|_| "无法读取 Excel 模板，请重新选择模板".to_string())?;
    if bytes.len() as u64 > MAX_TEMPLATE_BYTES {
        return Err("Excel 模板须为不超过 20 MB 的 .xlsx 文件".into());
    }
    Ok(bytes)
}

pub fn template_fingerprint(path: &Path) -> Result<String, String> {
    Ok(format!("{:x}", Sha256::digest(template_bytes(path)?)))
}

pub fn read_template(path: &Path) -> Result<Template, String> {
    let bytes = template_bytes(path)?;
    let fingerprint = format!("{:x}", Sha256::digest(&bytes));
    preflight(&bytes)?;
    let workbook = catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::reader::xlsx::read_reader(Cursor::new(bytes), true)
    }))
    .map_err(|_| "Excel 模板结构无效，请另存为新的 .xlsx 文件后重试".to_string())?
    .map_err(|_| "无法解析 Excel 模板，请选择未加密的 .xlsx 工作簿".to_string())?;
    if workbook.sheet_collection().is_empty() || workbook.sheet_collection().len() > MAX_SHEETS {
        return Err("Excel 模板须包含 1 至 32 个工作表".into());
    }
    Ok(Template { workbook, fingerprint })
}

impl Template {
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn context(&self) -> Result<String, String> {
        catch_unwind(AssertUnwindSafe(|| self.build_context()))
            .map_err(|_| "无法读取 Excel 模板内容，请检查模板格式".to_string())?
    }

    fn build_context(&self) -> Result<String, String> {
        let mut sheets = Vec::new();
        let mut count = 0;
        for sheet in self.workbook.sheet_collection() {
            let mut cells = Vec::new();
            for cell in sheet.cells_sorted() {
                // Include styled empty cells so the model can see writable template fields.
                if count >= MAX_CONTEXT_CELLS {
                    break;
                }
                cells.push(json!({
                    "cell": cell.coordinate().get_coordinate(),
                    "value": cell.formatted_value().chars().take(400).collect::<String>(),
                    "formula": cell.is_formula(),
                }));
                count += 1;
            }
            sheets.push(json!({
                "sheet": sheet.name(),
                "dimension": sheet.calculate_worksheet_dimension(),
                "mergedRanges": sheet.merge_cells().iter().map(|range| range.range()).collect::<Vec<_>>(),
                "cells": cells,
            }));
        }
        let context = serde_json::to_string(&json!({
            "sheets": sheets,
            "truncated": self.workbook.sheet_collection().iter().map(|s| s.collection_to_hashmap().len()).sum::<usize>() > count,
            "instructions": "Only fill requested template fields and format explicitly requested finite areas. Keep labels, merges, untargeted styles and formulas. Requested cells may contain formulas: the app replaces those formulas with your static values in the output copy only. Use listed sheet names and A1 coordinates. A merged range can only be filled at its top-left cell. Never return formulas. Return strict JSON: {\"cells\":[{\"sheet\":\"Sheet1\",\"cell\":\"B5\",\"value\":\"text\"}],\"styles\":[{\"sheet\":\"Sheet1\",\"range\":\"A14:F20\",\"border\":\"black\",\"wrapText\":true,\"autoRowHeight\":true}]}; value may be string, number, boolean or null. cells or styles may be omitted when unused. styles supports black thin solid borders on all four sides, wrapping and row height growth; at most 32 regions and 5000 affected cells in total. Never use whole rows or columns, change merges or format beyond the requested table and its final task row.",
        }))
        .map_err(|_| "无法读取 Excel 模板内容".to_string())?;
        if context.len() > MAX_CONTEXT_BYTES {
            return Err("Excel 模板内容过多，无法交给 AI，请简化模板".into());
        }
        Ok(context)
    }

    fn validate_plan(&self, response: &str) -> Result<FillPlan, String> {
        if response.len() > MAX_RESPONSE_BYTES {
            return Err("AI 返回的 Excel 内容过大，请缩小输出范围后重试".into());
        }
        let plan: FillPlan = serde_json::from_str(response.trim())
            .map_err(|_| "AI 未返回有效的 Excel 填充 JSON，请重试或调整提示词".to_string())?;
        if (plan.cells.is_empty() && plan.styles.is_empty()) || plan.cells.len() > MAX_PLAN_CELLS {
            return Err("AI Excel 输出须包含填充值或样式，填充值最多 500 个单元格".into());
        }
        if plan.styles.len() > MAX_STYLE_REGIONS {
            return Err("Excel 格式区域最多 32 个".into());
        }
        let mut affected = plan.cells.len() as u64;
        for style in &plan.styles {
            let range = parse_range(&style.range)?;
            affected += range.area();
            if affected > MAX_AFFECTED_CELLS {
                return Err("Excel 填充和格式区域合计最多影响 5000 个单元格".into());
            }
            self.workbook.sheet_by_name(&style.sheet).map_err(|_| {
                format!("AI 格式区域的工作表不存在：{}", style.sheet.chars().take(40).collect::<String>())
            })?;
            if style.border.is_none() && style.wrap_text.is_none() && style.auto_row_height.is_none() {
                return Err("Excel 格式区域须指定边框、自动换行或自动行高".into());
            }
        }
        let mut seen = HashSet::new();
        for update in &plan.cells {
            let (column, row) = parse_coordinate(&update.cell)?;
            let sheet = self.workbook.sheet_by_name(&update.sheet).map_err(|_| {
                format!(
                    "AI 返回了模板中不存在的工作表：{}",
                    update.sheet.chars().take(40).collect::<String>()
                )
            })?;
            if !seen.insert((&update.sheet, &update.cell)) {
                return Err(format!(
                    "AI 重复填充单元格：{}!{}",
                    update.sheet, update.cell
                ));
            }
            for merged in sheet.merge_cells() {
                let range = merged.range().replace('$', "");
                let (start, end) = range.split_once(':').unwrap_or((&range, &range));
                let (left, top) = parse_coordinate(start)?;
                let (right, bottom) = parse_coordinate(end)?;
                if (left..=right).contains(&column)
                    && (top..=bottom).contains(&row)
                    && (column, row) != (left, top)
                {
                    return Err(format!(
                        "合并单元格只能填入左上角：{}!{}",
                        update.sheet, start
                    ));
                }
            }
            match &update.value {
                Value::Null | Value::Bool(_) => {}
                Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {}
                Value::String(value) => {
                    if value.encode_utf16().count() > MAX_CELL_TEXT
                        || value.chars().any(|ch| {
                            !matches!(ch, '\t' | '\n' | '\r')
                                && (ch < ' ' || ch == '\u{fffe}' || ch == '\u{ffff}')
                        })
                    {
                        return Err("AI 返回的单元格文字过长或包含无效字符".into());
                    }
                    if value.trim_start().starts_with('=') {
                        return Err("Excel 输出仅允许填充值，不能写入公式".into());
                    }
                }
                _ => return Err("AI 单元格填充值仅支持文字、数字、布尔值或 null".into()),
            }
        }
        Ok(plan)
    }
}

fn parse_coordinate(value: &str) -> Result<(u32, u32), String> {
    let invalid = || "AI 返回了无效的 Excel 单元格坐标".to_string();
    if value.len() < 2 || value.len() > 10 {
        return Err(invalid());
    }
    let letters = value.bytes().take_while(u8::is_ascii_uppercase).count();
    if letters == 0 || letters > 3 || letters == value.len() {
        return Err(invalid());
    }
    let digits = &value[letters..];
    if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    let column = value[..letters].bytes().fold(0_u32, |result, byte| {
        result * 26 + u32::from(byte - b'A' + 1)
    });
    let row = digits.parse::<u32>().map_err(|_| invalid())?;
    if column > MAX_COLUMN || row == 0 || row > MAX_ROW {
        return Err(invalid());
    }
    Ok((column, row))
}

fn parse_range(value: &str) -> Result<CellRange, String> {
    let (start, end) = value.split_once(':').unwrap_or((value, value));
    let (left, top) = parse_coordinate(start)?;
    let (right, bottom) = parse_coordinate(end)?;
    if left > right || top > bottom {
        return Err("Excel 格式区域须为从左上至右下的有限 A1 范围".into());
    }
    Ok(CellRange { left, top, right, bottom })
}

fn black_thin_border(border: &mut Border) {
    let mut color = Color::default();
    color.set_argb([255_u8, 0, 0, 0]);
    border.set_border_style(Border::BORDER_THIN);
    border.set_color(color);
}

fn preserve_default_column_width(sheet: &mut Worksheet, column: u32) {
    // cell_mut creates a Column with the library's own default width. Preserve
    // a template's sheet-wide width before that creation makes it explicit.
    if sheet.column_dimension_by_number(column).is_none() {
        let width = sheet.sheet_format_properties().default_column_width();
        if width.is_finite() && width > 0.0 {
            sheet.column_dimension_by_number_mut(column).set_width(width);
        }
    }
}

fn apply_styles(
    workbook: &mut Workbook,
    styles: &[FillStyle],
    cancel: Option<&CancellationToken>,
) -> Result<(), String> {
    let mut fit_cells = BTreeSet::new();
    for update in styles {
        let range = parse_range(&update.range)?;
        let sheet = workbook
            .sheet_by_name_mut(&update.sheet)
            .map_err(|_| "Excel 格式区域的工作表不存在".to_string())?;
        for row in range.top..=range.bottom {
            check_cancelled(cancel)?;
            for column in range.left..=range.right {
                preserve_default_column_width(sheet, column);
                let style = sheet.cell_mut((column, row)).style_mut();
                if update.border.is_some() {
                    let borders = style.borders_mut();
                    black_thin_border(borders.top_mut());
                    black_thin_border(borders.bottom_mut());
                    black_thin_border(borders.left_mut());
                    black_thin_border(borders.right_mut());
                }
                if let Some(wrap) = update.wrap_text {
                    style.alignment_mut().set_wrap_text(wrap);
                }
                if update.auto_row_height == Some(true) {
                    fit_cells.insert((update.sheet.clone(), row, column));
                }
            }
        }
    }
    // Calculate after all style operations, using the final wrap settings and
    // final filled text. Grow each row to the maximum needed by its cells.
    for (name, row, column) in fit_cells {
        check_cancelled(cancel)?;
        let sheet = workbook
            .sheet_by_name_mut(&name)
            .map_err(|_| "Excel 格式区域的工作表不存在".to_string())?;
        if let Some(height) = estimated_row_height(sheet, column, row)? {
            if height > row_height(sheet, row) {
                sheet.row_dimension_mut(row).set_height(height);
            }
        }
    }
    Ok(())
}

fn row_height(sheet: &Worksheet, row: u32) -> f64 {
    let default = sheet.sheet_format_properties().default_row_height();
    let default = if default.is_finite() && default > 0.0 { default } else { 15.0 };
    sheet
        .row_dimension(row)
        .map(|dimension| dimension.height())
        .filter(|height| height.is_finite() && *height > 0.0)
        .unwrap_or(default)
}

fn estimated_row_height(sheet: &Worksheet, column: u32, row: u32) -> Result<Option<f64>, String> {
    let Some(cell) = sheet.cell((column, row)) else { return Ok(None) };
    let text = cell.formatted_value();
    if text.is_empty() {
        return Ok(None);
    }
    let mut merged = CellRange { left: column, right: column, top: row, bottom: row };
    for range in sheet.merge_cells() {
        let candidate = parse_range(&range.range().replace('$', ""))?;
        if candidate.contains(column, row) {
            if (column, row) != (candidate.left, candidate.top) {
                return Ok(None);
            }
            merged = candidate;
            break;
        }
    }
    if merged.area() > MAX_AFFECTED_CELLS {
        return Err("Excel 自动行高的合并区域最多支持 5000 个单元格，请缩小模板合并范围".into());
    }
    // Excel column widths are roughly digit widths; convert to points with
    // padding. Merged text uses the combined width without changing the merge.
    let default_width = sheet.sheet_format_properties().default_column_width();
    let default_width = if default_width.is_finite() && default_width > 0.0 {
        default_width
    } else {
        8.43
    };
    let width_points = (merged.left..=merged.right)
        .map(|index| {
            let width = sheet
                .column_dimension_by_number(index)
                .map(|dimension| dimension.width())
                .filter(|width| width.is_finite() && *width > 0.0)
                .unwrap_or(default_width);
            width * 5.25 + 3.75
        })
        .sum::<f64>();
    let font_size = cell
        .style()
        .font()
        .map(|font| font.size())
        .filter(|size| size.is_finite() && *size > 0.0)
        .unwrap_or(11.0);
    let wrap = cell.style().alignment().is_some_and(|alignment| alignment.wrap_text());
    let lines = estimated_text_lines(&text, (width_points - 6.0).max(1.0), font_size, wrap);
    let total_height = f64::from(lines) * font_size * 1.35 + 6.0;
    let other_rows = (merged.top + 1..=merged.bottom)
        .map(|index| row_height(sheet, index))
        .sum::<f64>();
    let height = (total_height - other_rows).max(row_height(sheet, row));
    if height > MAX_ROW_HEIGHT {
        return Err(format!(
            "{} 的第 {row} 行文字超出 Excel 自动行高上限，请缩短文字或扩大模板列宽后重试",
            sheet.name()
        ));
    }
    Ok(Some(height))
}

fn estimated_text_lines(text: &str, available_width: f64, font_size: f64, wrap: bool) -> u32 {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split('\n')
        .map(|line| {
            let mut lines = 1;
            let mut used = 0.0;
            for character in line.chars() {
                let width = character_width(character) * font_size;
                if wrap && used > 0.0 && used + width > available_width {
                    lines += 1;
                    used = 0.0;
                }
                used += width;
            }
            lines
        })
        .sum()
}

fn character_width(character: char) -> f64 {
    match character {
        '\t' => 2.2,
        ' ' => 0.33,
        '\u{0300}'..='\u{036f}' | '\u{200b}'..='\u{200f}' | '\u{fe00}'..='\u{fe0f}' => 0.0,
        '\u{1100}'..='\u{115f}' | '\u{2e80}'..='\u{a4cf}' | '\u{ac00}'..='\u{d7a3}'
        | '\u{f900}'..='\u{faff}' | '\u{fe10}'..='\u{fe6f}' | '\u{ff00}'..='\u{ff60}'
        | '\u{ffe0}'..='\u{ffe6}' | '\u{1f000}'..='\u{1faff}' | '\u{20000}'..='\u{3ffff}' => 1.0,
        character if character.is_ascii() => 0.58,
        _ => 0.65,
    }
}

#[cfg(test)]
pub fn fill_and_save(
    template: Template,
    response: &str,
    output_directory: &Path,
    start: &str,
    end: &str,
) -> Result<ExportResult, String> {
    fill_and_save_impl(template, response, output_directory, start, end, None)
}

#[cfg(test)]
pub fn fill_and_save_cancellable(
    template: Template,
    response: &str,
    output_directory: &Path,
    start: &str,
    end: &str,
    cancel: &CancellationToken,
) -> Result<ExportResult, String> {
    fill_and_save_impl(
        template,
        response,
        output_directory,
        start,
        end,
        Some(cancel),
    )
}

fn check_cancelled(cancel: Option<&CancellationToken>) -> Result<(), String> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err("Excel 输出已停止".into());
    }
    Ok(())
}

#[cfg(test)]
fn fill_and_save_impl(
    template: Template,
    response: &str,
    output_directory: &Path,
    start: &str,
    end: &str,
    cancel: Option<&CancellationToken>,
) -> Result<ExportResult, String> {
    validate_dates(&[start.to_string(), end.to_string()])?;
    let (bytes, cells, _, _) = render_workbook(template, response, cancel)?;
    publish_bytes(&bytes, output_directory, start, end, cells.len(), cancel)
}

pub fn prepare(
    template: Template,
    response: &str,
    provenance: Provenance,
    start: String,
    end: String,
    cancel: CancellationToken,
) -> Result<PendingPreview, String> {
    check_cancelled(Some(&cancel))?;
    let (validated_start, validated_end) = validate_dates(&provenance.dates)?;
    if start != validated_start || end != validated_end || template.fingerprint() != provenance.template_fingerprint {
        return Err("Excel 预览来源不一致，请重新生成预览".into());
    }
    let directory = Path::new(provenance.settings.output_directory.trim());
    if !directory.is_dir() {
        return Err("Excel 输出目录不存在，请重新选择输出目录".into());
    }
    let output_path = fs::canonicalize(directory).map_err(|_| "无法读取 Excel 输出目录")?;
    let (bytes, cells, styles, sheets) = render_workbook(template, response, Some(&cancel))?;
    let template_name = Path::new(provenance.settings.template_path.trim())
        .file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let preview = ExcelPreview {
        id: uuid::Uuid::new_v4().to_string(),
        template_name,
        output_directory: provenance.settings.output_directory.trim().to_string(),
        start,
        end,
        cells,
        styles,
        sheets,
    };
    Ok(PendingPreview { preview, bytes, provenance, output_path, created: Instant::now(), cancel })
}

fn render_workbook(
    mut template: Template,
    response: &str,
    cancel: Option<&CancellationToken>,
) -> Result<(Vec<u8>, Vec<ExcelPreviewCell>, Vec<ExcelPreviewStyle>, Vec<ExcelSheetPreview>), String> {
    check_cancelled(cancel)?;
    let plan = template.validate_plan(response)?;
    let cells = plan.cells.iter().map(|update| {
        let sheet = template.workbook.sheet_by_name(&update.sheet).map_err(|_| "Excel 模板工作表不存在")?;
        let previous = sheet.cell(update.cell.as_str());
        let replaces_formula = previous.is_some_and(|cell| cell.is_formula());
        let previous_value = previous.map(|cell| {
            if cell.is_formula() { format!("={}", cell.formula()) }
            else { cell.formatted_value().to_string() }
        }).unwrap_or_default();
        let previous_value = if previous_value.chars().count() > 2000 {
            format!("{}…", previous_value.chars().take(2000).collect::<String>())
        } else { previous_value };
        Ok(ExcelPreviewCell { sheet: update.sheet.clone(), cell: update.cell.clone(), value: update.value.clone(), previous_value, replaces_formula })
    }).collect::<Result<Vec<_>, String>>()?;
    let styles = plan.styles.iter().map(|style| ExcelPreviewStyle {
        sheet: style.sheet.clone(), range: style.range.clone(),
        border: style.border.as_ref().map(|_| "black"),
        wrap_text: style.wrap_text, auto_row_height: style.auto_row_height,
    }).collect();
    let mut replaced_formulas = Vec::new();
    for update in &plan.cells {
        let coordinate = parse_coordinate(&update.cell)?;
        let sheet = template
            .workbook
            .sheet_by_name_mut(&update.sheet)
            .map_err(|_| "Excel 模板工作表不存在".to_string())?;
        preserve_default_column_width(sheet, coordinate.0);
        let cell = sheet.cell_mut(coordinate);
        let replacing_formula = cell.is_formula();
        let written = catch_unwind(AssertUnwindSafe(|| {
            // Scalar setters can retain an existing formula. Replace only its
            // CellValue first; the cell's style and the source file stay intact.
            if replacing_formula {
                cell.set_cell_value(CellValue::default());
            }
            match &update.value {
                Value::String(value) => {
                    cell.set_value_string(value);
                }
                Value::Number(value) => {
                    cell.set_value_number(value.as_f64().unwrap());
                }
                Value::Bool(value) => {
                    cell.set_value_bool(*value);
                }
                Value::Null => {
                    cell.set_cell_value(CellValue::default());
                }
                _ => unreachable!("validated plan contains only scalar values"),
            }
            !cell.is_formula()
        }));
        if !matches!(written, Ok(true)) {
            let location = format!("{}!{}", update.sheet, update.cell);
            return Err(if replacing_formula {
                formula_write_error("无法写入 Excel 单元格", &[location])
            } else {
                format!("无法写入 Excel 单元格：{location}")
            });
        }
        if replacing_formula {
            replaced_formulas.push(format!("{}!{}", update.sheet, update.cell));
        }
    }
    catch_unwind(AssertUnwindSafe(|| apply_styles(&mut template.workbook, &plan.styles, cancel)))
        .map_err(|_| "Excel 样式设置失败，请检查模板格式或缩小格式区域".to_string())??;
    let sheets = catch_unwind(AssertUnwindSafe(|| preview::extract(&template.workbook, &plan, cancel)))
        .map_err(|_| "无法生成 Excel 表格预览，请检查模板格式".to_string())??;
    // Rendering and serialization happen entirely in memory before the user
    // sees the preview. Confirming later publishes these exact approved bytes.
    check_cancelled(cancel)?;
    let mut buffer = Cursor::new(Vec::new());
    catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::writer::xlsx::write_writer(&template.workbook, &mut buffer)
    }))
    .map_err(|_| formula_write_error("Excel 模板写入失败，请检查模板格式", &replaced_formulas))?
    .map_err(|_| formula_write_error("无法生成 Excel 预览，请检查模板格式", &replaced_formulas))?;
    check_cancelled(cancel)?;
    let bytes = buffer.into_inner();
    if bytes.len() > MAX_PENDING_BYTES {
        return Err("Excel 预览文件过大，请简化模板后重试".into());
    }
    Ok((bytes, cells, styles, sheets))
}

fn publish_bytes(
    bytes: &[u8],
    output_directory: &Path,
    start: &str,
    end: &str,
    cells: usize,
    cancel: Option<&CancellationToken>,
) -> Result<ExportResult, String> {
    check_cancelled(cancel)?;
    if !output_directory.is_dir() {
        return Err("Excel 输出目录不存在，请重新选择输出目录".into());
    }
    // A complete temporary workbook is created only after confirmation, then
    // published atomically without replacing an existing output or template.
    let mut file = tempfile::Builder::new()
        .prefix(".workreport-")
        .tempfile_in(output_directory)
        .map_err(|_| "Excel 输出目录不可写".to_string())?;
    file.write_all(bytes)
        .map_err(|_| "无法写入 Excel 文件，请检查输出目录和磁盘空间".to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|_| "无法完成 Excel 文件写入".to_string())?;
    check_cancelled(cancel)?;
    let suffix = file
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(".workreport-"))
        .ok_or("无法创建 Excel 文件名")?;
    let filename = format!("周报_{start}-{end}_{suffix}.xlsx");
    let path = output_directory.join(&filename);
    file.persist_noclobber(&path)
        .map_err(|_| "无法保存 Excel 文件，已有文件不会被覆盖，请重试".to_string())?;
    Ok(ExportResult {
        path: path.to_string_lossy().into_owned(),
        filename,
        cells,
    })
}

fn formula_write_error(reason: &str, cells: &[String]) -> String {
    if cells.is_empty() {
        return reason.to_string();
    }
    let locations = cells
        .iter()
        .take(6)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    let more = if cells.len() > 6 { "等" } else { "" };
    format!("{reason}：{locations}{more}。请在Excel模板中清除对应单元格的公式后重试")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_file(directory: &Path) -> std::path::PathBuf {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_string("本周工作");
        sheet.cell_mut("B2").set_value_string("待填写");
        sheet.cell_mut("B2").style_mut().font_mut().set_bold(true);
        sheet.cell_mut("C3").set_formula("SUM(D3:D4)");
        sheet.cell_mut("A5").set_value_string("合并标题");
        sheet.add_merge_cells("A5:C5");
        sheet.row_dimension_mut(2).set_height(36.0);
        let path = directory.join("template.xlsx");
        umya_spreadsheet::writer::xlsx::write(&workbook, &path).unwrap();
        path
    }

    #[test]
    fn dates_accept_any_subset_of_a_monday_to_sunday_week() {
        assert_eq!(
            validate_dates(&["20261025".into(), "20261019".into(), "20261021".into()]).unwrap(),
            ("20261019".into(), "20261025".into())
        );
        assert!(validate_dates(&["20261021".into()]).is_ok());
        assert!(validate_dates(&["20261026".into(), "20261025".into()]).is_err());
        assert!(validate_dates(&["20261019".into(), "20261026".into()]).is_err());
    }

    #[test]
    fn dates_support_cross_month_and_cross_year_iso_weeks() {
        assert!(validate_dates(&["20261228".into(), "20270103".into()]).is_ok());
        assert!(validate_dates(&["20261231".into(), "20270104".into()]).is_err());
        assert!(validate_dates(&["20260928".into(), "20261004".into()]).is_ok());
    }

    #[test]
    fn dates_reject_invalid_format_and_empty_selection() {
        assert!(validate_dates(&[]).is_err());
        for invalid in [
            "2026-10-21",
            "2026102",
            "20260229",
            "20261301",
            "00001021",
            "20261021x",
        ] {
            assert!(validate_dates(&[invalid.into()]).is_err(), "{invalid}");
        }
    }

    #[test]
    fn settings_allow_disabled_drafts_but_require_enabled_fields_and_xlsx() {
        let mut settings = Settings::default();
        assert!(validate_settings(&settings).is_ok());
        settings.enabled = true;
        assert!(validate_settings(&settings).is_err());
        settings.template_path = "template.xlsx".into();
        settings.prompt = "按照日报填入 B2".into();
        settings.output_directory = "/output".into();
        assert!(validate_settings(&settings).is_ok());
        settings.template_path = "template.xlsm".into();
        assert!(validate_settings(&settings).unwrap_err().contains(".xlsx"));
        settings.template_path = "template.xlsx".into();
        settings.prompt = "字".repeat(6001);
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn fill_plan_rejects_new_formulas_merge_children_and_malformed_values() {
        let temporary = tempfile::tempdir().unwrap();
        let template = read_template(&template_file(temporary.path())).unwrap();
        for update in [
            json!({"sheet":"Unknown", "cell":"B2", "value":"text"}),
            json!({"sheet":"Sheet1", "cell":"B5", "value":"text"}),
            json!({"sheet":"Sheet1", "cell":"B2", "value":" =HYPERLINK(\"url\")"}),
            json!({"sheet":"Sheet1", "cell":"B2", "value":{"text":"text"}}),
            json!({"sheet":"Sheet1", "cell":"B2", "value":"\u{0000}"}),
        ] {
            assert!(template
                .validate_plan(&json!({"cells":[update]}).to_string())
                .is_err());
        }
        for coordinate in [
            "a1", "$A$1", "A0", "A01", "XFE1", "A1048577", "A1:B2", "1A", "A1 ",
        ] {
            assert!(parse_coordinate(coordinate).is_err(), "{coordinate}");
        }
        assert!(parse_coordinate("XFD1048576").is_ok());
        assert!(template
            .validate_plan("```json\n{\"cells\":[]}\n```")
            .is_err());
        assert!(template
            .validate_plan(
                &json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"ok","formula":"=1"}]})
                    .to_string()
            )
            .is_err());
        assert!(template.validate_plan(&json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"ok"},{"sheet":"Sheet1","cell":"B2","value":"again"}]}).to_string()).is_err());
    }

    #[test]
    fn filled_workbook_retains_styles_formulas_merges_and_template_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let original_path = template_file(temporary.path());
        let original_bytes = fs::read(&original_path).unwrap();
        let original = read_template(&original_path).unwrap();
        let original_sheet = original.workbook.sheet_by_name("Sheet1").unwrap();
        let style = original_sheet.cell("B2").unwrap().style().clone();
        let response = json!({"cells":[
            {"sheet":"Sheet1","cell":"B2","value":"完成接口联调"},
            {"sheet":"Sheet1","cell":"A5","value":"本周总结"},
            {"sheet":"Sheet1","cell":"D3","value":12.5},
            {"sheet":"Sheet1","cell":"D4","value":true},
            {"sheet":"Sheet1","cell":"A1","value":null}
        ]})
        .to_string();
        let result = fill_and_save(
            original,
            &response,
            temporary.path(),
            "20261019",
            "20261025",
        )
        .unwrap();
        assert_eq!(result.cells, 5);
        let output = read_template(Path::new(&result.path)).unwrap();
        let sheet = output.workbook.sheet_by_name("Sheet1").unwrap();
        assert_eq!(sheet.cell("B2").unwrap().value(), "完成接口联调");
        assert_eq!(sheet.cell("B2").unwrap().style(), &style);
        assert_eq!(sheet.cell("C3").unwrap().formula(), "SUM(D3:D4)");
        assert_eq!(sheet.merge_cells()[0].range(), "A5:C5");
        assert_eq!(sheet.row_dimension(2).unwrap().height(), 36.0);
        assert_eq!(sheet.cell("D3").unwrap().value_number(), Some(12.5));
        assert_eq!(
            sheet
                .cell("A1")
                .map(|cell| cell.value().to_string())
                .unwrap_or_default(),
            ""
        );
        assert_eq!(fs::read(&original_path).unwrap(), original_bytes);
        let again = fill_and_save(
            read_template(&original_path).unwrap(),
            &response,
            temporary.path(),
            "20261019",
            "20261025",
        )
        .unwrap();
        assert_ne!(result.path, again.path);
        assert!(Path::new(&result.path).is_file());
    }

    #[test]
    fn selected_formula_cells_become_static_values_only_in_the_output_copy() {
        let temporary = tempfile::tempdir().unwrap();
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        for coordinate in ["A1", "B1", "C1", "D1", "E1"] {
            sheet.cell_mut(coordinate).set_formula("1+1");
            sheet
                .cell_mut(coordinate)
                .style_mut()
                .font_mut()
                .set_bold(true);
        }
        let original_path = temporary.path().join("formula-template.xlsx");
        umya_spreadsheet::writer::xlsx::write(&workbook, &original_path).unwrap();
        let original_bytes = fs::read(&original_path).unwrap();
        let template = read_template(&original_path).unwrap();
        let style = template
            .workbook
            .sheet_by_name("Sheet1")
            .unwrap()
            .cell("A1")
            .unwrap()
            .style()
            .clone();
        let response = json!({"cells":[
            {"sheet":"Sheet1","cell":"A1","value":"完成测试"},
            {"sheet":"Sheet1","cell":"B1","value":12.5},
            {"sheet":"Sheet1","cell":"C1","value":true},
            {"sheet":"Sheet1","cell":"D1","value":null}
        ]})
        .to_string();
        let result = fill_and_save(
            template,
            &response,
            temporary.path(),
            "20261019",
            "20261025",
        )
        .unwrap();
        let output = read_template(Path::new(&result.path)).unwrap();
        let sheet = output.workbook.sheet_by_name("Sheet1").unwrap();
        for coordinate in ["A1", "B1", "C1", "D1"] {
            let cell = sheet.cell(coordinate).unwrap();
            assert!(!cell.is_formula(), "{coordinate}");
            assert_eq!(cell.formula(), "", "{coordinate}");
            assert_eq!(cell.style(), &style, "{coordinate}");
        }
        assert_eq!(sheet.cell("A1").unwrap().value(), "完成测试");
        assert_eq!(sheet.cell("B1").unwrap().value_number(), Some(12.5));
        assert_eq!(sheet.cell("C1").unwrap().value(), "TRUE");
        assert_eq!(sheet.cell("D1").unwrap().value(), "");
        assert_eq!(sheet.cell("E1").unwrap().formula(), "1+1");
        assert_eq!(sheet.cell("E1").unwrap().style(), &style);
        assert_eq!(fs::read(&original_path).unwrap(), original_bytes);
        let original = read_template(&original_path).unwrap();
        for coordinate in ["A1", "B1", "C1", "D1", "E1"] {
            assert_eq!(
                original
                    .workbook
                    .sheet_by_name("Sheet1")
                    .unwrap()
                    .cell(coordinate)
                    .unwrap()
                    .formula(),
                "1+1"
            );
        }
    }

    #[test]
    fn formula_write_failures_identify_cells_and_explain_template_recovery() {
        assert_eq!(formula_write_error("写入失败", &[]), "写入失败");
        let error = formula_write_error("无法写入 Excel 单元格", &["周报!B5".into()]);
        assert!(error.contains("周报!B5"));
        assert!(error.contains("请在Excel模板中清除对应单元格的公式后重试"));
    }

    #[test]
    fn context_describes_cells_merges_and_static_formula_replacement_without_local_path() {
        let temporary = tempfile::tempdir().unwrap();
        let path = template_file(temporary.path());
        let context = read_template(&path).unwrap().context().unwrap();
        assert!(context.contains("本周工作"));
        assert!(context.contains("A5:C5"));
        assert!(context.contains("\"formula\":true"));
        assert!(context.contains("replaces those formulas with your static values"));
        assert!(!context.contains(&temporary.path().to_string_lossy().to_string()));
    }

    #[test]
    fn requested_styles_export_black_borders_wrapping_and_content_fitting_row_heights() {
        let temporary = tempfile::tempdir().unwrap();
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        for column in 1..=6 {
            sheet.column_dimension_by_number_mut(column).set_width(12.0);
        }
        sheet.add_merge_cells("A14:C14");
        sheet.cell_mut("A16").set_value_string("保留较高行高");
        sheet.cell_mut("G14").set_value_string("区域外内容");
        sheet.cell_mut("G14").style_mut().font_mut().set_bold(true);
        for coordinate in ["A14", "A15"] {
            sheet.cell_mut(coordinate).style_mut().font_mut().set_size(12.0);
        }
        sheet.row_dimension_mut(13).set_height(32.0);
        sheet.row_dimension_mut(14).set_height(14.0);
        sheet.row_dimension_mut(15).set_height(14.0);
        sheet.row_dimension_mut(16).set_height(120.0);
        let path = temporary.path().join("style-template.xlsx");
        umya_spreadsheet::writer::xlsx::write(&workbook, &path).unwrap();
        let bytes = fs::read(&path).unwrap();
        let template = read_template(&path).unwrap();
        let outside_style = template.workbook.sheet_by_name("Sheet1").unwrap().cell("G14").unwrap().style().clone();
        let text = "完成接口改造，包含中文说明与 English details，保留全部记录。\n下一步继续联调。";
        let plan = json!({
            "cells":[
                {"sheet":"Sheet1","cell":"A14","value":text},
                {"sheet":"Sheet1","cell":"A15","value":text}
            ],
            "styles":[{"sheet":"Sheet1","range":"A14:F16","border":"black","wrapText":true,"autoRowHeight":true}]
        });
        let result = fill_and_save(template, &plan.to_string(), temporary.path(), "20261019", "20261025").unwrap();
        let output = read_template(Path::new(&result.path)).unwrap();
        let sheet = output.workbook.sheet_by_name("Sheet1").unwrap();
        for row in 14..=16 {
            for column in 1..=6 {
                let style = sheet.cell((column, row)).unwrap().style();
                assert!(style.alignment().unwrap().wrap_text());
                let borders = style.borders().unwrap();
                for border in [borders.top(), borders.bottom(), borders.left(), borders.right()] {
                    assert_eq!(border.border_style(), Border::BORDER_THIN);
                    assert_eq!(border.color().unwrap().argb(), [255_u8, 0, 0, 0].into());
                }
            }
        }
        assert!(sheet.row_dimension(14).unwrap().height() > 14.0);
        assert!(sheet.row_dimension(15).unwrap().height() > sheet.row_dimension(14).unwrap().height());
        assert_eq!(sheet.row_dimension(16).unwrap().height(), 120.0);
        assert_eq!(sheet.row_dimension(13).unwrap().height(), 32.0);
        assert_eq!(sheet.cell("G14").unwrap().style(), &outside_style);
        assert_eq!(sheet.cell("G14").unwrap().value(), "区域外内容");
        assert_eq!(sheet.merge_cells()[0].range(), "A14:C14");
        assert_eq!(sheet.cell("A14").unwrap().value(), text);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn styles_only_can_change_wrapping_without_losing_other_cell_styles_or_values() {
        let temporary = tempfile::tempdir().unwrap();
        let path = template_file(temporary.path());
        let original_bytes = fs::read(&path).unwrap();
        let original = read_template(&path).unwrap();
        let original_font = original.workbook.sheet_by_name("Sheet1").unwrap().cell("B2").unwrap().style().font().cloned();
        let result = fill_and_save(
            original,
            &json!({"styles":[{"sheet":"Sheet1","range":"B2","wrapText":true}]}).to_string(),
            temporary.path(), "20261019", "20261025"
        ).unwrap();
        assert_eq!(result.cells, 0);
        let output = read_template(Path::new(&result.path)).unwrap();
        let sheet = output.workbook.sheet_by_name("Sheet1").unwrap();
        assert_eq!(sheet.cell("B2").unwrap().value(), "待填写");
        assert_eq!(sheet.cell("B2").unwrap().style().font(), original_font.as_ref());
        assert!(sheet.cell("B2").unwrap().style().alignment().unwrap().wrap_text());
        assert_eq!(sheet.row_dimension(2).unwrap().height(), 36.0);
        assert_eq!(sheet.cell("C3").unwrap().formula(), "SUM(D3:D4)");
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
    }

    #[test]
    fn invalid_and_excessive_style_plans_do_not_create_output() {
        let temporary = tempfile::tempdir().unwrap();
        let path = template_file(temporary.path());
        let count = fs::read_dir(temporary.path()).unwrap().count();
        for range in ["A:A", "14:20", "A0:B1", "B2:A1", "A1:B2:C3", "A1:XFD1048576", "A1:A5001", "A1 ", "$A$1:B2"] {
            let plan = json!({"styles":[{"sheet":"Sheet1","range":range,"border":"black"}]});
            assert!(fill_and_save(read_template(&path).unwrap(), &plan.to_string(), temporary.path(), "20261019", "20261025").is_err(), "{range}");
        }
        let template = read_template(&path).unwrap();
        for style in [
            json!({"sheet":"Unknown","range":"A1","border":"black"}),
            json!({"sheet":"Sheet1","range":"A1","border":"red"}),
            json!({"sheet":"Sheet1","range":"A1","wrapText":"yes"}),
            json!({"sheet":"Sheet1","range":"A1","fontSize":12}),
            json!({"sheet":"Sheet1","range":"A1"})
        ] {
            assert!(template.validate_plan(&json!({"styles":[style]}).to_string()).is_err());
        }
        let many = json!({"styles":(0..33).map(|_|json!({"sheet":"Sheet1","range":"B2","border":"black"})).collect::<Vec<_>>()});
        assert!(template.validate_plan(&many.to_string()).unwrap_err().contains("32"));
        let combined = json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"test"}],"styles":[{"sheet":"Sheet1","range":"A1:A5000","border":"black"}]});
        assert!(template.validate_plan(&combined.to_string()).unwrap_err().contains("5000"));
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), count);
    }

    #[test]
    fn text_height_estimation_accounts_for_unicode_newlines_font_size_and_wrapping() {
        assert_eq!(estimated_text_lines("ab\r\ncd\ref", 100.0, 11.0, true), 3);
        assert!(estimated_text_lines("中文中文", 30.0, 11.0, true) > estimated_text_lines("abcd", 30.0, 11.0, true));
        assert!(estimated_text_lines("abcdabcd", 30.0, 22.0, true) > estimated_text_lines("abcdabcd", 30.0, 11.0, true));
        assert_eq!(estimated_text_lines("中文中文", 2.0, 11.0, false), 1);
    }

    #[test]
    fn automatic_height_uses_sheet_defaults_and_never_reduces_the_default_row_height() {
        let temporary = tempfile::tempdir().unwrap();
        let mut heights = Vec::new();
        for width in [4.0, 20.0] {
            let mut workbook = umya_spreadsheet::new_file();
            let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
            sheet.sheet_format_properties_mut().set_default_row_height(30.0);
            sheet.sheet_format_properties_mut().set_default_column_width(width);
            sheet.cell_mut("A1").set_value_string("short");
            sheet.cell_mut("A2").set_value_string("abcdabcdabcdabcdabcdabcdabcdabcdabcdabcd");
            sheet.column_dimensions_mut().clear();
            assert!(!sheet.row_dimension(1).unwrap().custom_height());
            let path = temporary.path().join(format!("default-width-{width}.xlsx"));
            umya_spreadsheet::writer::xlsx::write(&workbook, &path).unwrap();
            let original_bytes = fs::read(&path).unwrap();
            let template = read_template(&path).unwrap();
            assert_eq!(row_height(template.workbook.sheet_by_name("Sheet1").unwrap(), 1), 30.0);
            assert!(template.workbook.sheet_by_name("Sheet1").unwrap().column_dimension_by_number(1).is_none());
            let result = fill_and_save(
                template,
                &json!({"cells":[{"sheet":"Sheet1","cell":"A3","value":"写值路径"}],"styles":[{"sheet":"Sheet1","range":"A1:A3","wrapText":true,"autoRowHeight":true}]}).to_string(),
                temporary.path(), "20261019", "20261025"
            ).unwrap();
            let output = read_template(Path::new(&result.path)).unwrap();
            let sheet = output.workbook.sheet_by_name("Sheet1").unwrap();
            assert_eq!(sheet.sheet_format_properties().default_row_height(), 30.0);
            assert_eq!(sheet.sheet_format_properties().default_column_width(), width);
            assert_eq!(sheet.cell("A3").unwrap().value(), "写值路径");
            assert!(row_height(sheet, 1) >= 30.0);
            heights.push(row_height(sheet, 2));
            assert_eq!(fs::read(&path).unwrap(), original_bytes);
        }
        assert!(heights[0] > heights[1], "narrow default columns need taller rows");
        assert!(heights[1] >= 30.0);
    }

    #[test]
    fn invalid_template_and_invalid_plan_do_not_create_output() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("invalid.xlsx");
        fs::write(&path, "invalid archive").unwrap();
        assert!(read_template(&path).is_err());
        let original_path = template_file(temporary.path());
        let count = fs::read_dir(temporary.path()).unwrap().count();
        assert!(fill_and_save(
            read_template(&original_path).unwrap(),
            "{\"cells\":[]}",
            temporary.path(),
            "20261019",
            "20261025"
        )
        .is_err());
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), count);
    }

    #[test]
    fn preflight_rejects_extreme_column_expansion_and_excessive_cell_objects() {
        use std::io::Write;
        fn archive(xml: &str) -> Vec<u8> {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file(
                    "xl/worksheets/sheet1.xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(xml.as_bytes()).unwrap();
            writer.finish().unwrap().into_inner()
        }
        assert!(preflight(&archive(
            "<worksheet><cols><col min=\"1\" max=\"4294967295\"/></cols></worksheet>"
        ))
        .is_err());
        assert!(preflight(&archive(&format!(
            "<worksheet><sheetData>{}</sheetData></worksheet>",
            "<c r=\"A1\"/>".repeat(MAX_TEMPLATE_CELLS + 1)
        )))
        .is_err());
        assert!(preflight(&archive("<worksheet><cols><col min=\"1\" max=\"7\"/></cols><sheetData><row r=\"1\"><c r=\"A1\"/></row></sheetData></worksheet>")).is_ok());
    }

    #[test]
    fn plan_resource_limits_match_excel_text_limits() {
        let temporary = tempfile::tempdir().unwrap();
        let template = read_template(&template_file(temporary.path())).unwrap();
        let response = |value: String| {
            json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":value}]}).to_string()
        };
        assert!(template
            .validate_plan(&response("a".repeat(MAX_CELL_TEXT)))
            .is_ok());
        assert!(template
            .validate_plan(&response("😀".repeat(MAX_CELL_TEXT / 2 + 1)))
            .is_err());
        let too_many = json!({"cells":(0..=MAX_PLAN_CELLS).map(|index| json!({"sheet":"Sheet1","cell":format!("F{}", index + 1),"value":"text"})).collect::<Vec<_>>()});
        assert!(template.validate_plan(&too_many.to_string()).is_err());
        assert!(template
            .validate_plan(&" ".repeat(MAX_RESPONSE_BYTES + 1))
            .is_err());
    }

    #[test]
    fn cancelled_export_does_not_create_any_output_or_change_template() {
        let temporary = tempfile::tempdir().unwrap();
        let path = template_file(temporary.path());
        let original = fs::read(&path).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = fill_and_save_cancellable(
            read_template(&path).unwrap(),
            &json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"取消内容"}]}).to_string(),
            temporary.path(),
            "20261019",
            "20261025",
            &cancel,
        )
        .unwrap_err();
        assert_eq!(error, "Excel 输出已停止");
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    fn pending_preview(template_path: &Path, output: &Path, cancel: CancellationToken) -> PendingPreview {
        let template = read_template(template_path).unwrap();
        let provenance = Provenance {
            workspace_path: template_path.parent().unwrap().to_string_lossy().into_owned(),
            dates: vec!["20261019".into(), "20261025".into()],
            settings: Settings {
                enabled: true,
                template_path: template_path.to_string_lossy().into_owned(),
                prompt: "写入内容".into(),
                output_directory: output.to_string_lossy().into_owned(),
            },
            source_revision: "source-hash".into(),
            template_fingerprint: template.fingerprint().into(),
        };
        prepare(template, &json!({
            "cells":[
                {"sheet":"Sheet1","cell":"B2","value":"预览确认的内容"},
                {"sheet":"Sheet1","cell":"C3","value":12.5},
                {"sheet":"Sheet1","cell":"D3","value":true},
                {"sheet":"Sheet1","cell":"A1","value":null}
            ],
            "styles":[{"sheet":"Sheet1","range":"B2:C3","border":"black","wrapText":true,"autoRowHeight":true}]
        }).to_string(), provenance, "20261019".into(), "20261025".into(), cancel).unwrap()
    }

    #[test]
    fn two_step_preview_has_no_output_and_confirmation_publishes_exact_prepared_bytes_once() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        let template_path = template_file(root.path());
        let original = fs::read(&template_path).unwrap();
        let pending = pending_preview(&template_path, &output, CancellationToken::new());
        let approved_bytes = pending.bytes.clone();
        let runtime = Runtime::default();
        let preview = runtime.insert(pending).unwrap();
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        assert_eq!(preview.template_name, "template.xlsx");
        assert_eq!(preview.cells[0].previous_value, "待填写");
        assert!(!preview.cells[0].replaces_formula);
        assert_eq!(preview.cells[1].previous_value, "=SUM(D3:D4)");
        assert!(preview.cells[1].replaces_formula);
        assert_eq!(preview.cells[1].value, json!(12.5));
        assert_eq!(preview.cells[2].value, json!(true));
        assert_eq!(preview.cells[3].value, Value::Null);
        assert_eq!(preview.styles[0].border, Some("black"));
        let sheet_preview = &preview.sheets[0];
        let filled_preview = sheet_preview.regions.iter().flat_map(|region| &region.cells)
            .find(|cell| cell.address == "B2").unwrap();
        assert_eq!(filled_preview.text, "预览确认的内容");
        assert!(filled_preview.changed && filled_preview.style.wrap_text);
        assert_eq!(filled_preview.style.border_top.as_ref().unwrap().color, "#000000");
        let serialized = serde_json::to_value(&preview).unwrap();
        assert_eq!(serialized["cells"][1]["replacesFormula"], true);
        assert_eq!(serialized["styles"][0]["wrapText"], true);
        assert!(serialized.get("bytes").is_none());
        assert!(serialized.get("provenance").is_none());
        let exported = runtime.confirm(&preview.id, |_| Ok(())).unwrap();
        assert_eq!(fs::read(&exported.path).unwrap(), approved_bytes);
        let output_workbook = umya_spreadsheet::reader::xlsx::read(Path::new(&exported.path)).unwrap();
        assert_eq!(output_workbook.sheet_by_name("Sheet1").unwrap().cell("B2").unwrap().formatted_value(), filled_preview.text);
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
        assert_eq!(fs::read(&template_path).unwrap(), original);
        let workbook = read_template(Path::new(&exported.path)).unwrap();
        let sheet = workbook.workbook.sheet_by_name("Sheet1").unwrap();
        assert_eq!(sheet.cell("B2").unwrap().value(), "预览确认的内容");
        assert!(!sheet.cell("C3").unwrap().is_formula());
        assert!(sheet.cell("B2").unwrap().style().alignment().unwrap().wrap_text());
        assert!(runtime.confirm(&preview.id, |_| Ok(())).unwrap_err().contains("重新生成"));
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    }

    #[test]
    fn discarded_expired_cancelled_and_invalidated_previews_never_create_files() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        let path = template_file(root.path());
        let runtime = Runtime::default();
        let preview = runtime.insert(pending_preview(&path, &output, CancellationToken::new())).unwrap();
        runtime.discard(&preview.id);
        runtime.discard(&preview.id);
        assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err());
        let mut pending = pending_preview(&path, &output, CancellationToken::new());
        pending.created = Instant::now() - PREVIEW_LIFETIME;
        let expired = runtime.insert(pending).unwrap();
        assert!(runtime.confirm(&expired.id, |_| Ok(())).is_err());
        let cancel = CancellationToken::new();
        let pending = pending_preview(&path, &output, cancel.clone());
        cancel.cancel();
        assert!(runtime.insert(pending).is_err());
        let cancel = CancellationToken::new();
        let preview = runtime.insert(pending_preview(&path, &output, cancel.clone())).unwrap();
        cancel.cancel();
        assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err());
        let preview = runtime.insert(pending_preview(&path, &output, CancellationToken::new())).unwrap();
        let error = runtime.confirm(&preview.id, |_| Err("内容已修改，请重新生成 Excel 预览".into())).unwrap_err();
        assert!(error.contains("内容已修改"));
        assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err());
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    }

    #[test]
    fn pending_cache_evicts_oldest_previews_at_its_bound() {
        let root = tempfile::tempdir().unwrap();
        let path = template_file(root.path());
        let runtime = Runtime::default();
        let first = runtime.insert(pending_preview(&path, root.path(), CancellationToken::new())).unwrap();
        for _ in 0..MAX_PENDING_PREVIEWS {
            runtime.insert(pending_preview(&path, root.path(), CancellationToken::new())).unwrap();
        }
        assert_eq!(runtime.0.lock().unwrap().len(), MAX_PENDING_PREVIEWS);
        assert!(runtime.confirm(&first.id, |_| Ok(())).is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_confirmation_is_rejected_without_duplicate_publication() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        let path = template_file(root.path());
        let runtime = Runtime::default();
        let preview = runtime.insert(pending_preview(&path, &output, CancellationToken::new())).unwrap();
        let other = runtime.clone();
        let id = preview.id.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = std::thread::spawn(move || other.confirm(&id, |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        }));
        entered_rx.recv().unwrap();
        assert!(runtime.confirm(&preview.id, |_| Ok(())).unwrap_err().contains("正在输出"));
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        release_tx.send(()).unwrap();
        first.join().unwrap().unwrap();
        assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err());
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    }

    #[test]
    fn temporary_publication_failure_keeps_the_same_approved_preview_for_retry() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        let backup = root.path().join("output-backup");
        fs::create_dir(&output).unwrap();
        let path = template_file(root.path());
        let runtime = Runtime::default();
        let preview = runtime.insert(pending_preview(&path, &output, CancellationToken::new())).unwrap();
        fs::rename(&output, &backup).unwrap();
        fs::write(&output, "not a directory").unwrap();
        assert!(runtime.confirm(&preview.id, |_| Ok(())).is_err());
        assert_eq!(runtime.0.lock().unwrap().len(), 1);
        fs::remove_file(&output).unwrap();
        fs::rename(&backup, &output).unwrap();
        runtime.confirm(&preview.id, |_| Ok(())).unwrap();
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    }

    #[test]
    fn rendering_errors_are_reported_before_preview_without_any_output() {
        let root = tempfile::tempdir().unwrap();
        let path = template_file(root.path());
        let template = read_template(&path).unwrap();
        let error = render_workbook(template,
            &json!({"cells":[{"sheet":"Sheet1","cell":"B2","value":"字".repeat(5000)}],"styles":[{"sheet":"Sheet1","range":"B2","wrapText":true,"autoRowHeight":true}]}).to_string(),
            None).unwrap_err();
        assert!(error.contains("行高上限"));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

}
