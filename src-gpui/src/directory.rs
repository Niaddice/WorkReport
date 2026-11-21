use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use gpui_kit::{
    ClipboardItem, Context, Entity, FontWeight, MouseButton, SharedString, Subscription, Task, Window,
    base::{Button as FlatButton, Disableable, SelectableText},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, Input, InputEvent, InputState},
    },
    div, prelude::*, px,
};
use workreport_core::{
    directory_ops::{Action as DirAction, Plan as DirPlan, Request as DirRequest, TrashEntry},
    excel::ExcelSheetPreview,
    workspace::{FileNode, SearchResultItem},
    workspace_document,
    workspace_preview::{
        apply_directory_change_at, list_workspace_trash_at, preview_directory_change_at,
        save_document_at,
    },
};

use crate::{Prototype, markdown_preview, workspace_view::WorkspaceView};

const SEARCH_LIMIT: usize = 50;

#[derive(Clone, Debug)]
pub(crate) struct TreeNode {
    pub(crate) node: FileNode,
    pub(crate) children: Vec<TreeNode>,
}

#[derive(Clone, Debug)]
pub(crate) struct VisibleRow {
    pub(crate) node: FileNode,
    pub(crate) depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DirectoryFormKind {
    CreateFile,
    CreateDir,
    Rename,
}

#[derive(Clone, Debug)]
pub(crate) struct DirectoryForm {
    pub(crate) kind: DirectoryFormKind,
    pub(crate) parent: String,
    pub(crate) target_path: String,
    pub(crate) is_journal: bool,
    pub(crate) input: Entity<InputState>,
    pub(crate) error: Option<String>,
    pub(crate) busy: bool,
}

pub(crate) struct DeleteConfirmation {
    pub(crate) request: DirRequest,
    pub(crate) plan: DirPlan,
    pub(crate) busy: bool,
}

pub(crate) struct WorkspaceDocumentState {
    pub(crate) doc: workspace_document::Document,
    pub(crate) editor: Option<Entity<EditorState>>,
    pub(crate) _subscription: Option<Subscription>,
    pub(crate) draft: String,
    pub(crate) dirty: bool,
    pub(crate) saving: bool,
    pub(crate) confirm_discard: bool,
    pub(crate) error: Option<String>,
    pub(crate) active_sheet: usize,
}

impl WorkspaceDocumentState {
    pub(crate) fn from_document(
        doc: workspace_document::Document,
        window: &mut Window,
        cx: &mut Context<Prototype>,
    ) -> Self {
        let is_text = doc.kind == "text";
        let draft = doc.content.clone();
        let (editor, subscription) = if is_text {
            let initial = draft.clone();
            let state = cx.new(|cx| {
                EditorState::new(window, cx)
                    .soft_wrap(true)
                    .line_number(true)
                    .default_value(initial)
            });
            let sub = cx.subscribe_in(&state, window, |this, entity, event, _, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if let Some(open_doc) = &mut this.directory.open_doc {
                    let current = entity.read(cx).value().to_string();
                    open_doc.dirty = current != open_doc.doc.content;
                    open_doc.draft = current;
                    open_doc.confirm_discard = false;
                    open_doc.error = None;
                    cx.notify();
                }
            });
            (Some(state), Some(sub))
        } else {
            (None, None)
        };
        Self {
            doc,
            editor,
            _subscription: subscription,
            draft,
            dirty: false,
            saving: false,
            confirm_discard: false,
            error: None,
            active_sheet: 0,
        }
    }
}

pub(crate) struct DirectoryState {
    pub(crate) nodes: Vec<FileNode>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) collapsed: BTreeSet<String>,
    pub(crate) selected_path: Option<String>,
    pub(crate) form: Option<DirectoryForm>,
    pub(crate) delete_confirm: Option<DeleteConfirmation>,
    pub(crate) trash_open: bool,
    pub(crate) trash_entries: Vec<TrashEntry>,
    pub(crate) trash_loading: bool,
    pub(crate) trash_error: Option<String>,
    pub(crate) search_input: Entity<InputState>,
    pub(crate) search_query: String,
    pub(crate) search_results: Vec<SearchResultItem>,
    pub(crate) search_loading: bool,
    pub(crate) search_error: Option<String>,
    pub(crate) search_dialog_open: bool,
    pub(crate) open_doc: Option<WorkspaceDocumentState>,
    tree_key: Option<(usize, usize, u64)>,
    tree_revision: u64,
    load_request: u64,
    load_task: Option<Task<()>>,
    search_request: u64,
    search_task: Option<Task<()>>,
    doc_request: u64,
    doc_task: Option<Task<()>>,
    _search_subscription: Subscription,
}

impl DirectoryState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Prototype>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx));
        let search_sub = cx.subscribe_in(&search_input, window, |this, entity, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                let query = entity.read(cx).value().to_string();
                this.run_workspace_search(query, window, cx);
            }
        });
        Self {
            nodes: Vec::new(),
            loading: false,
            error: None,
            collapsed: BTreeSet::new(),
            selected_path: None,
            form: None,
            delete_confirm: None,
            trash_open: false,
            trash_entries: Vec::new(),
            trash_loading: false,
            trash_error: None,
            search_input,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_error: None,
            search_dialog_open: false,
            open_doc: None,
            tree_key: None,
            tree_revision: 0,
            load_request: 0,
            load_task: None,
            search_request: 0,
            search_task: None,
            doc_request: 0,
            doc_task: None,
            _search_subscription: search_sub,
        }
    }

    pub(crate) fn reset(&mut self, window: &mut Window, cx: &mut Context<Prototype>) {
        self.nodes.clear();
        self.loading = false;
        self.error = None;
        self.collapsed.clear();
        self.selected_path = None;
        self.form = None;
        self.delete_confirm = None;
        self.trash_open = false;
        self.trash_entries.clear();
        self.trash_loading = false;
        self.trash_error = None;
        self.search_query.clear();
        self.search_results.clear();
        self.search_loading = false;
        self.search_error = None;
        self.search_dialog_open = false;
        self.open_doc = None;
        self.tree_key = None;
        self.tree_revision += 1;
        self.load_request += 1;
        self.load_task = None;
        self.search_request += 1;
        self.search_task = None;
        self.doc_request += 1;
        self.doc_task = None;
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }

    pub(crate) fn invalidate_tree(&mut self) {
        self.tree_key = None;
        self.tree_revision += 1;
    }

    pub(crate) fn has_dirty_document(&self) -> bool {
        self.open_doc.as_ref().is_some_and(|doc| doc.dirty)
    }

    pub(crate) fn close_clean_document(&mut self) -> bool {
        if self.has_dirty_document() {
            return false;
        }
        self.open_doc = None;
        true
    }
}

pub(crate) fn build_tree(nodes: &[FileNode]) -> Vec<TreeNode> {
    let mut children_by_parent: BTreeMap<String, Vec<FileNode>> = BTreeMap::new();
    for node in nodes {
        let parent = parent_path(&node.path).to_string();
        children_by_parent
            .entry(parent)
            .or_default()
            .push(node.clone());
    }
    build_subtree("", &mut children_by_parent)
}

fn build_subtree(
    parent: &str,
    children_by_parent: &mut BTreeMap<String, Vec<FileNode>>,
) -> Vec<TreeNode> {
    let mut list = children_by_parent.remove(parent).unwrap_or_default();
    list.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    list.into_iter()
        .map(|node| {
            let children = if node.is_dir {
                build_subtree(&node.path, children_by_parent)
            } else {
                Vec::new()
            };
            TreeNode { node, children }
        })
        .collect()
}

pub(crate) fn visible_rows(tree: &[TreeNode], collapsed: &BTreeSet<String>) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    collect_visible(tree, collapsed, 0, &mut rows);
    rows
}

fn collect_visible(
    tree: &[TreeNode],
    collapsed: &BTreeSet<String>,
    depth: usize,
    rows: &mut Vec<VisibleRow>,
) {
    for item in tree {
        let is_dir = item.node.is_dir;
        let path = item.node.path.clone();
        rows.push(VisibleRow {
            node: item.node.clone(),
            depth,
        });
        if is_dir && !collapsed.contains(&path) {
            collect_visible(&item.children, collapsed, depth + 1, rows);
        }
    }
}

pub(crate) fn parent_path(path: &str) -> &str {
    match path.rfind('/') {
        Some(idx) => &path[..idx],
        None => "",
    }
}

pub(crate) fn validate_node_name(
    raw: &str,
    parent: &str,
    existing: &[FileNode],
    current_path: Option<&str>,
    is_journal: bool,
) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("请输入名称".into());
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err("名称不能包含路径分隔符或 ..".into());
    }
    if name.starts_with('.') || name == "workreport.json" {
        return Err("不能使用隐藏文件名或设置文件名".into());
    }
    if is_journal && !name.ends_with(".md") && !name.ends_with(".txt") {
        return Err("日报名称需保留 .md 或 .txt 扩展名".into());
    }
    let next_path = if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    };
    if existing
        .iter()
        .any(|node| node.path == next_path && current_path != Some(node.path.as_str()))
    {
        return Err("同名文件或目录已存在，请使用其他名称".into());
    }
    Ok(name.to_string())
}

fn is_table_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".xlsx", ".xlsm", ".xls", ".xlsb", ".csv", ".tsv"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

impl Prototype {
    pub(crate) fn ensure_directory_tree(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (
            Arc::as_ptr(&workspace.reader) as usize,
            workspace.entries.len(),
            self.directory.tree_revision,
        );
        if self.directory.tree_key.as_ref() == Some(&key) {
            return;
        }
        self.directory.tree_key = Some(key);
        self.directory.load_request += 1;
        let request_id = self.directory.load_request;
        self.directory.loading = true;
        self.directory.error = None;
        let reader = workspace.reader.clone();
        let load = cx
            .background_executor()
            .spawn(async move { reader.list_workspace_files() });
        self.directory.load_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.directory.load_request != request_id {
                    return;
                }
                this.directory.loading = false;
                match result {
                    Ok(nodes) => {
                        this.directory.nodes = nodes;
                        this.directory.error = None;
                    }
                    Err(error) => {
                        this.directory.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn run_workspace_search(
        &mut self,
        query: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.directory.search_query = query.clone();
        self.directory.search_request += 1;
        let request_id = self.directory.search_request;
        self.directory.search_error = None;
        if query.trim().is_empty() {
            self.directory.search_loading = false;
            self.directory.search_results.clear();
            self.directory.search_task = None;
            cx.notify();
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        self.directory.search_loading = true;
        cx.notify();
        let reader = workspace.reader.clone();
        let search = cx
            .background_executor()
            .spawn(async move { reader.search(&query, SEARCH_LIMIT) });
        self.directory.search_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = search.await;
            let _ = this.update(cx, |this, cx| {
                if this.directory.search_request != request_id {
                    return;
                }
                this.directory.search_loading = false;
                match result {
                    Ok(items) => {
                        this.directory.search_results = items;
                        this.directory.search_error = None;
                    }
                    Err(error) => {
                        this.directory.search_error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn open_workspace_document(
        &mut self,
        relative: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy {
            return;
        }
        if self.directory.has_dirty_document() {
            if let Some(open_doc) = &mut self.directory.open_doc {
                open_doc.confirm_discard = true;
            }
            self.status = "当前文档有未保存修改，请先保存或放弃后再打开其他文件".into();
            cx.notify();
            return;
        }
        if !self.leave_journal_editor_if_needed(window, cx) {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        self.directory.selected_path = Some(relative.clone());
        self.directory.doc_request += 1;
        let request_id = self.directory.doc_request;
        let reader = workspace.reader.clone();
        self.status = format!("正在打开 {relative}…").into();
        cx.notify();
        let read = cx
            .background_executor()
            .spawn(async move { reader.read_document(&relative) });
        self.directory.doc_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.directory.doc_request != request_id {
                    return;
                }
                match result {
                    Ok(doc) => {
                        if let Some(date) = doc.journal_date.clone() {
                            if let Some(entry) = this
                                .workspace
                                .as_ref()
                                .and_then(|ws| ws.entries.iter().find(|e| e.date == date))
                                .cloned()
                            {
                                this.directory.open_doc = None;
                                this.open_journal(entry.date, entry.format, window, cx);
                                return;
                            }
                        }
                        let name = doc.filename.clone();
                        this.directory.open_doc =
                            Some(WorkspaceDocumentState::from_document(doc, window, cx));
                        this.module_layout.ensure_editor_visible();
                        this.status = format!("已打开工作空间文件：{name}").into();
                    }
                    Err(error) => {
                        this.status = format!("无法打开文件：{error}").into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn save_open_workspace_document(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(open_doc) = &mut self.directory.open_doc else {
            return;
        };
        if open_doc.doc.kind != "text" || open_doc.saving {
            return;
        }
        let content = open_doc
            .editor
            .as_ref()
            .map(|ed| ed.read(cx).value().to_string())
            .unwrap_or_else(|| open_doc.draft.clone());
        let expected_rev = open_doc.doc.revision.clone();
        let path = open_doc.doc.path.clone();
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        open_doc.saving = true;
        open_doc.error = None;
        open_doc.confirm_discard = false;
        cx.notify();

        let save = cx.background_executor().spawn(async move {
            save_document_at(&root, &expected_path, &path, &content, &expected_rev)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = save.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(open_doc) = &mut this.directory.open_doc else {
                    return;
                };
                open_doc.saving = false;
                match result {
                    Ok(saved) => {
                        let current_draft = open_doc
                            .editor
                            .as_ref()
                            .map(|ed| ed.read(cx).value().to_string())
                            .unwrap_or_else(|| open_doc.draft.clone());
                        open_doc.dirty = current_draft != saved.content;
                        open_doc.doc = saved;
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.status = "已保存工作空间文件".into();
                    }
                    Err(error) => {
                        open_doc.error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn start_directory_form(
        &mut self,
        kind: DirectoryFormKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self
            .directory
            .selected_path
            .as_deref()
            .and_then(|path| self.directory.nodes.iter().find(|n| n.path == path))
            .cloned();

        let (parent, target_path, initial_name, is_journal) = match kind {
            DirectoryFormKind::CreateFile => {
                let parent = match &selected {
                    Some(node) if node.is_dir => node.path.clone(),
                    Some(node) => parent_path(&node.path).to_string(),
                    None => String::new(),
                };
                (parent, String::new(), String::new(), false)
            }
            DirectoryFormKind::CreateDir => {
                let parent = match &selected {
                    Some(node) if node.is_dir => node.path.clone(),
                    Some(node) => parent_path(&node.path).to_string(),
                    None => String::new(),
                };
                (parent, String::new(), String::new(), false)
            }
            DirectoryFormKind::Rename => {
                let Some(node) = selected else {
                    self.status = "请先在目录中选择要重命名的项目".into();
                    cx.notify();
                    return;
                };
                if node.protected {
                    self.status = "受保护项目不能重命名".into();
                    cx.notify();
                    return;
                }
                (
                    parent_path(&node.path).to_string(),
                    node.path.clone(),
                    node.name.clone(),
                    node.journal_date.is_some(),
                )
            }
        };

        let input = cx.new(|cx| InputState::new(window, cx).default_value(initial_name));
        self.directory.delete_confirm = None;
        self.directory.form = Some(DirectoryForm {
            kind,
            parent,
            target_path,
            is_journal,
            input,
            error: None,
            busy: false,
        });
        cx.notify();
    }

    pub(crate) fn submit_directory_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(form) = &mut self.directory.form else {
            return;
        };
        if form.busy {
            return;
        }
        let raw_name = form.input.read(cx).value().to_string();
        let current = match form.kind {
            DirectoryFormKind::Rename => Some(form.target_path.as_str()),
            _ => None,
        };
        let name = match validate_node_name(
            &raw_name,
            &form.parent,
            &self.directory.nodes,
            current,
            form.is_journal,
        ) {
            Ok(name) => name,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return;
            }
        };

        let request = match form.kind {
            DirectoryFormKind::CreateFile => DirRequest {
                action: DirAction::CreateFile,
                path: form.parent.clone(),
                name: Some(name),
                trash_id: None,
            },
            DirectoryFormKind::CreateDir => DirRequest {
                action: DirAction::CreateDirectory,
                path: form.parent.clone(),
                name: Some(name),
                trash_id: None,
            },
            DirectoryFormKind::Rename => DirRequest {
                action: DirAction::Rename,
                path: form.target_path.clone(),
                name: Some(name),
                trash_id: None,
            },
        };

        form.busy = true;
        form.error = None;
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            let plan = preview_directory_change_at(&root, &expected_path, &request)?;
            apply_directory_change_at(&root, &expected_path, &request, &plan.token)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(changed) => {
                        this.directory.form = None;
                        if !changed.path.is_empty() {
                            this.directory.selected_path = Some(changed.path.clone());
                        }
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.refresh_workspace_entries(window, cx);
                        this.status = "目录变更已应用".into();
                    }
                    Err(error) => {
                        if let Some(form) = &mut this.directory.form {
                            form.busy = false;
                            form.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn request_delete_selected(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(path) = self.directory.selected_path.clone() else {
            self.status = "请先在目录中选择要删除的文件或文件夹".into();
            cx.notify();
            return;
        };
        let request = DirRequest {
            action: DirAction::Delete,
            path,
            name: None,
            trash_id: None,
        };
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        let req_copy = request.clone();
        let preview = cx
            .background_executor()
            .spawn(async move { preview_directory_change_at(&root, &expected_path, &req_copy) });
        cx.spawn_in(window, async move |this, cx| {
            let result = preview.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(plan) => {
                        this.directory.form = None;
                        this.directory.delete_confirm = Some(DeleteConfirmation {
                            request,
                            plan,
                            busy: false,
                        });
                    }
                    Err(error) => {
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn confirm_delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(confirm) = &mut self.directory.delete_confirm else {
            return;
        };
        if confirm.busy {
            return;
        }
        confirm.busy = true;
        let request = confirm.request.clone();
        let token = confirm.plan.token.clone();
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        cx.notify();
        let apply = cx
            .background_executor()
            .spawn(async move { apply_directory_change_at(&root, &expected_path, &request, &token) });
        cx.spawn_in(window, async move |this, cx| {
            let result = apply.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(_) => {
                        if let Some(confirm) = &this.directory.delete_confirm {
                            if this
                                .directory
                                .open_doc
                                .as_ref()
                                .is_some_and(|doc| doc.doc.path == confirm.request.path)
                            {
                                this.directory.open_doc = None;
                            }
                        }
                        this.directory.delete_confirm = None;
                        this.directory.selected_path = None;
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.refresh_workspace_entries(window, cx);
                        this.status = "已移入工作空间回收站".into();
                    }
                    Err(error) => {
                        if let Some(confirm) = &mut this.directory.delete_confirm {
                            confirm.busy = false;
                        }
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn toggle_trash_panel(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.directory.trash_open {
            self.directory.trash_open = false;
            cx.notify();
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        self.directory.trash_open = true;
        self.directory.trash_loading = true;
        self.directory.trash_error = None;
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        cx.notify();
        let load = cx
            .background_executor()
            .spawn(async move { list_workspace_trash_at(&root, &expected_path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                this.directory.trash_loading = false;
                match result {
                    Ok(entries) => this.directory.trash_entries = entries,
                    Err(error) => this.directory.trash_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn restore_trash_item(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let request = DirRequest {
            action: DirAction::Restore,
            path: String::new(),
            name: None,
            trash_id: Some(id),
        };
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        let task = cx.background_executor().spawn(async move {
            let plan = preview_directory_change_at(&root, &expected_path, &request)?;
            apply_directory_change_at(&root, &expected_path, &request, &plan.token)?;
            list_workspace_trash_at(&root, &expected_path)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(entries) => {
                        this.directory.trash_entries = entries;
                        this.directory.invalidate_tree();
                        this.ensure_directory_tree(window, cx);
                        this.refresh_workspace_entries(window, cx);
                        this.status = "已从回收站恢复项目".into();
                    }
                    Err(error) => {
                        this.directory.trash_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn leave_journal_editor_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_edit.is_some() {
            return self.allow_workspace_close(false, window, cx);
        }
        true
    }

    pub(crate) fn open_search_result(
        &mut self,
        item: SearchResultItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.directory.search_dialog_open = false;
        if item.kind == "journal" {
            if let Some(date) = item.date {
                self.directory.open_doc = None;
                let format = item
                    .format
                    .unwrap_or_else(|| self.workspace_default_format());
                self.open_journal(date, format, window, cx);
                return;
            }
        }
        self.open_workspace_document(item.path, window, cx);
    }
}

pub(crate) fn render_directory_module(
    app: &Prototype,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    use crate::icons::{PhIcon, ph_icon};

    let palette = crate::theme_skin::active_palette(cx);
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let accent = theme.primary;
    let secondary = theme.secondary;

    let tree = build_tree(&app.directory.nodes);
    let rows = visible_rows(&tree, &app.directory.collapsed);
    let all_expanded = app.directory.collapsed.is_empty();
    let active_doc_path = app
        .directory
        .open_doc
        .as_ref()
        .map(|doc| doc.doc.path.as_str());
    let current_journal_date = workspace.current_date();
    let ws_name = workspace.info_name().to_string();
    let item_count = app.directory.nodes.len();

    let dir_tool_btn = |id: &'static str, label: &'static str, icon: PhIcon, active: bool| {
        FlatButton::new(id)
            .accessibility_label(label)
            .selected(active)
            .w(px(26.))
            .h(px(26.))
            .rounded(px(5.))
            .bg(if active {
                palette.tab_active()
            } else {
                palette.surface_header
            })
            .hover(move |s| s.bg(palette.tab_active()))
            .flex()
            .items_center()
            .justify_center()
            .child(ph_icon(
                icon,
                px(15.),
                if active {
                    palette.accent
                } else {
                    palette.text_muted
                },
            ))
    };

    div()
        .id("directory-module")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_1p5()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_1p5()
                .min_h(px(26.))
                .px_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_start()
                        .gap_1p5()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(palette.text)
                                .truncate()
                                .child(ws_name),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(palette.text_muted)
                                .child(format!("{item_count}")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .child(
                            dir_tool_btn("directory-new-file", "新建文件", PhIcon::FilePlus, false)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_directory_form(
                                        DirectoryFormKind::CreateFile,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            dir_tool_btn(
                                "directory-new-dir",
                                "新建文件夹",
                                PhIcon::FolderPlus,
                                false,
                            )
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_directory_form(DirectoryFormKind::CreateDir, window, cx);
                            })),
                        )
                        .child(
                            dir_tool_btn("directory-rename", "重命名", PhIcon::NotePencil, false)
                                .disabled(busy || app.directory.selected_path.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.start_directory_form(
                                        DirectoryFormKind::Rename,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            dir_tool_btn("directory-delete", "删除", PhIcon::Trash, false)
                                .disabled(busy || app.directory.selected_path.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.request_delete_selected(window, cx);
                                })),
                        )
                        .child(
                            dir_tool_btn(
                                "directory-toggle-all",
                                if all_expanded { "全部折叠" } else { "全部展开" },
                                if all_expanded {
                                    PhIcon::ArrowsInSimple
                                } else {
                                    PhIcon::ArrowsOutSimple
                                },
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.directory.collapsed.is_empty() {
                                    for node in &this.directory.nodes {
                                        if node.is_dir {
                                            this.directory.collapsed.insert(node.path.clone());
                                        }
                                    }
                                } else {
                                    this.directory.collapsed.clear();
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            dir_tool_btn(
                                "directory-trash",
                                "回收站",
                                PhIcon::ClockCounterClockwise,
                                app.directory.trash_open,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_trash_panel(window, cx);
                            })),
                        )
                        .child(
                            dir_tool_btn(
                                "directory-refresh",
                                "刷新",
                                PhIcon::ArrowClockwise,
                                false,
                            )
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.directory.invalidate_tree();
                                this.ensure_directory_tree(window, cx);
                            })),
                        )
                        .child(
                            dir_tool_btn(
                                "global-search-open",
                                "全文检索",
                                PhIcon::MagnifyingGlass,
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.directory.search_dialog_open = true;
                                cx.notify();
                            })),
                        ),
                ),
        )
        .when_some(app.directory.form.as_ref(), |panel, form| {
            let title = match form.kind {
                DirectoryFormKind::CreateFile => "新建文件",
                DirectoryFormKind::CreateDir => "新建文件夹",
                DirectoryFormKind::Rename => "重命名",
            };
            panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(accent)
                    .bg(secondary.opacity(0.45))
                    .child(div().text_size(px(11.)).text_color(muted).child(
                        if form.parent.is_empty() {
                            format!("{title}（根目录）")
                        } else {
                            format!("{title}：{}", form.parent)
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Input::new(&form.input)
                                    .aria_label(title)
                                    .flex_1(),
                            )
                            .child(
                                Button::new("directory-form-submit")
                                    .label("确定")
                                    .primary()
                                    .disabled(form.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_directory_form(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("directory-form-cancel")
                                    .label("取消")
                                    .disabled(form.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.directory.form = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when_some(form.error.as_ref(), |box_el, error| {
                        box_el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(cx.theme().red)
                                .child(error.clone()),
                        )
                    }),
            )
        })
        .when_some(app.directory.delete_confirm.as_ref(), |panel, confirm| {
            let detail = format!(
                "将「{}」移至 .trash（文件 {} 个、目录 {} 个、日报 {} 篇）",
                confirm.request.path,
                confirm.plan.file_count,
                confirm.plan.directory_count,
                confirm.plan.journal_count
            );
            panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().red)
                    .bg(secondary.opacity(0.55))
                    .child(div().text_size(px(11.)).child(detail))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new("directory-delete-confirm")
                                    .label("确认移入回收站")
                                    .primary()
                                    .disabled(confirm.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_delete_selected(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("directory-delete-cancel")
                                    .label("取消")
                                    .disabled(confirm.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.directory.delete_confirm = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
        })
        .when(app.directory.trash_open, |panel| {
            panel.child(render_trash_section(&app.directory, cx))
        })
        .when_some(app.directory.error.as_ref(), |panel, error| {
            panel.child(
                div()
                    .text_size(px(12.))
                    .text_color(cx.theme().red)
                    .child(error.clone()),
            )
        })
        .child(
            div()
                .id("directory-tree-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .when(app.directory.loading && rows.is_empty(), |list| {
                    list.child(div().text_size(px(12.)).text_color(muted).child("正在读取目录…"))
                })
                .when(!app.directory.loading && rows.is_empty(), |list| {
                    list.child(
                        div()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child("工作空间暂无其他文件"),
                    )
                })
                .children(rows.into_iter().enumerate().map(|(idx, row)| {
                    let node = row.node;
                    let is_dir = node.is_dir;
                    let path = node.path.clone();
                    let is_collapsed = is_dir && app.directory.collapsed.contains(&path);
                    let is_selected = app.directory.selected_path.as_deref() == Some(path.as_str());
                    let is_current = match active_doc_path {
                        Some(doc_path) => doc_path == path,
                        None => {
                            node.journal_date.as_deref().is_some()
                                && node.journal_date.as_deref() == current_journal_date
                        }
                    };
                    let active_row = is_selected || is_current;
                    let file_icon = if is_dir {
                        if is_collapsed {
                            PhIcon::Folder
                        } else {
                            PhIcon::FolderOpen
                        }
                    } else if node.journal_date.is_some() {
                        PhIcon::CalendarBlank
                    } else if is_table_filename(&node.name) {
                        PhIcon::FileXls
                    } else {
                        PhIcon::FileText
                    };
                    let icon_color = if active_row || is_dir || node.journal_date.is_some() {
                        palette.accent
                    } else {
                        palette.text_muted
                    };
                    let text_color = if active_row {
                        palette.accent
                    } else {
                        palette.text_body
                    };
                    let path_click = path.clone();
                    let date_opt = node.journal_date.clone();
                    FlatButton::new(format!("directory-row-{idx}"))
                        .selected(active_row)
                        .disabled(busy)
                        .w_full()
                        .h(px(27.))
                        .pl(px(6.0 + (row.depth as f32) * 14.0))
                        .pr_2()
                        .rounded(px(4.))
                        .bg(if active_row {
                            palette.tab_active()
                        } else {
                            palette.surface_header
                        })
                        .hover(move |s| s.bg(palette.subtle))
                        .flex()
                        .items_center()
                        .justify_start()
                        .gap_1p5()
                        .when(is_dir, |row_el| {
                            row_el.child(ph_icon(
                                if is_collapsed {
                                    PhIcon::CaretRight
                                } else {
                                    PhIcon::CaretDown
                                },
                                px(11.),
                                palette.text_muted,
                            ))
                        })
                        .when(!is_dir, |row_el| row_el.child(div().w(px(11.))))
                        .child(ph_icon(file_icon, px(15.), icon_color))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(text_color)
                                .truncate()
                                .child(node.name.clone()),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.directory.selected_path = Some(path_click.clone());
                            if is_dir {
                                if this.directory.collapsed.contains(&path_click) {
                                    this.directory.collapsed.remove(&path_click);
                                } else {
                                    this.directory.collapsed.insert(path_click.clone());
                                }
                                cx.notify();
                            } else if let Some(date) = &date_opt {
                                if !this.directory.close_clean_document() {
                                    this.status =
                                        "当前工作空间文档有未保存修改，请先保存或放弃".into();
                                    cx.notify();
                                    return;
                                }
                                if let Some(entry) = this
                                    .workspace
                                    .as_ref()
                                    .and_then(|ws| ws.entries.iter().find(|e| e.date == *date))
                                    .cloned()
                                {
                                    this.open_journal(entry.date, entry.format, window, cx);
                                }
                            } else {
                                this.open_workspace_document(path_click.clone(), window, cx);
                            }
                        }))
                })),
        )
}

fn render_trash_section(
    directory: &DirectoryState,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_size(px(12.)).child("回收站 (.trash)"))
                .child(
                    Button::new("directory-trash-close")
                        .label("收起")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.directory.trash_open = false;
                            cx.notify();
                        })),
                ),
        )
        .when_some(directory.trash_error.as_ref(), |el, err| {
            el.child(div().text_size(px(11.)).text_color(cx.theme().red).child(err.clone()))
        })
        .when(directory.trash_loading, |el| {
            el.child(div().text_size(px(11.)).text_color(muted).child("读取回收站…"))
        })
        .when(!directory.trash_loading && directory.trash_entries.is_empty(), |el| {
            el.child(div().text_size(px(11.)).text_color(muted).child("回收站为空"))
        })
        .children(directory.trash_entries.iter().map(|entry| {
            let id = entry.id.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_size(px(11.))
                        .flex_1()
                        .child(format!("{} ({})", entry.name, entry.path)),
                )
                .child(
                    Button::new(format!("directory-trash-restore-{}", entry.id))
                        .label("恢复")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.restore_trash_item(id.clone(), window, cx);
                        })),
                )
        }))
}

fn render_search_results_list(
    directory: &DirectoryState,
    prefix: &str,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let secondary = cx.theme().secondary;
    div()
        .id(format!("{prefix}-results"))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_1()
        .when(directory.search_loading, |list| {
            list.child(div().text_size(px(12.)).text_color(muted).child("正在检索…"))
        })
        .when_some(directory.search_error.as_ref(), |list, err| {
            list.child(div().text_size(px(12.)).text_color(cx.theme().red).child(err.clone()))
        })
        .when(
            !directory.search_loading
                && directory.search_error.is_none()
                && directory.search_results.is_empty()
                && !directory.search_query.trim().is_empty(),
            |list| {
                list.child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("未找到匹配内容"),
                )
            },
        )
        .children(directory.search_results.iter().enumerate().map(|(idx, item)| {
            let item_clone = item.clone();
            let kind_badge = if item.kind == "journal" { "日报" } else { "文件" };
            Button::new(format!("{prefix}-item-{idx}"))
                .border_1()
                .border_color(border)
                .bg(secondary.opacity(0.35))
                .p_2()
                .justify_start()
                .label(format!(
                    "[{kind_badge}] {} ({}处) — {}",
                    item.title, item.match_count, item.snippet
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_search_result(item_clone.clone(), window, cx);
                }))
        }))
}

pub(crate) fn render_global_search_dialog(
    directory: &DirectoryState,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let muted = theme.muted_foreground;

    div()
        .id("global-search-backdrop")
        .occlude()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(theme.background.opacity(0.75))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("global-search-dialog")
                .w_full()
                .max_w(px(680.))
                .h(px(480.))
                .bg(background)
                .border_1()
                .border_color(border)
                .rounded_lg()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child("工作空间全局搜索"))
                        .child(
                            Button::new("global-search-close")
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.directory.search_dialog_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    Input::new(&directory.search_input)
                        .aria_label("输入关键词搜索全部日报和文件")
                        .w_full(),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child("支持多关键词空格分词；点击结果直接打开对应日报或文件。"),
                )
                .child(render_search_results_list(directory, "global-search", cx)),
        )
}

pub(crate) fn render_workspace_document_editor(
    open_doc: &WorkspaceDocumentState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let doc = &open_doc.doc;

    div()
        .id("workspace-document-pane")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child(doc.filename.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(if doc.kind == "text" {
                                    format!(
                                        "{}{}",
                                        doc.encoding,
                                        if open_doc.dirty { " · 未保存" } else { "" }
                                    )
                                } else if doc.kind == "table" {
                                    "表格只读预览".to_string()
                                } else {
                                    "暂不支持编辑".to_string()
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .when(doc.kind == "text", |bar| {
                            let content_copy = open_doc.draft.clone();
                            bar.child(
                                Button::new("workspace-doc-copy")
                                    .label("复制正文")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            content_copy.clone(),
                                        ));
                                        this.status = "已复制文件内容".into();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("workspace-doc-save")
                                    .label(if open_doc.saving { "保存中…" } else { "保存文档" })
                                    .primary()
                                    .disabled(busy || open_doc.saving || !open_doc.dirty)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_open_workspace_document(window, cx);
                                    })),
                            )
                        })
                        .when(open_doc.confirm_discard && open_doc.dirty, |bar| {
                            bar.child(
                                Button::new("workspace-doc-discard")
                                    .label("放弃修改")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.directory.open_doc = None;
                                        this.status = "已放弃文件修改".into();
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            Button::new("workspace-doc-back")
                                .label("返回日报")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.directory.has_dirty_document() {
                                        if let Some(doc) = &mut this.directory.open_doc {
                                            doc.confirm_discard = true;
                                        }
                                        this.status =
                                            "当前文档有未保存修改，请先保存或点击「放弃修改」"
                                                .into();
                                        cx.notify();
                                        return;
                                    }
                                    this.directory.open_doc = None;
                                    this.status = "已返回日报视图".into();
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .when_some(open_doc.error.as_ref(), |pane, err| {
            pane.child(div().text_size(px(12.)).text_color(theme.red).child(err.clone()))
        })
        .child(match doc.kind {
            "text" => match &open_doc.editor {
                None => pane_fallback("文档编辑器未就绪", muted),
                Some(editor) if doc.markdown => div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .child(
                        Editor::new(editor)
                            .aria_label(doc.filename.clone())
                            .flex_1()
                            .min_h_0(),
                    )
                    .child(
                        div()
                            .id("workspace-doc-md-preview")
                            .flex_1()
                            .min_h_0()
                            .border_1()
                            .border_color(border)
                            .rounded_md()
                            .p_3()
                            .overflow_y_scroll()
                            .child(markdown_preview(
                                "workspace-doc-markdown",
                                SharedString::from(open_doc.draft.clone()),
                            )),
                    )
                    .into_any_element(),
                Some(editor) => Editor::new(editor)
                    .aria_label(doc.filename.clone())
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .into_any_element(),
            },
            "table" => render_table_sheets(
                &doc.sheets,
                open_doc.active_sheet,
                doc.message.as_str(),
                cx,
            )
            .into_any_element(),
            _ => div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(border)
                .rounded_md()
                .p_4()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(muted)
                .child(if doc.message.is_empty() {
                    "暂不支持预览此文件格式。".to_string()
                } else {
                    doc.message.clone()
                })
                .into_any_element(),
        })
}

fn pane_fallback(message: &str, muted: gpui_kit::Hsla) -> gpui_kit::AnyElement {
    div()
        .text_size(px(12.))
        .text_color(muted)
        .child(message.to_string())
        .into_any_element()
}

pub(crate) fn render_table_sheets(
    sheets: &[ExcelSheetPreview],
    active_sheet: usize,
    notice: &str,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let secondary = theme.secondary;

    let current = sheets.get(active_sheet).or_else(|| sheets.first());

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .when(!notice.is_empty(), |pane| {
            pane.child(
                div()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(notice.to_string()),
            )
        })
        .when(sheets.len() > 1, |pane| {
            pane.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(sheets.iter().enumerate().map(|(idx, sheet)| {
                        let active = idx == active_sheet;
                        Button::new(format!("workspace-doc-sheet-{idx}"))
                            .label(sheet.name.clone())
                            .toggled(active)
                            .when(active, Button::primary)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(doc) = &mut this.directory.open_doc {
                                    doc.active_sheet = idx;
                                    cx.notify();
                                }
                            }))
                    })),
            )
        })
        .child(
            div()
                .id("table-sheet-scroll")
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(border)
                .rounded_md()
                .p_2()
                .overflow_y_scroll()
                .overflow_x_scroll()
                .child(if let Some(sheet) = current {
                    render_sheet_regions(sheet, border, secondary, muted).into_any_element()
                } else {
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("工作簿没有可预览的单元格")
                        .into_any_element()
                }),
        )
}

fn render_sheet_regions(
    sheet: &ExcelSheetPreview,
    border: gpui_kit::Hsla,
    secondary: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .when(sheet.truncated, |el| {
            el.child(
                div()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child("内容已按安全上限截断显示（最多 240 行 × 80 列）。"),
            )
        })
        .children(sheet.regions.iter().map(|region| {
            let cell_map: BTreeMap<(u32, u32), &str> = region
                .cells
                .iter()
                .map(|cell| ((cell.row, cell.column), cell.text.as_str()))
                .collect();
            div()
                .flex()
                .flex_col()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .child(
                    div()
                        .flex()
                        .bg(secondary.opacity(0.6))
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .w(px(44.))
                                .p_1()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child("#"),
                        )
                        .children(region.columns.iter().map(|col| {
                            div()
                                .w(px(120.))
                                .p_1()
                                .border_l_1()
                                .border_color(border)
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(col.label.clone())
                        })),
                )
                .children(region.rows.iter().map(|row| {
                    div()
                        .flex()
                        .border_b_1()
                        .border_color(border.opacity(0.6))
                        .child(
                            div()
                                .w(px(44.))
                                .p_1()
                                .bg(secondary.opacity(0.35))
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(row.index.to_string()),
                        )
                        .children(region.columns.iter().map(|col| {
                            let text = cell_map
                                .get(&(row.index, col.index))
                                .copied()
                                .unwrap_or_default();
                            div()
                                .w(px(120.))
                                .p_1()
                                .border_l_1()
                                .border_color(border.opacity(0.6))
                                .text_size(px(12.))
                                .child(SelectableText::new(
                                    format!("cell-{}-{}", row.index, col.index),
                                    text.to_string(),
                                ))
                        }))
                }))
        }))
}
