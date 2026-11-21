use gpui_kit::{
    Context, Entity, SharedString, Subscription, Window,
    base::{Button as FlatButton, Disableable},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, InputEvent},
    },
    div, prelude::*, px,
};

use crate::Prototype;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RichBlockKind {
    Heading { level: u8, text: String },
    Task { checked: bool, text: String },
    Bullet { text: String },
    Ordered { number: usize, text: String },
    Quote { text: String },
    CodeBlock { lang: String, code: String },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Divider,
    Paragraph { text: String },
    Preserved { raw: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RichBlock {
    pub(crate) kind: RichBlockKind,
    /// Original raw Markdown slice if this block has not been mutated in rich mode.
    raw_source: Option<String>,
}

impl RichBlock {
    fn from_kind(kind: RichBlockKind) -> Self {
        Self {
            kind,
            raw_source: None,
        }
    }

    fn with_raw(kind: RichBlockKind, raw: String) -> Self {
        Self {
            kind,
            raw_source: Some(raw),
        }
    }

    pub(crate) fn editable_text(&self) -> Option<String> {
        match &self.kind {
            RichBlockKind::Heading { text, .. }
            | RichBlockKind::Task { text, .. }
            | RichBlockKind::Bullet { text }
            | RichBlockKind::Ordered { text, .. }
            | RichBlockKind::Quote { text }
            | RichBlockKind::Paragraph { text } => Some(text.clone()),
            RichBlockKind::CodeBlock { code, .. } => Some(code.clone()),
            RichBlockKind::Preserved { raw } => Some(raw.clone()),
            RichBlockKind::Table { headers, rows } => {
                let mut lines = Vec::new();
                lines.push(format!("| {} |", headers.join(" | ")));
                let sep = headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ");
                lines.push(format!("| {sep} |"));
                for row in rows {
                    lines.push(format!("| {} |", row.join(" | ")));
                }
                Some(lines.join("\n"))
            }
            RichBlockKind::Divider => None,
        }
    }

    pub(crate) fn apply_edited_text(&mut self, value: String) {
        self.raw_source = None;
        match &mut self.kind {
            RichBlockKind::Heading { text, .. }
            | RichBlockKind::Task { text, .. }
            | RichBlockKind::Bullet { text }
            | RichBlockKind::Ordered { text, .. }
            | RichBlockKind::Quote { text }
            | RichBlockKind::Paragraph { text } => {
                *text = value;
            }
            RichBlockKind::CodeBlock { code, .. } => {
                *code = value;
            }
            RichBlockKind::Preserved { raw } => {
                *raw = value;
            }
            RichBlockKind::Table { headers, rows } => {
                if let Some((parsed_headers, parsed_rows)) = parse_table_lines(
                    &value.lines().collect::<Vec<_>>(),
                ) {
                    *headers = parsed_headers;
                    *rows = parsed_rows;
                }
            }
            RichBlockKind::Divider => {}
        }
    }

    pub(crate) fn to_markdown(&self) -> String {
        if let Some(raw) = &self.raw_source {
            return raw.clone();
        }
        match &self.kind {
            RichBlockKind::Heading { level, text } => {
                let hashes = "#".repeat((*level).clamp(1, 6) as usize);
                format!("{hashes} {text}")
            }
            RichBlockKind::Task { checked, text } => {
                let mark = if *checked { "x" } else { " " };
                format!("- [{mark}] {text}")
            }
            RichBlockKind::Bullet { text } => format!("- {text}"),
            RichBlockKind::Ordered { number, text } => format!("{number}. {text}"),
            RichBlockKind::Quote { text } => text
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            RichBlockKind::CodeBlock { lang, code } => {
                if lang.is_empty() {
                    format!("```\n{code}\n```")
                } else {
                    format!("```{lang}\n{code}\n```")
                }
            }
            RichBlockKind::Table { headers, rows } => {
                let mut lines = Vec::with_capacity(rows.len() + 2);
                lines.push(format!("| {} |", headers.join(" | ")));
                let sep = headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ");
                lines.push(format!("| {sep} |"));
                for row in rows {
                    lines.push(format!("| {} |", row.join(" | ")));
                }
                lines.join("\n")
            }
            RichBlockKind::Divider => "---".into(),
            RichBlockKind::Paragraph { text } => text.clone(),
            RichBlockKind::Preserved { raw } => raw.clone(),
        }
    }

    fn kind_label(&self) -> &'static str {
        match &self.kind {
            RichBlockKind::Heading { level, .. } => match level {
                1 => "H1",
                2 => "H2",
                3 => "H3",
                4 => "H4",
                5 => "H5",
                _ => "H6",
            },
            RichBlockKind::Task { .. } => "待办",
            RichBlockKind::Bullet { .. } => "列表",
            RichBlockKind::Ordered { .. } => "有序",
            RichBlockKind::Quote { .. } => "引用",
            RichBlockKind::CodeBlock { .. } => "代码",
            RichBlockKind::Table { .. } => "表格",
            RichBlockKind::Divider => "分割线",
            RichBlockKind::Paragraph { .. } => "正文",
            RichBlockKind::Preserved { .. } => "原文保留",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RichDocument {
    pub(crate) blocks: Vec<RichBlock>,
}

impl RichDocument {
    pub(crate) fn parse(source: &str) -> Self {
        let normalized = source.replace("\r\n", "\n");
        let lines: Vec<&str> = normalized.split('\n').collect();
        let mut blocks = Vec::new();
        let mut idx = 0usize;

        // Preserve leading YAML frontmatter byte-for-byte.
        if lines.first().copied() == Some("---") {
            let mut end = 1usize;
            while end < lines.len() {
                if lines[end] == "---" || lines[end] == "..." {
                    let raw = lines[..=end].join("\n");
                    blocks.push(RichBlock::with_raw(
                        RichBlockKind::Preserved { raw: raw.clone() },
                        raw,
                    ));
                    idx = end + 1;
                    break;
                }
                end += 1;
            }
        }

        while idx < lines.len() {
            let line = lines[idx];
            if line.trim().is_empty() {
                idx += 1;
                continue;
            }

            // Reference link definition `[id]: url` -> preserve verbatim.
            if is_ref_definition(line) {
                let raw = line.to_string();
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Preserved { raw: raw.clone() },
                    raw,
                ));
                idx += 1;
                continue;
            }

            // Raw HTML block `<tag...>` -> preserve verbatim.
            if is_html_block_start(line) {
                let start = idx;
                idx += 1;
                while idx < lines.len() && !lines[idx].trim().is_empty() {
                    idx += 1;
                }
                let raw = lines[start..idx].join("\n");
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Preserved { raw: raw.clone() },
                    raw,
                ));
                continue;
            }

            // Linked or formatted image `[![...` -> preserve verbatim so nested marks are never lost.
            if line.trim_start().starts_with("[![") {
                let raw = line.to_string();
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Preserved { raw: raw.clone() },
                    raw,
                ));
                idx += 1;
                continue;
            }

            // Fenced code block ```lang
            if let Some(rest) = line.trim_start().strip_prefix("```") {
                let start = idx;
                let lang = rest.trim().to_string();
                idx += 1;
                let code_start = idx;
                while idx < lines.len() && !lines[idx].trim_start().starts_with("```") {
                    idx += 1;
                }
                let code_end = idx;
                if idx < lines.len() {
                    idx += 1;
                }
                let raw = lines[start..idx].join("\n");
                let code = lines[code_start..code_end].join("\n");
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::CodeBlock { lang, code },
                    raw,
                ));
                continue;
            }

            // Headings # .. ######
            if let Some((level, text)) = parse_heading(line) {
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Heading {
                        level,
                        text: text.to_string(),
                    },
                    line.to_string(),
                ));
                idx += 1;
                continue;
            }

            // Horizontal rule `---` or `***`
            if is_divider(line) {
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Divider,
                    line.to_string(),
                ));
                idx += 1;
                continue;
            }

            // Table block (`| ... |` followed by separator row)
            if idx + 1 < lines.len() && looks_like_table_row(line) && is_table_separator(lines[idx + 1]) {
                let start = idx;
                idx += 2;
                while idx < lines.len() && looks_like_table_row(lines[idx]) {
                    idx += 1;
                }
                let raw = lines[start..idx].join("\n");
                if let Some((headers, rows)) = parse_table_lines(&lines[start..idx]) {
                    blocks.push(RichBlock::with_raw(
                        RichBlockKind::Table { headers, rows },
                        raw,
                    ));
                    continue;
                }
            }

            // Task list item `- [ ] ...` / `- [x] ...`
            if let Some((checked, text)) = parse_task(line) {
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Task {
                        checked,
                        text: text.to_string(),
                    },
                    line.to_string(),
                ));
                idx += 1;
                continue;
            }

            // Unordered list item `- ...` / `* ...`
            if let Some(text) = parse_bullet(line) {
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Bullet {
                        text: text.to_string(),
                    },
                    line.to_string(),
                ));
                idx += 1;
                continue;
            }

            // Ordered list item `1. ...`
            if let Some((number, text)) = parse_ordered(line) {
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Ordered {
                        number,
                        text: text.to_string(),
                    },
                    line.to_string(),
                ));
                idx += 1;
                continue;
            }

            // Blockquote `> ...`
            if line.trim_start().starts_with('>') {
                let start = idx;
                let mut quote_lines = Vec::new();
                while idx < lines.len() && lines[idx].trim_start().starts_with('>') {
                    let trimmed = lines[idx].trim_start();
                    let stripped = trimmed
                        .strip_prefix("> ")
                        .or_else(|| trimmed.strip_prefix('>'))
                        .unwrap_or("");
                    quote_lines.push(stripped);
                    idx += 1;
                }
                let raw = lines[start..idx].join("\n");
                blocks.push(RichBlock::with_raw(
                    RichBlockKind::Quote {
                        text: quote_lines.join("\n"),
                    },
                    raw,
                ));
                continue;
            }

            // Paragraph until blank line or next block construct
            let start = idx;
            idx += 1;
            while idx < lines.len()
                && !lines[idx].trim().is_empty()
                && !is_block_boundary(lines[idx])
            {
                idx += 1;
            }
            let raw = lines[start..idx].join("\n");
            blocks.push(RichBlock::with_raw(
                RichBlockKind::Paragraph { text: raw.clone() },
                raw,
            ));
        }

        if blocks.is_empty() {
            blocks.push(RichBlock::from_kind(RichBlockKind::Paragraph {
                text: String::new(),
            }));
        }

        Self { blocks }
    }

    pub(crate) fn to_markdown(&self) -> String {
        let mut out = String::new();
        for (index, block) in self.blocks.iter().enumerate() {
            if index > 0 {
                let prev_list = is_list_block(&self.blocks[index - 1].kind);
                let cur_list = is_list_block(&block.kind);
                if prev_list && cur_list {
                    out.push('\n');
                } else {
                    out.push_str("\n\n");
                }
            }
            out.push_str(&block.to_markdown());
        }
        out
    }

    pub(crate) fn toggle_task(&mut self, index: usize) -> bool {
        if let Some(block) = self.blocks.get_mut(index) {
            if let RichBlockKind::Task { checked, .. } = &mut block.kind {
                *checked = !*checked;
                block.raw_source = None;
                return true;
            }
        }
        false
    }

    pub(crate) fn set_heading(&mut self, index: usize, level: u8) {
        let Some(block) = self.blocks.get_mut(index) else {
            return;
        };
        if matches!(block.kind, RichBlockKind::Preserved { .. } | RichBlockKind::Divider) {
            return;
        }
        let text = match &block.kind {
            RichBlockKind::Heading { level: cur, text } if *cur == level => {
                block.kind = RichBlockKind::Paragraph { text: text.clone() };
                block.raw_source = None;
                return;
            }
            _ => block.editable_text().unwrap_or_default(),
        };
        block.kind = RichBlockKind::Heading {
            level: level.clamp(1, 6),
            text,
        };
        block.raw_source = None;
    }

    pub(crate) fn convert_block_to(&mut self, index: usize, target: &str) {
        let Some(block) = self.blocks.get_mut(index) else {
            return;
        };
        if matches!(block.kind, RichBlockKind::Preserved { .. }) {
            return;
        }
        let text = block.editable_text().unwrap_or_default();
        block.raw_source = None;
        block.kind = match target {
            "task" => RichBlockKind::Task {
                checked: false,
                text,
            },
            "bullet" => RichBlockKind::Bullet { text },
            "ordered" => RichBlockKind::Ordered { number: 1, text },
            "quote" => RichBlockKind::Quote { text },
            "code" => RichBlockKind::CodeBlock {
                lang: String::new(),
                code: text,
            },
            "paragraph" => RichBlockKind::Paragraph { text },
            _ => return,
        };
    }

    pub(crate) fn wrap_inline(&mut self, index: usize, prefix: &str, suffix: &str) {
        let Some(block) = self.blocks.get_mut(index) else {
            return;
        };
        let Some(text) = block.editable_text() else {
            return;
        };
        let updated = if text.starts_with(prefix) && text.ends_with(suffix) && text.len() >= prefix.len() + suffix.len() {
            text[prefix.len()..text.len() - suffix.len()].to_string()
        } else if text.trim().is_empty() {
            format!("{prefix}文本{suffix}")
        } else {
            format!("{prefix}{text}{suffix}")
        };
        block.apply_edited_text(updated);
    }

    pub(crate) fn insert_block_after(&mut self, index: usize, kind: RichBlockKind) -> usize {
        let insert_at = (index + 1).min(self.blocks.len());
        self.blocks.insert(insert_at, RichBlock::from_kind(kind));
        insert_at
    }

    pub(crate) fn add_table_row(&mut self, index: usize) {
        if let Some(block) = self.blocks.get_mut(index) {
            if let RichBlockKind::Table { headers, rows } = &mut block.kind {
                rows.push(vec!["内容".into(); headers.len().max(1)]);
                block.raw_source = None;
            }
        }
    }

    pub(crate) fn add_table_column(&mut self, index: usize) {
        if let Some(block) = self.blocks.get_mut(index) {
            if let RichBlockKind::Table { headers, rows } = &mut block.kind {
                headers.push(format!("列{}", headers.len() + 1));
                for row in rows {
                    row.push(String::new());
                }
                block.raw_source = None;
            }
        }
    }

    pub(crate) fn move_block(&mut self, index: usize, delta: isize) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }
        let target = (index as isize + delta).clamp(0, (self.blocks.len() - 1) as isize) as usize;
        if target != index && index < self.blocks.len() {
            let block = self.blocks.remove(index);
            self.blocks.insert(target, block);
        }
        target
    }

    pub(crate) fn remove_block(&mut self, index: usize) -> usize {
        if self.blocks.len() <= 1 {
            if let Some(first) = self.blocks.first_mut() {
                first.kind = RichBlockKind::Paragraph {
                    text: String::new(),
                };
                first.raw_source = None;
            }
            return 0;
        }
        if index < self.blocks.len() {
            self.blocks.remove(index);
        }
        index.min(self.blocks.len().saturating_sub(1))
    }
}

fn is_list_block(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Task { .. } | RichBlockKind::Bullet { .. } | RichBlockKind::Ordered { .. }
    )
}

fn is_ref_definition(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('[') && trimmed.contains("]:")
}

fn is_html_block_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('<')
        && trimmed.ends_with('>')
        && !trimmed.starts_with("<!--")
        && trimmed
            .chars()
            .nth(1)
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '/')
}

fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = trimmed[level..].strip_prefix(' ')?;
    Some((level as u8, rest))
}

fn is_divider(line: &str) -> bool {
    let trimmed = line.trim();
    (trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '-'))
        || (trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '*'))
}

fn parse_task(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("- [ ] ")
        .map(|text| (false, text))
        .or_else(|| trimmed.strip_prefix("- [x] ").map(|text| (true, text)))
        .or_else(|| trimmed.strip_prefix("- [X] ").map(|text| (true, text)));
    rest
}

fn parse_bullet(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if parse_task(line).is_some() {
        return None;
    }
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
}

fn parse_ordered(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let digits_len = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_len == 0 {
        return None;
    }
    let rest = trimmed[digits_len..].strip_prefix(". ")?;
    let num = trimmed[..digits_len].parse().ok()?;
    Some((num, rest))
}

fn looks_like_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2
}

fn is_table_separator(line: &str) -> bool {
    if !looks_like_table_row(line) {
        return false;
    }
    let cells = split_table_cells(line);
    !cells.is_empty()
        && cells
            .iter()
            .all(|cell| cell.contains('-') && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn split_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
    trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn parse_table_lines(lines: &[&str]) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    if lines.len() < 2 || !is_table_separator(lines[1]) {
        return None;
    }
    let headers = split_table_cells(lines[0]);
    if headers.is_empty() {
        return None;
    }
    let col_count = headers.len();
    let rows = lines[2..]
        .iter()
        .map(|line| {
            let mut cells = split_table_cells(line);
            cells.resize(col_count, String::new());
            cells
        })
        .collect();
    Some((headers, rows))
}

fn is_block_boundary(line: &str) -> bool {
    line.trim_start().starts_with("```")
        || parse_heading(line).is_some()
        || is_divider(line)
        || parse_task(line).is_some()
        || parse_bullet(line).is_some()
        || parse_ordered(line).is_some()
        || line.trim_start().starts_with('>')
        || is_ref_definition(line)
        || is_html_block_start(line)
        || line.trim_start().starts_with("[![")
        || looks_like_table_row(line)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JournalEditorMode {
    SourceSplit,
    RichBlocks,
}

pub(crate) struct RichEditorState {
    pub(crate) mode: JournalEditorMode,
    pub(crate) doc: RichDocument,
    pub(crate) active_block: usize,
    pub(crate) block_editor: Entity<EditorState>,
    syncing_from_block: bool,
    _sub: Subscription,
}

impl RichEditorState {
    pub(crate) fn new(
        initial_markdown: &str,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) -> Self {
        let doc = RichDocument::parse(initial_markdown);
        let first_text = doc
            .blocks
            .first()
            .and_then(|block| block.editable_text())
            .unwrap_or_default();
        let block_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .line_number(false)
                .default_value(first_text)
        });
        let sub = cx.subscribe_in(&block_editor, window, |this, entity, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.apply_active_rich_block_change(entity.read(cx).value().to_string(), window, cx);
            }
        });
        Self {
            mode: JournalEditorMode::SourceSplit,
            doc,
            active_block: 0,
            block_editor,
            syncing_from_block: false,
            _sub: sub,
        }
    }

    pub(crate) fn sync_from_source(
        &mut self,
        markdown: &str,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) {
        if self.syncing_from_block {
            return;
        }
        self.doc = RichDocument::parse(markdown);
        self.active_block = self
            .active_block
            .min(self.doc.blocks.len().saturating_sub(1));
        self.load_active_block_into_editor(window, cx);
    }

    fn load_active_block_into_editor(&mut self, window: &mut Window, cx: &mut Context<Prototype>) {
        let text = self
            .doc
            .blocks
            .get(self.active_block)
            .and_then(|block| block.editable_text())
            .unwrap_or_default();
        self.syncing_from_block = true;
        self.block_editor
            .update(cx, |editor, cx| editor.set_value(text, window, cx));
        self.syncing_from_block = false;
    }
}

impl Prototype {
    pub(crate) fn switch_journal_editor_mode(
        &mut self,
        mode: JournalEditorMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.rich.mode == mode {
            return;
        }
        if mode == JournalEditorMode::RichBlocks {
            let source = edit.editor.read(cx).value().to_string();
            edit.rich.sync_from_source(&source, window, cx);
            edit.rich.mode = JournalEditorMode::RichBlocks;
        } else {
            edit.rich.mode = JournalEditorMode::SourceSplit;
            edit.editor.update(cx, |editor, cx| editor.focus(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn select_rich_block(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if index < edit.rich.doc.blocks.len() {
            edit.rich.active_block = index;
            edit.rich.load_active_block_into_editor(window, cx);
            edit.rich
                .block_editor
                .update(cx, |editor, cx| editor.focus(window, cx));
            cx.notify();
        }
    }

    pub(crate) fn apply_active_rich_block_change(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.rich.syncing_from_block || edit.rich.mode != JournalEditorMode::RichBlocks {
            return;
        }
        let idx = edit.rich.active_block;
        let Some(block) = edit.rich.doc.blocks.get_mut(idx) else {
            return;
        };
        block.apply_edited_text(value);
        let markdown = edit.rich.doc.to_markdown();
        edit.rich.syncing_from_block = true;
        edit.editor
            .update(cx, |editor, cx| editor.set_value(markdown, window, cx));
        edit.rich.syncing_from_block = false;
        cx.notify();
    }

    pub(crate) fn toggle_rich_task(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if !edit.rich.doc.toggle_task(index) {
            return;
        }
        let markdown = edit.rich.doc.to_markdown();
        edit.rich.syncing_from_block = true;
        edit.editor
            .update(cx, |editor, cx| editor.set_value(markdown, window, cx));
        edit.rich.syncing_from_block = false;
        cx.notify();
    }

    pub(crate) fn mutate_rich_doc(
        &mut self,
        mutate: impl FnOnce(&mut RichDocument, usize) -> usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.rich.mode != JournalEditorMode::RichBlocks {
            let source = edit.editor.read(cx).value().to_string();
            edit.rich.sync_from_source(&source, window, cx);
        }
        let active = edit.rich.active_block;
        let next_active = mutate(&mut edit.rich.doc, active);
        edit.rich.active_block = next_active.min(edit.rich.doc.blocks.len().saturating_sub(1));
        edit.rich.load_active_block_into_editor(window, cx);
        let markdown = edit.rich.doc.to_markdown();
        edit.rich.syncing_from_block = true;
        edit.editor
            .update(cx, |editor, cx| editor.set_value(markdown, window, cx));
        edit.rich.syncing_from_block = false;
        cx.notify();
    }
}

#[inline(never)]
pub(crate) fn render_rich_toolbar(
    rich: &RichEditorState,
    disabled: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    use crate::icons::{PhIcon, ph_icon};

    let palette = crate::theme_skin::active_palette(cx);
    let fg = palette.text_muted;
    let is_table = rich
        .doc
        .blocks
        .get(rich.active_block)
        .is_some_and(|block| matches!(block.kind, RichBlockKind::Table { .. }));

    let icon_btn = |id: &'static str, label: &'static str, icon: PhIcon, suffix: Option<&'static str>| {
        FlatButton::new(id)
            .accessibility_label(label)
            .disabled(disabled)
            .h(px(26.))
            .min_w(px(26.))
            .px_1()
            .rounded(px(5.))
            .hover(move |s| s.bg(palette.tab_active()))
            .flex()
            .items_center()
            .justify_center()
            .gap_0p5()
            .child(ph_icon(icon, px(16.), fg))
            .when_some(suffix, |b, s| {
                b.child(div().text_size(px(9.5)).text_color(fg).child(s))
            })
    };

    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_0p5()
        .child(
            icon_btn("rich-tb-undo", "撤销", PhIcon::ArrowCounterClockwise, None).on_click(
                cx.listener(|this, _, window, cx| {
                    if let Some(edit) = &this.workspace_edit {
                        edit.editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-redo", "重做", PhIcon::ArrowClockwise, None).on_click(cx.listener(
                |this, _, window, cx| {
                    if let Some(edit) = &this.workspace_edit {
                        edit.editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                },
            )),
        )
        .child(
            icon_btn(
                "rich-tb-h1",
                "标题与正文",
                PhIcon::TextH,
                if cfg!(test) { Some("1") } else { None },
            )
            .on_click(cx.listener(|this, _, window, cx| {
                #[cfg(test)]
                this.mutate_rich_doc(
                    |doc, idx| {
                        doc.set_heading(idx, 1);
                        idx
                    },
                    window,
                    cx,
                );
                #[cfg(not(test))]
                {
                    let _ = window;
                    if let Some(edit) = &mut this.workspace_edit {
                        edit.toggle_popover(crate::workspace_edit::EditorPopover::HeadingMenu);
                        cx.notify();
                    }
                }
            })),
        )
        .when(cfg!(test), |bar| {
            bar.child(
                icon_btn("rich-tb-h2", "二级标题", PhIcon::TextH, Some("2")).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.mutate_rich_doc(
                            |doc, idx| {
                                doc.set_heading(idx, 2);
                                idx
                            },
                            window,
                            cx,
                        );
                    },
                )),
            )
            .child(
                icon_btn("rich-tb-h3", "三级标题", PhIcon::TextH, Some("3")).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.mutate_rich_doc(
                            |doc, idx| {
                                doc.set_heading(idx, 3);
                                idx
                            },
                            window,
                            cx,
                        );
                    },
                )),
            )
        })
        .child(
            icon_btn("rich-tb-bold", "加粗", PhIcon::TextB, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "**", "**");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-bullet", "插入列表", PhIcon::ListBullets, None).on_click(
                cx.listener(|this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.convert_block_to(idx, "bullet");
                            idx
                        },
                        window,
                        cx,
                    );
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-task", "插入待办", PhIcon::CheckSquare, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.convert_block_to(idx, "task");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-italic", "斜体", PhIcon::TextItalic, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "*", "*");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-strike", "删除线", PhIcon::TextStrikethrough, None).on_click(
                cx.listener(|this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "~~", "~~");
                            idx
                        },
                        window,
                        cx,
                    );
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-ordered", "有序列表", PhIcon::ListNumbers, None).on_click(
                cx.listener(|this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.convert_block_to(idx, "ordered");
                            idx
                        },
                        window,
                        cx,
                    );
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-quote", "引用", PhIcon::Quotes, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.convert_block_to(idx, "quote");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-code", "行内代码", PhIcon::Code, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "`", "`");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-codeblock", "代码块", PhIcon::CodeBlock, None).on_click(
                cx.listener(|this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.insert_block_after(
                                idx,
                                RichBlockKind::CodeBlock {
                                    lang: String::new(),
                                    code: String::new(),
                                },
                            )
                        },
                        window,
                        cx,
                    );
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-link", "链接", PhIcon::Link, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "[", "](https://)");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-add-para", "分隔线", PhIcon::Minus, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.insert_block_after(
                                idx,
                                RichBlockKind::Paragraph {
                                    text: String::new(),
                                },
                            )
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .child(
            icon_btn("rich-tb-table", "表格", PhIcon::Table, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.insert_block_after(
                                idx,
                                RichBlockKind::Table {
                                    headers: vec!["事项".into(), "进展".into()],
                                    rows: vec![vec!["".into(), "".into()]],
                                },
                            )
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .when(is_table, |bar| {
            bar.child(
                FlatButton::new("rich-tb-add-row")
                    .disabled(disabled)
                    .h(px(24.))
                    .px_1p5()
                    .rounded(px(5.))
                    .hover(move |s| s.bg(palette.tab_active()))
                    .text_size(px(11.))
                    .text_color(fg)
                    .flex()
                    .items_center()
                    .child("+行")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.mutate_rich_doc(
                            |doc, idx| {
                                doc.add_table_row(idx);
                                idx
                            },
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                FlatButton::new("rich-tb-add-col")
                    .disabled(disabled)
                    .h(px(24.))
                    .px_1p5()
                    .rounded(px(5.))
                    .hover(move |s| s.bg(palette.tab_active()))
                    .text_size(px(11.))
                    .text_color(fg)
                    .flex()
                    .items_center()
                    .child("+列")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.mutate_rich_doc(
                            |doc, idx| {
                                doc.add_table_column(idx);
                                idx
                            },
                            window,
                            cx,
                        );
                    })),
            )
        })
        .child(
            icon_btn("rich-tb-indent", "增加缩进", PhIcon::TextIndent, None).on_click(
                cx.listener(|this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.wrap_inline(idx, "  ", "");
                            idx
                        },
                        window,
                        cx,
                    );
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-outdent", "减少缩进", PhIcon::TextOutdent, None).on_click(
                cx.listener(|this, _, window, cx| {
                    if let Some(edit) = &this.workspace_edit {
                        edit.editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                }),
            ),
        )
        .child(
            icon_btn("rich-tb-clear", "清除格式", PhIcon::Eraser, None).on_click(cx.listener(
                |this, _, window, cx| {
                    this.mutate_rich_doc(
                        |doc, idx| {
                            doc.convert_block_to(idx, "paragraph");
                            idx
                        },
                        window,
                        cx,
                    );
                },
            )),
        )
        .when(rich.mode == JournalEditorMode::RichBlocks, |bar| {
            bar.child(
                FlatButton::new("rich-tb-up")
                    .disabled(disabled)
                    .w(px(24.))
                    .h(px(24.))
                    .rounded(px(5.))
                    .hover(move |s| s.bg(palette.tab_active()))
                    .text_size(px(11.5))
                    .text_color(fg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("↑")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.mutate_rich_doc(|doc, idx| doc.move_block(idx, -1), window, cx);
                    })),
            )
            .child(
                FlatButton::new("rich-tb-down")
                    .disabled(disabled)
                    .w(px(24.))
                    .h(px(24.))
                    .rounded(px(5.))
                    .hover(move |s| s.bg(palette.tab_active()))
                    .text_size(px(11.5))
                    .text_color(fg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("↓")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.mutate_rich_doc(|doc, idx| doc.move_block(idx, 1), window, cx);
                    })),
            )
            .child(
                icon_btn("rich-tb-delete", "删除块", PhIcon::Trash, None).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.mutate_rich_doc(|doc, idx| doc.remove_block(idx), window, cx);
                    },
                )),
            )
        })
        .into_any_element()
}

#[inline(never)]
pub(crate) fn render_rich_editor_body(
    rich: &RichEditorState,
    disabled: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let secondary = theme.secondary;

    let active_idx = rich.active_block;
    let active_editable = rich
        .doc
        .blocks
        .get(active_idx)
        .and_then(|block| block.editable_text())
        .is_some();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .id("rich-blocks-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .border_1()
                .border_color(border)
                .rounded_md()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .children(rich.doc.blocks.iter().enumerate().map(|(index, block)| {
                    let is_active = index == active_idx;
                    let card_border = if is_active { accent } else { border };
                    let card = div()
                        .id(format!("rich-block-card-{index}"))
                        .border_1()
                        .border_color(card_border)
                        .when(is_active, |el| el.bg(secondary))
                        .rounded_sm()
                        .p_2()
                        .flex()
                        .items_start()
                        .gap_2();

                    let body: gpui_kit::AnyElement = match &block.kind {
                        RichBlockKind::Heading { level, text } => {
                            let size = match level {
                                1 => px(20.),
                                2 => px(17.),
                                3 => px(15.),
                                _ => px(14.),
                            };
                            div()
                                .flex_1()
                                .text_size(size)
                                .child(SharedString::from(if text.is_empty() {
                                    "（空标题）".to_string()
                                } else {
                                    text.clone()
                                }))
                                .into_any_element()
                        }
                        RichBlockKind::Task { checked, text } => {
                            let check_mark = if *checked { "☑" } else { "☐" };
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new(format!("rich-task-toggle-{index}"))
                                        .label(check_mark)
                                        .disabled(disabled)
                                        .px_1()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.toggle_rich_task(index, window, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(13.))
                                        .child(SharedString::from(if text.is_empty() {
                                            "待办事项".to_string()
                                        } else {
                                            text.clone()
                                        })),
                                )
                                .into_any_element()
                        }
                        RichBlockKind::Bullet { text } => div()
                            .flex_1()
                            .flex()
                            .gap_2()
                            .child(div().text_color(accent).child("•"))
                            .child(div().flex_1().text_size(px(13.)).child(text.clone()))
                            .into_any_element(),
                        RichBlockKind::Ordered { number, text } => div()
                            .flex_1()
                            .flex()
                            .gap_2()
                            .child(div().text_color(accent).child(format!("{number}.")))
                            .child(div().flex_1().text_size(px(13.)).child(text.clone()))
                            .into_any_element(),
                        RichBlockKind::Quote { text } => div()
                            .flex_1()
                            .border_l_2()
                            .border_color(accent)
                            .pl_2()
                            .text_size(px(13.))
                            .text_color(muted)
                            .child(text.clone())
                            .into_any_element(),
                        RichBlockKind::CodeBlock { lang, code } => div()
                            .flex_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(border)
                            .p_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when(!lang.is_empty(), |box_el| {
                                box_el.child(div().text_size(px(10.)).text_color(muted).child(lang.clone()))
                            })
                            .child(div().text_size(px(12.)).child(code.clone()))
                            .into_any_element(),
                        RichBlockKind::Table { headers, rows } => div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .text_size(px(12.))
                                    .children(headers.iter().map(|h| {
                                        div().flex_1().border_b_1().border_color(border).child(h.clone())
                                    })),
                            )
                            .children(rows.iter().map(|row| {
                                div()
                                    .flex()
                                    .gap_2()
                                    .text_size(px(12.))
                                    .text_color(muted)
                                    .children(row.iter().map(|cell| {
                                        div().flex_1().child(if cell.is_empty() {
                                            "—".to_string()
                                        } else {
                                            cell.clone()
                                        })
                                    }))
                            }))
                            .into_any_element(),
                        RichBlockKind::Divider => div()
                            .flex_1()
                            .border_t_1()
                            .border_color(border)
                            .my_2()
                            .into_any_element(),
                        RichBlockKind::Paragraph { text } => div()
                            .flex_1()
                            .text_size(px(13.))
                            .child(SharedString::from(if text.is_empty() {
                                "（点击编辑当前段落内容）".to_string()
                            } else {
                                text.clone()
                            }))
                            .into_any_element(),
                        RichBlockKind::Preserved { raw } => div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("[原文块保留] {}", raw.lines().next().unwrap_or("")))
                            .into_any_element(),
                    };

                    card.child(
                        Button::new(format!("rich-block-{index}"))
                            .label(block.kind_label())
                            .when(is_active, Button::primary)
                            .disabled(disabled)
                            .px_1()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_rich_block(index, window, cx);
                            })),
                    )
                    .child(body)
                })),
        )
        .when(active_editable, |pane| {
            pane.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(format!("正在编辑第 {} 块（实时回写 Markdown 源码）", active_idx + 1)),
                    )
                    .child(
                        Editor::new(&rich.block_editor)
                            .aria_label("当前块编辑")
                            .disabled(disabled)
                            .h(px(88.))
                            .w_full(),
                    ),
            )
        })
        .into_any_element()
}
