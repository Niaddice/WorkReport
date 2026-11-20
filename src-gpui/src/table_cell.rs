use std::ops::Range;

use markdown::{ParseOptions, mdast::Node, to_mdast};

/// Finds the literal text of the first body cell in the first top-level table.
pub fn first_plain_body_cell(source: &str) -> Option<Range<usize>> {
    let mut options = ParseOptions::gfm();
    options.constructs.frontmatter = true;
    let Node::Root(root) = to_mdast(source, &options).ok()? else {
        return None;
    };
    let table = root.children.iter().find_map(|node| match node {
        Node::Table(table) => Some(table),
        _ => None,
    })?;
    let Node::TableRow(row) = table.children.get(1)? else {
        return None;
    };
    let Node::TableCell(cell) = row.children.first()? else {
        return None;
    };
    let [Node::Text(text)] = cell.children.as_slice() else {
        return None;
    };
    let position = text.position.as_ref()?;
    let range = position.start.offset..position.end.offset;
    (source.get(range.clone())? == text.value).then_some(range)
}

pub fn replace_first_plain_body_cell(
    source: &str,
    replacement: &str,
) -> Result<String, &'static str> {
    let range = first_plain_body_cell(source).ok_or("第一个表格的首个正文单元格不支持编辑")?;
    if replacement.is_empty()
        || replacement.trim() != replacement
        || replacement.contains(['\r', '\n', '|', '\\'])
    {
        return Err("请输入非空的单行文本，首尾不能留空，也不能含有竖线或反斜杠");
    }

    let edited_range = range.start..range.start + replacement.len();
    let mut edited = source.to_owned();
    edited.replace_range(range, replacement);
    if first_plain_body_cell(&edited) != Some(edited_range) {
        return Err("修改会改变单元格的 Markdown 结构，请输入纯文本");
    }
    Ok(edited)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../fixtures/gpui-migration/workspace/日报/2026/日报_20261119.md");

    #[test]
    fn replaces_only_the_fixture_cell_bytes() {
        let range = first_plain_body_cell(FIXTURE).unwrap();
        assert_eq!(&FIXTURE[range.clone()], "日报");
        let replacement = "中文、WorkReport 与 👩🏽‍💻";
        let result = replace_first_plain_body_cell(FIXTURE, replacement).unwrap();
        assert_eq!(
            result,
            format!(
                "{}{replacement}{}",
                &FIXTURE[..range.start],
                &FIXTURE[range.end..]
            )
        );
        assert_eq!(
            replace_first_plain_body_cell(FIXTURE, "日报"),
            Ok(FIXTURE.to_owned())
        );
    }

    #[test]
    fn rich_or_missing_first_body_cells_do_not_retarget_the_edit() {
        for first_cell in ["**加粗**", "`代码`", "A &amp; B", "a\\|b", ""] {
            let source =
                format!("| H1 | H2 |\n| --- | --- |\n| {first_cell} | 普通文本 |\n\n{FIXTURE}");
            assert!(first_plain_body_cell(&source).is_none(), "{first_cell:?}");
            assert!(replace_first_plain_body_cell(&source, "更新").is_err());
        }
        let header_only = "| H1 | H2 |\n| --- | --- |\n";
        assert!(first_plain_body_cell(header_only).is_none());
    }

    #[test]
    fn rejects_markup_and_table_delimiters_in_the_replacement() {
        for replacement in [
            "",
            " 空格",
            "空格 ",
            "两\n行",
            "a|b",
            "a\\|b",
            "**加粗**",
            "&amp;",
        ] {
            assert!(
                replace_first_plain_body_cell(FIXTURE, replacement).is_err(),
                "{replacement:?}"
            );
        }
    }

    #[test]
    fn ignores_frontmatter_and_code_tables_and_preserves_crlf() {
        let source = "---\r\n| A | B |\r\n| --- | --- |\r\n| YAML | 标记 |\r\n---\r\n\r\n```md\r\n| C | D |\r\n| --- | --- |\r\n| 代码 | 标记 |\r\n```\r\n\r\n| E | F |\r\n| --- | --- |\r\n| 正文 | 保持 |\r\n";
        let range = first_plain_body_cell(source).unwrap();
        assert_eq!(&source[range], "正文");
        assert_eq!(
            replace_first_plain_body_cell(source, "新内容").unwrap(),
            source.replace("正文", "新内容")
        );
    }
}
