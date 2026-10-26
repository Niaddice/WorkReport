use super::{check_cancelled, parse_coordinate, parse_range, row_height, CellRange, FillPlan};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tokio_util::sync::CancellationToken;
use umya_spreadsheet::{drawing::Theme, Border, Cell, CellRawValue, Color, EnumTrait, Style, Workbook, Worksheet};

const PRIMARY_COLUMNS: u32 = 40;
const PRIMARY_ROWS: u32 = 120;
const REGION_COLUMNS: u32 = 60;
const REGION_ROWS: u32 = 200;
// Primary template windows share a workbook budget. Additional regions always
// include every planned value/style change (the fill plan itself is <= 5000 cells).
const PRIMARY_WORKBOOK_CELLS: u64 = 30_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelSheetPreview {
    pub name: String,
    pub state: String,
    pub regions: Vec<ExcelPreviewRegion>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreviewRegion {
    pub start_row: u32,
    pub start_column: u32,
    pub row_count: u32,
    pub column_count: u32,
    pub columns: Vec<ExcelPreviewColumn>,
    pub rows: Vec<ExcelPreviewRow>,
    pub cells: Vec<ExcelSheetCell>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreviewColumn {
    pub index: u32,
    pub label: String,
    pub width: f64,
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreviewRow {
    pub index: u32,
    pub height: f64,
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelSheetCell {
    pub row: u32,
    pub column: u32,
    pub address: String,
    pub text: String,
    pub formula: Option<String>,
    pub style: ExcelCellStyle,
    pub row_span: u32,
    pub column_span: u32,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelCellStyle {
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: String,
    pub background: String,
    pub horizontal_align: &'static str,
    pub vertical_align: &'static str,
    pub wrap_text: bool,
    pub border_top: Option<ExcelCellBorder>,
    pub border_right: Option<ExcelCellBorder>,
    pub border_bottom: Option<ExcelCellBorder>,
    pub border_left: Option<ExcelCellBorder>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExcelCellBorder {
    pub style: &'static str,
    pub width: f64,
    pub color: String,
}

fn column_label(mut column: u32) -> String {
    let mut label = Vec::new();
    while column > 0 {
        column -= 1;
        label.push((b'A' + (column % 26) as u8) as char);
        column /= 26;
    }
    label.iter().rev().collect()
}

fn address(column: u32, row: u32) -> String { format!("{}{row}", column_label(column)) }

fn includes(outer: CellRange, inner: CellRange) -> bool {
    outer.contains(inner.left, inner.top) && outer.contains(inner.right, inner.bottom)
}

fn intersection(a: CellRange, b: CellRange) -> Option<CellRange> {
    let value = CellRange {
        left: a.left.max(b.left), top: a.top.max(b.top),
        right: a.right.min(b.right), bottom: a.bottom.min(b.bottom),
    };
    (value.left <= value.right && value.top <= value.bottom).then_some(value)
}

fn add_range(regions: &mut Vec<CellRange>, range: CellRange) {
    if regions.iter().any(|region| includes(*region, range)) { return; }
    let mut top = range.top;
    while top <= range.bottom {
        let bottom = range.bottom.min(top.saturating_add(REGION_ROWS - 1));
        let mut left = range.left;
        while left <= range.right {
            let right = range.right.min(left.saturating_add(REGION_COLUMNS - 1));
            let tile = CellRange { left, top, right, bottom };
            if !regions.iter().any(|region| includes(*region, tile)) { regions.push(tile); }
            left = right + 1;
        }
        top = bottom + 1;
    }
}

/// 读取已导出工作簿的静态预览，不做任何填充。
pub fn extract_static(workbook: &Workbook) -> Result<Vec<ExcelSheetPreview>, String> {
    extract(workbook, &FillPlan { cells: Vec::new(), styles: Vec::new() }, None)
}

pub(super) fn extract(
    workbook: &Workbook,
    plan: &FillPlan,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<ExcelSheetPreview>, String> {
    let mut primary_budget = PRIMARY_WORKBOOK_CELLS;
    let mut result = Vec::new();
    for sheet in workbook.sheet_collection() {
        check_cancelled(cancel)?;
        let changes = plan.cells.iter().filter(|cell| cell.sheet == sheet.name())
            .map(|cell| parse_coordinate(&cell.cell)).collect::<Result<HashSet<_>, _>>()?;
        let style_changes = plan.styles.iter().filter(|style| style.sheet == sheet.name())
            .map(|style| parse_range(&style.range)).collect::<Result<Vec<_>, _>>()?;
        let merges = sheet.merge_cells().iter().map(|range| parse_range(&range.range().replace('$', "")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut extent = CellRange { left: 1, top: 1, right: 1, bottom: 1 };
        let mut first = true;
        // The library's internal hashmap key is (row, column), unlike its
        // public cell((column, row)) API. Read the explicit coordinates.
        let points = sheet.collection_to_hashmap().values().map(|cell| (cell.coordinate().col_num(), cell.coordinate().row_num()))
            .chain(merges.iter().flat_map(|range| [(range.left, range.top), (range.right, range.bottom)]))
            .collect::<Vec<_>>();
        for &(column, row) in &points {
            if first { extent = CellRange { left: column, top: row, right: column, bottom: row }; first = false; }
            extent.left = extent.left.min(column); extent.right = extent.right.max(column);
            extent.top = extent.top.min(row); extent.bottom = extent.bottom.max(row);
        }
        // Keep normal worksheets anchored at A1. For sparse far-away data use
        // the actual starting cell rather than allocating the empty lead-in.
        if extent.left <= PRIMARY_COLUMNS { extent.left = 1; }
        if extent.top <= PRIMARY_ROWS { extent.top = 1; }
        let window = CellRange { left: extent.left, top: extent.top,
            right: extent.right.min(extent.left + PRIMARY_COLUMNS - 1),
            bottom: extent.bottom.min(extent.top + PRIMARY_ROWS - 1) };
        let mut local_right = window.left;
        let mut local_bottom = window.top;
        for &(column, row) in &points {
            if window.contains(column, row) { local_right = local_right.max(column); local_bottom = local_bottom.max(row); }
        }
        for merge in &merges {
            if let Some(clipped) = intersection(window, *merge) {
                local_right = local_right.max(clipped.right); local_bottom = local_bottom.max(clipped.bottom);
            }
        }
        // A distant annotation must not stretch a small weekly template into
        // an otherwise empty 40-by-120 grid. It gets its own region below.
        let width = local_right - window.left + 1;
        let height = (local_bottom - window.top + 1)
            .min((primary_budget / u64::from(width)).max(1) as u32);
        let primary = CellRange { left: extent.left, top: extent.top,
            right: extent.left + width - 1, bottom: extent.top + height - 1 };
        primary_budget = primary_budget.saturating_sub(primary.area());
        let mut ranges = vec![primary];
        // Stable ordering makes region selection repeatable across previews.
        let mut change_points = changes.iter().copied().collect::<Vec<_>>();
        change_points.sort_by_key(|(column, row)| (*row, *column));
        for (column, row) in change_points {
            if ranges.iter().any(|range| range.contains(column, row)) { continue; }
            add_range(&mut ranges, CellRange { left: column.saturating_sub(1).max(1),
                top: row.saturating_sub(1).max(1), right: column.saturating_add(1).min(super::MAX_COLUMN),
                bottom: row.saturating_add(1).min(super::MAX_ROW) });
        }
        for range in &style_changes { add_range(&mut ranges, *range); }
        let mut truncated = !includes(primary, extent);
        let regions = ranges.iter().map(|range| {
            extract_region(workbook.theme(), sheet, *range, &merges, &changes, &style_changes, &mut truncated, cancel)
        }).collect::<Result<Vec<_>, _>>()?;
        result.push(ExcelSheetPreview { name: sheet.name().to_string(),
            state: sheet.state().value_string().to_string(), regions, truncated });
    }
    Ok(result)
}

fn extract_region(
    theme: &Theme,
    sheet: &Worksheet,
    range: CellRange,
    merges: &[CellRange],
    changes: &HashSet<(u32, u32)>,
    style_changes: &[CellRange],
    truncated: &mut bool,
    cancel: Option<&CancellationToken>,
) -> Result<ExcelPreviewRegion, String> {
    let default_width = sheet.sheet_format_properties().default_column_width();
    let default_width = if default_width.is_finite() && default_width > 0.0 { default_width } else { 8.43 };
    let columns = (range.left..=range.right).map(|index| {
        let dimension = sheet.column_dimension_by_number(index);
        let width = dimension.map(|column| column.width()).filter(|width| width.is_finite() && *width > 0.0).unwrap_or(default_width);
        ExcelPreviewColumn { index, label: column_label(index), width: width.clamp(0.01, 255.0) * 7.0 + 5.0,
            hidden: dimension.is_some_and(|column| column.hidden()) }
    }).collect();
    let rows = (range.top..=range.bottom).map(|index| ExcelPreviewRow { index, height: row_height(sheet, index).clamp(0.75, super::MAX_ROW_HEIGHT) * 4.0 / 3.0,
        hidden: sheet.row_dimension(index).is_some_and(|row| row.hidden()) }).collect();
    let mut anchors = HashMap::new();
    let mut covered = HashSet::new();
    let mut merge_work = 0;
    for merge in merges {
        let Some(clipped) = intersection(*merge, range) else { continue };
        merge_work += clipped.area();
        if merge_work > 100_000 { *truncated = true; break; }
        if !includes(range, *merge) { *truncated = true; }
        anchors.insert((clipped.left, clipped.top), (*merge, clipped));
        for row in clipped.top..=clipped.bottom { for column in clipped.left..=clipped.right {
            if (column, row) != (clipped.left, clipped.top) { covered.insert((column, row)); }
        } }
    }
    let mut cells = Vec::new();
    for row in range.top..=range.bottom {
        check_cancelled(cancel)?;
        for column in range.left..=range.right {
            if covered.contains(&(column, row)) { continue; }
            let single = CellRange { left: column, right: column, top: row, bottom: row };
            let (source, visible) = anchors.get(&(column, row)).copied().unwrap_or((single, single));
            let cell = sheet.cell((source.left, source.top));
            let formula = cell.filter(|cell| cell.is_formula()).map(|cell| format!("={}", cell.formula()));
            let text = cell.map(|cell| cell.formatted_value()).unwrap_or_default();
            let text = if cell.is_some_and(|cell| cell.is_formula() && matches!(cell.raw_value(), CellRawValue::Empty)) {
                formula.clone().unwrap_or(text)
            } else { text };
            let mut style = cell_style(theme, sheet, source.left, source.top, cell);
            // Excel commonly stores merged outlines on the cells at the far
            // edge, not on the anchor itself. Carry these edges onto the one
            // rendered merged cell without mutating the workbook.
            if source.area() > 1 {
                if let Some(edge) = sheet.cell((source.right, source.top)).and_then(|cell| cell.style().borders()) {
                    style.border_right = border(theme, edge.right()).or(style.border_right);
                }
                if let Some(edge) = sheet.cell((source.left, source.bottom)).and_then(|cell| cell.style().borders()) {
                    style.border_bottom = border(theme, edge.bottom()).or(style.border_bottom);
                }
                if let Some(corner) = sheet.cell((source.right, source.bottom)).and_then(|cell| cell.style().borders()) {
                    style.border_right = style.border_right.or_else(|| border(theme, corner.right()));
                    style.border_bottom = style.border_bottom.or_else(|| border(theme, corner.bottom()));
                }
            }
            let changed = changes.contains(&(source.left, source.top)) || changes.contains(&(column, row))
                || style_changes.iter().any(|change| intersection(*change, source).is_some());
            cells.push(ExcelSheetCell { row, column, address: address(source.left, source.top), text, formula, style,
                row_span: visible.bottom - visible.top + 1, column_span: visible.right - visible.left + 1, changed });
        }
    }
    Ok(ExcelPreviewRegion { start_row: range.top, start_column: range.left, row_count: range.bottom - range.top + 1,
        column_count: range.right - range.left + 1, columns, rows, cells })
}

fn css_color(theme: &Theme, color: &Color, fallback: &str) -> String {
    if color.automatic() || color == &Color::default() { return fallback.to_string(); }
    let value = color.argb_with_theme(theme);
    let value = value.trim_start_matches('#');
    if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        // Spreadsheet cell colors use the RGB channels. Writers commonly
        // serialize a six-digit color as 00RRGGBB; that leading byte must not
        // turn an explicitly specified font/fill into a transparent color.
        return format!("#{}", &value[2..]);
    }
    if value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) { return format!("#{value}"); }
    fallback.to_string()
}

fn border(theme: &Theme, value: &Border) -> Option<ExcelCellBorder> {
    let (style, width) = match value.border_style() {
        "none" | "" => return None,
        "double" => ("double", 3.0),
        "dotted" => ("dotted", 1.0),
        "dashDot" | "dashDotDot" | "dashed" | "slantDashDot" => ("dashed", 1.0),
        "mediumDashDot" | "mediumDashDotDot" | "mediumDashed" => ("dashed", 2.0),
        "medium" => ("solid", 2.0),
        "thick" => ("solid", 3.0),
        "hair" => ("solid", 0.5),
        _ => ("solid", 1.0),
    };
    Some(ExcelCellBorder { style, width, color: value.color().as_ref().map(|color| css_color(theme, color, "#000000")).unwrap_or_else(|| "#000000".into()) })
}

fn cell_style(theme: &Theme, sheet: &Worksheet, column: u32, row: u32, cell: Option<&Cell>) -> ExcelCellStyle {
    let defaults = Style::default_value();
    let candidates = [cell.map(|cell| cell.style()), sheet.row_dimension(row).map(|row| row.style()),
        sheet.column_dimension_by_number(column).map(|column| column.style()), Some(&defaults)];
    let font = candidates.iter().flatten().find_map(|style| style.font()).expect("default font");
    let fill = candidates.iter().flatten().find_map(|style| style.fill());
    let background = fill.and_then(|fill| fill.pattern_fill())
        .filter(|pattern| pattern.pattern_type().value_string() != "none")
        .and_then(|pattern| pattern.foreground_color().or_else(|| pattern.background_color()))
        .map(|color| css_color(theme, color, "#FFFFFF")).unwrap_or_else(|| "#FFFFFF".into());
    let alignment = candidates.iter().flatten().find_map(|style| style.alignment());
    let numeric = cell.is_some_and(|cell| cell.value_number().is_some());
    let horizontal_align = match alignment.map(|value| value.horizontal().value_string()).unwrap_or("general") {
        "center" | "centerContinuous" | "distributed" => "center", "right" => "right", "justify" => "justify",
        "general" if numeric => "right", _ => "left",
    };
    let vertical_align = match alignment.map(|value| value.vertical().value_string()).unwrap_or("bottom") {
        "top" => "top", "center" | "distributed" | "justify" => "middle", _ => "bottom",
    };
    let borders = candidates.iter().flatten().find_map(|style| style.borders());
    let size = font.size();
    ExcelCellStyle { font_family: if font.name().is_empty() { "Calibri".into() } else { font.name().into() },
        font_size: if size.is_finite() && size > 0.0 { size.clamp(1.0, 409.5) * 4.0 / 3.0 } else { 44.0 / 3.0 },
        bold: font.bold(), italic: font.italic(), underline: font.font_underline().val().value_string() != "none", strike: font.strikethrough(),
        color: css_color(theme, font.color(), "#000000"), background, horizontal_align, vertical_align,
        wrap_text: alignment.is_some_and(|alignment| alignment.wrap_text()),
        border_top: borders.and_then(|borders| border(theme, borders.top())), border_right: borders.and_then(|borders| border(theme, borders.right())),
        border_bottom: borders.and_then(|borders| border(theme, borders.bottom())), border_left: borders.and_then(|borders| border(theme, borders.left())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use umya_spreadsheet::{HorizontalAlignmentValues, SheetStateValues, VerticalAlignmentValues};

    fn plan(value: serde_json::Value) -> FillPlan { serde_json::from_value(value).unwrap() }
    fn find_cell<'a>(sheet: &'a ExcelSheetPreview, address: &str) -> &'a ExcelSheetCell {
        sheet.regions.iter().flat_map(|region| &region.cells).find(|cell| cell.address == address).unwrap()
    }

    #[test]
    fn worksheet_preview_preserves_template_geometry_merges_hidden_axes_and_styles() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_string("工作周报");
        sheet.add_merge_cells("A1:C2");
        sheet.cell_mut("C4").set_value_string("正文\n第二行");
        sheet.column_dimension_by_number_mut(3).set_width(30.0).set_hidden(true);
        sheet.row_dimension_mut(2).set_height(30.0).set_hidden(true);
        let style = sheet.cell_mut("A1").style_mut();
        style.font_mut().set_name("SimSun").set_size(18.0).set_bold(true).set_italic(true)
            .set_underline("single").set_strikethrough(true);
        style.font_mut().color_mut().set_argb_str("FF123456");
        style.set_background_color_solid("FFFFEECC");
        style.alignment_mut().set_horizontal(HorizontalAlignmentValues::Center);
        style.alignment_mut().set_vertical(VerticalAlignmentValues::Center);
        style.alignment_mut().set_wrap_text(true);
        style.borders_mut().top_mut().set_border_style("thick");
        sheet.cell_mut("C1").style_mut().borders_mut().right_mut().set_border_style("double");
        sheet.cell_mut("A2").style_mut().borders_mut().bottom_mut().set_border_style("dashed");
        workbook.new_sheet("隐藏数据").unwrap().set_state(SheetStateValues::Hidden);
        let preview = extract(&workbook, &plan(json!({"cells": [{"sheet":"Sheet1","cell":"C4","value":"正文\n第二行"}]})), None).unwrap();
        let sheet = &preview[0];
        assert!(!sheet.truncated);
        assert_eq!(preview[1].state, "hidden");
        let region = &sheet.regions[0];
        assert_eq!((region.start_column, region.start_row, region.column_count, region.row_count), (1, 1, 3, 4));
        assert_eq!(region.columns[2].width, 215.0);
        assert!(region.columns[2].hidden);
        assert_eq!(region.rows[1].height, 40.0);
        assert!(region.rows[1].hidden);
        let title = find_cell(sheet, "A1");
        assert_eq!((title.row_span, title.column_span), (2, 3));
        assert_eq!(title.style.font_family, "SimSun");
        assert_eq!(title.style.font_size, 24.0);
        assert!(title.style.bold && title.style.italic && title.style.underline && title.style.strike);
        assert_eq!(title.style.color, "#123456");
        assert_eq!(title.style.background, "#FFEECC");
        assert_eq!(title.style.horizontal_align, "center");
        assert_eq!(title.style.vertical_align, "middle");
        assert_eq!(title.style.border_top.as_ref().unwrap().width, 3.0);
        assert_eq!(title.style.border_right.as_ref().unwrap().style, "double");
        assert_eq!(title.style.border_bottom.as_ref().unwrap().style, "dashed");
        assert!(!region.cells.iter().any(|cell| (cell.column, cell.row) == (2, 2)));
        assert!(find_cell(sheet, "C4").changed);
    }

    #[test]
    fn numeric_formats_and_cached_formulas_are_displayed_without_calculation_or_mutation() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_number(0.125).style_mut().numbering_format_mut().set_format_code("0.0%");
        sheet.cell_mut("B1").set_formula("SUM(A2:A3)").set_formula_result_number(1234.5)
            .style_mut().numbering_format_mut().set_format_code("#,##0.00");
        sheet.cell_mut("C1").set_formula("SUM(B2:B3)");
        sheet.cell_mut("D1").set_value_string("<script>alert(1)</script>");
        sheet.cell_mut("E1").set_formula("IF(1=1,\"\",\"x\")").set_formula_result_string("");
        let preview = extract(&workbook, &plan(json!({})), None).unwrap();
        assert_eq!(find_cell(&preview[0], "A1").text, "12.5%");
        assert_eq!(find_cell(&preview[0], "A1").style.horizontal_align, "right");
        assert_eq!(find_cell(&preview[0], "B1").text, "1,234.50");
        assert_eq!(find_cell(&preview[0], "B1").formula.as_deref(), Some("=SUM(A2:A3)"));
        assert_eq!(find_cell(&preview[0], "C1").text, "=SUM(B2:B3)");
        assert_eq!(find_cell(&preview[0], "D1").text, "<script>alert(1)</script>");
        assert_eq!(find_cell(&preview[0], "E1").text, "");
        assert_eq!(workbook.sheet_by_name("Sheet1").unwrap().cell("B1").unwrap().value_number(), Some(1234.5));
    }

    #[test]
    fn inherited_row_column_styles_and_theme_tints_have_safe_resolved_colors() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("C3").set_value_string("表格边界");
        sheet.row_dimension_mut(2).style_mut().font_mut().set_name("Arial").set_size(15.0);
        sheet.column_dimension_by_number_mut(2).style_mut().set_background_color_solid("FFCCEEFF");
        sheet.cell_mut("A1").style_mut().font_mut().color_mut().set_theme_index(4).set_tint(0.5);
        let expected = css_color(workbook.theme(), workbook.sheet_by_name("Sheet1").unwrap().cell("A1").unwrap().style().font().unwrap().color(), "#000000");
        assert_ne!(expected, "#000000");
        let preview = extract(&workbook, &plan(json!({})), None).unwrap();
        assert_eq!(find_cell(&preview[0], "A1").style.color, expected);
        assert_eq!(find_cell(&preview[0], "A2").style.font_family, "Arial");
        assert_eq!(find_cell(&preview[0], "A2").style.font_size, 20.0);
        assert_eq!(find_cell(&preview[0], "B2").style.background, "#CCEEFF");
        assert_eq!(find_cell(&preview[0], "C2").style.background, "#FFFFFF");
    }

    #[test]
    fn rgb_colors_with_zero_or_opaque_argb_prefixes_keep_the_same_excel_color() {
        let workbook = umya_spreadsheet::new_file();
        for (value, expected) in [("00174A7E", "#174A7E"), ("FF174A7E", "#174A7E"), ("00000000", "#000000")] {
            let mut color = Color::default(); color.set_argb_str(value);
            assert_eq!(css_color(workbook.theme(), &color, "#FFFFFF"), expected);
        }
        assert_eq!(css_color(workbook.theme(), &Color::default(), "#FFFFFF"), "#FFFFFF");
    }

    #[test]
    fn distant_updates_and_style_only_areas_are_always_in_bounded_regions() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_string("标题");
        sheet.cell_mut("K18").set_value_string("普通周报区域末尾");
        sheet.cell_mut("XFD1048576").set_value_string("最远修改");
        sheet.cell_mut("G5000").style_mut().alignment_mut().set_wrap_text(true);
        let plan = plan(json!({"cells":[{"sheet":"Sheet1","cell":"XFD1048576","value":"最远修改"}],
            "styles":[{"sheet":"Sheet1","range":"G5000:H5002","wrapText":true}]}));
        let preview = extract(&workbook, &plan, None).unwrap();
        let sheet = &preview[0];
        assert!(sheet.truncated);
        assert_eq!((sheet.regions[0].column_count, sheet.regions[0].row_count), (11, 18));
        assert!(sheet.regions.len() >= 3);
        assert!(find_cell(sheet, "XFD1048576").changed);
        assert!(find_cell(sheet, "H5002").changed);
        assert!(find_cell(sheet, "G5000").style.wrap_text);
        assert!(sheet.regions.iter().all(|region| region.column_count <= REGION_COLUMNS && region.row_count <= REGION_ROWS));
        assert!(sheet.regions.iter().map(|region| u64::from(region.column_count) * u64::from(region.row_count)).sum::<u64>() < 20_000);
    }

    #[test]
    fn large_merges_clip_safely_and_report_partial_template_while_retaining_anchor_text() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_string("巨大合并标题");
        sheet.add_merge_cells("A1:XFD1048576");
        let preview = extract(&workbook, &plan(json!({"cells":[{"sheet":"Sheet1","cell":"A1","value":"巨大合并标题"}]})), None).unwrap();
        assert!(preview[0].truncated);
        let title = find_cell(&preview[0], "A1");
        assert_eq!(title.text, "巨大合并标题");
        assert_eq!((title.column_span, title.row_span), (PRIMARY_COLUMNS, PRIMARY_ROWS));
        assert!(title.changed);
        assert_eq!(preview[0].regions[0].cells.len(), 1);
    }

    #[test]
    fn extraction_is_cancellable_and_keeps_full_changes_available_as_separate_regions() {
        let workbook = umya_spreadsheet::new_file();
        let token = CancellationToken::new(); token.cancel();
        assert!(extract(&workbook, &plan(json!({})), Some(&token)).unwrap_err().contains("已停止"));
        let mut workbook = umya_spreadsheet::new_file();
        for index in 0..31 { workbook.new_sheet(format!("表{index}")).unwrap(); }
        for sheet in workbook.sheet_collection_mut() { sheet.cell_mut("AN120").set_value_string("末尾"); }
        let preview = extract(&workbook, &plan(json!({"cells":[{"sheet":"表30","cell":"AZ9999","value":"最后工作表修改"}]})), None).unwrap();
        let area = preview.iter().flat_map(|sheet| &sheet.regions).map(|region| u64::from(region.row_count) * u64::from(region.column_count)).sum::<u64>();
        assert!(area <= PRIMARY_WORKBOOK_CELLS + 32 * u64::from(PRIMARY_COLUMNS) + 9);
        assert!(find_cell(preview.last().unwrap(), "AZ9999").changed);
    }

    #[test]
    fn unset_and_explicit_underline_properties_remain_distinct_in_preview() {
        let mut workbook = umya_spreadsheet::new_file();
        let sheet = workbook.sheet_by_name_mut("Sheet1").unwrap();
        sheet.cell_mut("A1").set_value_string("普通文本");
        sheet.cell_mut("B1").set_value_string("有下划线").style_mut().font_mut().set_underline("single");
        sheet.cell_mut("C1").set_value_string("明确无下划线").style_mut().font_mut().set_underline("none");
        let preview = extract(&workbook, &plan(json!({})), None).unwrap();
        assert!(!find_cell(&preview[0], "A1").style.underline);
        assert!(find_cell(&preview[0], "B1").style.underline);
        assert!(!find_cell(&preview[0], "C1").style.underline);
    }
}
