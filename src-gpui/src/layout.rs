use std::collections::{BTreeMap, HashSet};

use workreport_core::appearance::{self, Settings as AppearanceSettings};
use workreport_core::ui::{self, Axis, Placement, Preferences as CorePreferences};

pub(crate) const MIN_SPLIT_RATIO: f64 = 0.01;
pub(crate) const MAX_SPLIT_RATIO: f64 = 0.99;
pub(crate) const DEFAULT_SPLIT_RATIO: f64 = 0.5;
pub(crate) const MAX_MODULES: usize = 12;
const MAX_LAYOUT_DEPTH: usize = MAX_MODULES;
pub(crate) const MAX_SAVED_LAYOUTS: usize = 20;
pub(crate) const MAX_LAYOUT_NAME_CHARS: usize = 50;
pub(crate) const BOARD_GAP: f32 = 6.0;
pub(crate) const EDGE_BAND: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ModuleType {
    Calendar,
    Editor,
    Overview,
    History,
    Summary,
    Excel,
    Directory,
    Git,
}

impl ModuleType {
    pub(crate) const ALL: [Self; 8] = [
        Self::Calendar,
        Self::Editor,
        Self::Overview,
        Self::History,
        Self::Excel,
        Self::Summary,
        Self::Directory,
        Self::Git,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::Editor => "editor",
            Self::Overview => "overview",
            Self::History => "history",
            Self::Summary => "summary",
            Self::Excel => "excel",
            Self::Directory => "directory",
            Self::Git => "git",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Calendar => "日历",
            Self::Editor => "编辑",
            Self::Overview => "数据概览",
            Self::History => "历史记录",
            Self::Summary => "总结",
            Self::Excel => "Excel报告",
            Self::Directory => "工作空间目录",
            Self::Git => "Git",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Calendar => "周 / 月 / 年视图、农历与节假日、框选日报。",
            Self::Editor => "Markdown / 纯文本编辑、预览与历史版本。",
            Self::Overview => "日报统计、连续记录天数与每日热力图。",
            Self::History => "查看当前日报的历史版本，并预览任意版本全文。",
            Self::Summary => "对当前选中日期或当天日报生成 AI 总结或直接合并总结。",
            Self::Excel => "把所选日期范围的日报整理进 Excel 模板，预览确认后输出。",
            Self::Directory => "浏览和管理工作空间内的文件与文件夹。",
            Self::Git => "多仓库 Git 工作台，管理更改、分支与同步，跨仓库多选提交并获取信息。",
        }
    }

    pub(crate) fn help(self) -> &'static [&'static str] {
        match self {
            Self::Calendar => &[
                "点击任意日期，切换日报编辑区显示的当天内容。",
                "在日期上按下并拖到另一天，可框选一段连续日期，用于批量查看。",
                "顶部可切换周 / 月 / 年视图；日期上的圆点表示当天已有日报。",
            ],
            Self::Editor => &[
                "点击「编辑日报」进入编辑，停笔 650ms 后自动保存；按 ⌘S / Ctrl+S 可立即保存。",
                "框选多个日期时，在此模块内按日期倒序预览或切换原文。",
            ],
            Self::Overview => &[
                "统计本月、本年、本周的日报数量与连续记录天数。",
                "热力图按字数分级展示近 365 天记录密度，点击格子可跳转到对应日期。",
            ],
            Self::History => &[
                "自动保存的历史快照每分钟最多生成一份；手动保存或离开时补存最终版本。",
                "点击任意历史版本可在编辑模块预览该版本内容。",
            ],
            Self::Summary => &[
                "对选中日期或日历范围生成直接汇总或 AI 总结，并导出为 Markdown / TXT 文件。",
            ],
            Self::Excel => &["两阶段 Excel 报告模块将在阶段 3 迁移至原生 GPUI。"],
            Self::Directory => &[
                "浏览工作空间目录树与全屏全文搜索，支持新建、重命名、回收站与普通文本编辑。",
                "Excel 与 CSV 表格在此以只读方式预览。",
            ],
            Self::Git => &["多仓库 Git 工作台将在阶段 3 迁移至原生 GPUI。"],
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            "calendar" => Some(Self::Calendar),
            "editor" => Some(Self::Editor),
            "overview" => Some(Self::Overview),
            "history" => Some(Self::History),
            "summary" => Some(Self::Summary),
            "excel" => Some(Self::Excel),
            "directory" => Some(Self::Directory),
            "git" => Some(Self::Git),
            _ => None,
        }
    }

    pub(crate) fn parse_legacy_id(id: &str) -> Option<(Self, bool)> {
        if let Some(module) = Self::from_id(id) {
            return Some((module, false));
        }
        if let Some(suffix) = id.strip_prefix("editor-") {
            if !suffix.is_empty()
                && suffix.len() <= 9
                && suffix.as_bytes()[0].is_ascii_digit()
                && suffix.as_bytes()[0] != b'0'
                && suffix.bytes().all(|b| b.is_ascii_digit())
            {
                return Some((Self::Editor, true));
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SplitDirection {
    #[default]
    Row,
    Column,
}

impl SplitDirection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Column => "column",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropSide {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RailSide {
    Left,
    #[default]
    Right,
}

impl RailSide {
    pub(crate) fn from_setting(value: &str) -> Self {
        if value == "left" {
            Self::Left
        } else {
            Self::Right
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayoutNode {
    Leaf {
        id: ModuleType,
    },
    Split {
        direction: SplitDirection,
        ratio: f64,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Layout {
    Empty,
    Node(LayoutNode),
}

impl Default for Layout {
    fn default() -> Self {
        default_layout()
    }
}

pub(crate) fn layout_node_to_json(node: &LayoutNode) -> serde_json::Value {
    match node {
        LayoutNode::Leaf { id } => serde_json::json!({
            "type": "leaf",
            "id": id.id(),
        }),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => serde_json::json!({
            "type": "split",
            "direction": direction.as_str(),
            "ratio": ratio,
            "first": layout_node_to_json(first),
            "second": layout_node_to_json(second),
        }),
    }
}

pub(crate) fn layout_to_json(layout: &Layout) -> serde_json::Value {
    match layout {
        Layout::Empty => serde_json::json!({ "type": "empty" }),
        Layout::Node(node) => layout_node_to_json(node),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SavedLayout {
    pub(crate) name: String,
    pub(crate) layout: Layout,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiPreferences {
    pub(crate) layout: Layout,
    pub(crate) skin: String,
    pub(crate) saved_layouts: Vec<SavedLayout>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        default_ui_preferences()
    }
}

pub(crate) struct SkinInfo {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    #[allow(dead_code)]
    pub(crate) dark: bool,
}

pub(crate) const SKINS: &[SkinInfo] = &[
    SkinInfo {
        id: "default",
        name: "默认",
        description: "清透留白，经典蓝灰。",
        dark: false,
    },
    SkinInfo {
        id: "islands-light",
        name: "Islands 浅色",
        description: "IDEA 风格，灰白工具窗口与蓝色活动标签。",
        dark: false,
    },
    SkinInfo {
        id: "islands-dark",
        name: "Islands 深色",
        description: "IDEA 风格，石墨工具窗口与独立编辑区。",
        dark: true,
    },
    SkinInfo {
        id: "apple",
        name: "Apple 风格",
        description: "灰白层次，系统蓝与细腻圆角。",
        dark: false,
    },
    SkinInfo {
        id: "frosted-glass",
        name: "磨砂玻璃",
        description: "柔和光影，通透面板与磨砂质感。",
        dark: false,
    },
    SkinInfo {
        id: "jade",
        name: "青竹",
        description: "浅青底色，竹绿点缀。",
        dark: false,
    },
    SkinInfo {
        id: "apricot",
        name: "暖杏",
        description: "温润纸色，琥珀暖意。",
        dark: false,
    },
    SkinInfo {
        id: "wisteria",
        name: "紫藤",
        description: "淡紫雾色，柔和安静。",
        dark: false,
    },
    SkinInfo {
        id: "midnight",
        name: "午夜蓝",
        description: "深蓝夜色，明净星光。",
        dark: true,
    },
    SkinInfo {
        id: "graphite",
        name: "石墨",
        description: "沉稳灰阶，薄荷微光。",
        dark: true,
    },
    SkinInfo {
        id: "vscode-dark-2026",
        name: "VS Code 黑色 2026",
        description: "Dark 2026 · 黑灰底色。",
        dark: true,
    },
    SkinInfo {
        id: "vscode-light-2026",
        name: "VS Code 白色 2026",
        description: "Light 2026 · 纯白底色。",
        dark: false,
    },
];

pub(crate) fn skin_by_id(id: &str) -> &'static SkinInfo {
    SKINS
        .iter()
        .find(|skin| skin.id == id)
        .unwrap_or(&SKINS[0])
}

pub(crate) struct LayoutPreset {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    build: fn() -> Layout,
}

impl LayoutPreset {
    pub(crate) fn layout(&self) -> Layout {
        (self.build)()
    }
}

pub(crate) const LAYOUT_PRESETS: &[LayoutPreset] = &[LayoutPreset {
    id: "compact-writing",
    name: "紧凑月历",
    description: "左侧月历与概览，右侧留给编辑。",
    build: || {
        Layout::Node(LayoutNode::Split {
            direction: SplitDirection::Row,
            ratio: 0.32,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Column,
                ratio: 0.62,
                first: Box::new(LayoutNode::Leaf {
                    id: ModuleType::Calendar,
                }),
                second: Box::new(LayoutNode::Leaf {
                    id: ModuleType::Overview,
                }),
            }),
            second: Box::new(LayoutNode::Leaf {
                id: ModuleType::Editor,
            }),
        })
    },
}];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutSlot {
    pub(crate) id: ModuleType,
    pub(crate) rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutGutter {
    pub(crate) rect: Rect,
    pub(crate) path: Vec<usize>,
    pub(crate) direction: SplitDirection,
    pub(crate) ratio: f64,
    pub(crate) parent: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LayoutGeometry {
    pub(crate) slots: Vec<LayoutSlot>,
    pub(crate) gutters: Vec<LayoutGutter>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutMetrics {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) gap: f64,
}

impl Default for LayoutMetrics {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            gap: 0.0,
        }
    }
}

pub(crate) fn default_layout() -> Layout {
    Layout::Node(LayoutNode::Split {
        direction: SplitDirection::Row,
        ratio: DEFAULT_SPLIT_RATIO,
        first: Box::new(LayoutNode::Leaf {
            id: ModuleType::Calendar,
        }),
        second: Box::new(LayoutNode::Leaf {
            id: ModuleType::Editor,
        }),
    })
}

pub(crate) fn default_ui_preferences() -> UiPreferences {
    UiPreferences {
        layout: default_layout(),
        skin: ui::DEFAULT_SKIN.into(),
        saved_layouts: Vec::new(),
    }
}

pub(crate) fn clamp_split_ratio(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_SPLIT_RATIO;
    }
    value.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
}

fn normalize_skin(value: Option<&str>) -> String {
    value
        .and_then(|id| SKINS.iter().find(|skin| skin.id == id))
        .map(|skin| skin.id.to_owned())
        .unwrap_or_else(|| ui::DEFAULT_SKIN.to_owned())
}

fn has_editor_value(value: &serde_json::Value, depth: usize) -> bool {
    if depth > MAX_LAYOUT_DEPTH {
        return false;
    }
    let Some(obj) = value.as_object() else {
        return false;
    };
    match obj.get("type").and_then(serde_json::Value::as_str) {
        Some("leaf") => obj.get("id").and_then(serde_json::Value::as_str) == Some("editor"),
        Some("split") => {
            obj.get("first")
                .is_some_and(|first| has_editor_value(first, depth + 1))
                || obj
                    .get("second")
                    .is_some_and(|second| has_editor_value(second, depth + 1))
        }
        _ => false,
    }
}

fn normalize_layout_node(
    value: &serde_json::Value,
    seen: &mut HashSet<ModuleType>,
    depth: usize,
    keep_editor: bool,
) -> Option<LayoutNode> {
    if depth > MAX_LAYOUT_DEPTH {
        return None;
    }
    let obj = value.as_object()?;
    match obj.get("type").and_then(serde_json::Value::as_str)? {
        "leaf" => {
            if seen.len() >= MAX_MODULES {
                return None;
            }
            let raw_id = obj.get("id").and_then(serde_json::Value::as_str)?;
            let (id, legacy_preview) = ModuleType::parse_legacy_id(raw_id)?;
            if legacy_preview && keep_editor {
                return None;
            }
            if !seen.insert(id) {
                return None;
            }
            Some(LayoutNode::Leaf { id })
        }
        "split" => {
            let first = obj
                .get("first")
                .and_then(|node| normalize_layout_node(node, seen, depth + 1, keep_editor));
            let second = obj
                .get("second")
                .and_then(|node| normalize_layout_node(node, seen, depth + 1, keep_editor));
            match (first, second) {
                (Some(first), Some(second)) => {
                    let direction = match obj.get("direction").and_then(serde_json::Value::as_str) {
                        Some("column") => SplitDirection::Column,
                        _ => SplitDirection::Row,
                    };
                    let ratio = obj
                        .get("ratio")
                        .and_then(serde_json::Value::as_f64)
                        .map(clamp_split_ratio)
                        .unwrap_or(DEFAULT_SPLIT_RATIO);
                    Some(LayoutNode::Split {
                        direction,
                        ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    })
                }
                (Some(node), None) | (None, Some(node)) => Some(node),
                (None, None) => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn normalize_layout_value(value: &serde_json::Value) -> Option<Layout> {
    if value
        .as_object()
        .and_then(|obj| obj.get("type"))
        .and_then(serde_json::Value::as_str)
        == Some("empty")
    {
        return Some(Layout::Empty);
    }
    let keep_editor = has_editor_value(value, 0);
    let mut seen = HashSet::new();
    normalize_layout_node(value, &mut seen, 0, keep_editor).map(Layout::Node)
}

#[cfg(test)]
fn collect_module_ids_from_json(value: Option<&serde_json::Value>) -> Vec<ModuleType> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let raw_ids: Vec<&str> = items
        .iter()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item.get("id").and_then(serde_json::Value::as_str))
        })
        .collect();
    collect_module_ids(&raw_ids)
}

fn collect_module_ids(raw_ids: &[&str]) -> Vec<ModuleType> {
    let keep_editor = raw_ids.contains(&"editor");
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for raw in raw_ids {
        let Some((id, legacy_preview)) = ModuleType::parse_legacy_id(raw) else {
            continue;
        };
        if legacy_preview && keep_editor {
            continue;
        }
        if seen.insert(id) {
            ids.push(id);
            if ids.len() >= MAX_MODULES {
                break;
            }
        }
    }
    ids
}

fn stack_vertically(nodes: Vec<LayoutNode>) -> LayoutNode {
    let count = nodes.len();
    let mut iter = nodes.into_iter();
    let first = iter.next().expect("stack_vertically requires non-empty nodes");
    let rest: Vec<LayoutNode> = iter.collect();
    if rest.is_empty() {
        return first;
    }
    LayoutNode::Split {
        direction: SplitDirection::Column,
        ratio: 1.0 / count as f64,
        first: Box::new(first),
        second: Box::new(stack_vertically(rest)),
    }
}

fn build_legacy_layout(ids: &[ModuleType], single_column: bool) -> LayoutNode {
    let leaves: Vec<LayoutNode> = ids.iter().map(|&id| LayoutNode::Leaf { id }).collect();
    if single_column {
        return stack_vertically(leaves);
    }
    let mut rows = Vec::new();
    for chunk in leaves.chunks(2) {
        if chunk.len() == 2 {
            rows.push(LayoutNode::Split {
                direction: SplitDirection::Row,
                ratio: DEFAULT_SPLIT_RATIO,
                first: Box::new(chunk[0].clone()),
                second: Box::new(chunk[1].clone()),
            });
        } else {
            rows.push(chunk[0].clone());
        }
    }
    stack_vertically(rows)
}

#[cfg(test)]
fn normalize_saved_layouts_value(value: Option<&serde_json::Value>) -> Vec<SavedLayout> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(raw_name) = obj.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let name = raw_name.trim();
        if name.is_empty() || name.chars().count() > MAX_LAYOUT_NAME_CHARS {
            continue;
        }
        if !seen.insert(name.to_owned()) {
            continue;
        }
        let Some(layout) = obj.get("layout").and_then(normalize_layout_value) else {
            seen.remove(name);
            continue;
        };
        result.push(SavedLayout {
            name: name.to_owned(),
            layout,
        });
        if result.len() >= MAX_SAVED_LAYOUTS {
            break;
        }
    }
    result
}

#[cfg(test)]
pub(crate) fn normalize_ui_preferences(value: &serde_json::Value) -> UiPreferences {
    let Some(obj) = value.as_object() else {
        return default_ui_preferences();
    };
    let skin = normalize_skin(obj.get("skin").and_then(serde_json::Value::as_str));
    let saved_layouts = normalize_saved_layouts_value(obj.get("savedLayouts"));
    if let Some(layout) = obj.get("layout").and_then(normalize_layout_value) {
        return UiPreferences {
            layout,
            skin,
            saved_layouts,
        };
    }
    let modules = collect_module_ids_from_json(obj.get("modules"));
    let legacy_ids = if modules.is_empty() {
        collect_module_ids_from_json(obj.get("order"))
    } else {
        modules
    };
    if legacy_ids.is_empty() {
        return UiPreferences {
            layout: default_layout(),
            skin,
            saved_layouts,
        };
    }
    let single_column = if let Some(cols) = obj.get("columns").and_then(serde_json::Value::as_u64) {
        cols == 1
    } else {
        obj.get("axis").and_then(serde_json::Value::as_str) == Some("column")
    };
    UiPreferences {
        layout: Layout::Node(build_legacy_layout(&legacy_ids, single_column)),
        skin,
        saved_layouts,
    }
}

pub(crate) fn from_core_preferences(preferences: Option<&CorePreferences>) -> UiPreferences {
    let Some(preferences) = preferences else {
        return default_ui_preferences();
    };
    let skin = normalize_skin(Some(preferences.skin.as_str()));
    let saved_layouts = preferences
        .saved_layouts
        .iter()
        .filter_map(|saved| {
            let name = saved.name.trim();
            if name.is_empty() || name.chars().count() > MAX_LAYOUT_NAME_CHARS {
                return None;
            }
            let layout = normalize_layout_value(&saved.layout)?;
            Some(SavedLayout {
                name: name.to_owned(),
                layout,
            })
        })
        .fold(Vec::<SavedLayout>::new(), |mut acc, item| {
            if acc.len() < MAX_SAVED_LAYOUTS && !acc.iter().any(|existing| existing.name == item.name)
            {
                acc.push(item);
            }
            acc
        });

    if let Some(layout) = preferences
        .layout
        .as_ref()
        .and_then(normalize_layout_value)
    {
        return UiPreferences {
            layout,
            skin,
            saved_layouts,
        };
    }

    let module_refs: Vec<&str> = preferences.modules.iter().map(|m| m.id.as_str()).collect();
    let mut legacy_ids = collect_module_ids(&module_refs);
    if legacy_ids.is_empty() {
        let order_refs: Vec<&str> = preferences.order.iter().map(String::as_str).collect();
        legacy_ids = collect_module_ids(&order_refs);
    }
    if legacy_ids.is_empty() {
        return UiPreferences {
            layout: default_layout(),
            skin,
            saved_layouts,
        };
    }
    let single_column = if !preferences.modules.is_empty() {
        preferences.columns == 1
    } else {
        preferences.axis == Some(Axis::Column)
    };
    UiPreferences {
        layout: Layout::Node(build_legacy_layout(&legacy_ids, single_column)),
        skin,
        saved_layouts,
    }
}

pub(crate) fn to_core_preferences(preferences: &UiPreferences) -> CorePreferences {
    let ids = flatten_layout(&preferences.layout);
    let modules = if ids.is_empty() {
        Vec::new()
    } else {
        ids.iter()
            .map(|id| Placement {
                id: id.id().to_owned(),
            })
            .collect()
    };
    let layout = Some(layout_to_json(&preferences.layout));
    let saved_layouts = preferences
        .saved_layouts
        .iter()
        .map(|item| ui::SavedLayout {
            name: item.name.clone(),
            layout: layout_to_json(&item.layout),
        })
        .collect();
    ui::normalize(CorePreferences {
        columns: ui::DEFAULT_COLUMNS,
        modules,
        skin: normalize_skin(Some(preferences.skin.as_str())),
        layout,
        saved_layouts,
        axis: None,
        order: Vec::new(),
    })
}

pub(crate) fn flatten_layout(layout: &Layout) -> Vec<ModuleType> {
    fn walk(node: &LayoutNode, out: &mut Vec<ModuleType>) {
        match node {
            LayoutNode::Leaf { id } => out.push(*id),
            LayoutNode::Split { first, second, .. } => {
                walk(first, out);
                walk(second, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Layout::Node(node) = layout {
        walk(node, &mut out);
    }
    out
}

pub(crate) fn layout_has_module(layout: &Layout, id: ModuleType) -> bool {
    flatten_layout(layout).contains(&id)
}

fn layout_leaf_count(node: &LayoutNode) -> usize {
    match node {
        LayoutNode::Leaf { .. } => 1,
        LayoutNode::Split { first, second, .. } => {
            layout_leaf_count(first) + layout_leaf_count(second)
        }
    }
}

enum DetachResult {
    Unchanged,
    Removed(Option<LayoutNode>),
}

fn detach_layout_module(node: &LayoutNode, id: ModuleType) -> DetachResult {
    match node {
        LayoutNode::Leaf { id: leaf_id } => {
            if *leaf_id == id {
                DetachResult::Removed(None)
            } else {
                DetachResult::Unchanged
            }
        }
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            match detach_layout_module(first, id) {
                DetachResult::Removed(None) => return DetachResult::Removed(Some((**second).clone())),
                DetachResult::Removed(Some(new_first)) => {
                    return DetachResult::Removed(Some(LayoutNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(new_first),
                        second: second.clone(),
                    }));
                }
                DetachResult::Unchanged => {}
            }
            match detach_layout_module(second, id) {
                DetachResult::Removed(None) => DetachResult::Removed(Some((**first).clone())),
                DetachResult::Removed(Some(new_second)) => {
                    DetachResult::Removed(Some(LayoutNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: first.clone(),
                        second: Box::new(new_second),
                    }))
                }
                DetachResult::Unchanged => DetachResult::Unchanged,
            }
        }
    }
}

pub(crate) fn remove_layout_module(layout: &Layout, id: ModuleType) -> Layout {
    let Layout::Node(node) = layout else {
        return Layout::Empty;
    };
    match detach_layout_module(node, id) {
        DetachResult::Removed(Some(next)) => Layout::Node(next),
        DetachResult::Removed(None) => Layout::Empty,
        DetachResult::Unchanged => layout.clone(),
    }
}

pub(crate) fn append_layout_module(layout: &Layout, id: ModuleType) -> Layout {
    let Layout::Node(node) = layout else {
        return Layout::Node(LayoutNode::Leaf { id });
    };
    if layout_has_module(layout, id) {
        return layout.clone();
    }
    let count = layout_leaf_count(node);
    Layout::Node(LayoutNode::Split {
        direction: SplitDirection::Row,
        ratio: clamp_split_ratio(count as f64 / (count + 1) as f64),
        first: Box::new(node.clone()),
        second: Box::new(LayoutNode::Leaf { id }),
    })
}

pub(crate) fn prepend_layout_module(layout: &Layout, id: ModuleType) -> Layout {
    let Layout::Node(node) = layout else {
        return Layout::Node(LayoutNode::Leaf { id });
    };
    if layout_has_module(layout, id) {
        return layout.clone();
    }
    let count = layout_leaf_count(node);
    Layout::Node(LayoutNode::Split {
        direction: SplitDirection::Row,
        ratio: clamp_split_ratio(1.0 / (count + 1) as f64),
        first: Box::new(LayoutNode::Leaf { id }),
        second: Box::new(node.clone()),
    })
}

pub(crate) fn move_layout_module_to_edge(
    root: &Layout,
    dragged: ModuleType,
    side: EdgeSide,
) -> Layout {
    if matches!(root, Layout::Empty) || !layout_has_module(root, dragged) {
        return root.clone();
    }
    let remaining = remove_layout_module(root, dragged);
    if matches!(remaining, Layout::Empty) {
        return root.clone();
    }
    match side {
        EdgeSide::Left => prepend_layout_module(&remaining, dragged),
        EdgeSide::Right => append_layout_module(&remaining, dragged),
    }
}

fn replace_layout_leaf(
    node: &LayoutNode,
    target: ModuleType,
    replacement: &LayoutNode,
) -> Option<LayoutNode> {
    match node {
        LayoutNode::Leaf { id } => (*id == target).then(|| replacement.clone()),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            if let Some(new_first) = replace_layout_leaf(first, target, replacement) {
                return Some(LayoutNode::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: Box::new(new_first),
                    second: second.clone(),
                });
            }
            if let Some(new_second) = replace_layout_leaf(second, target, replacement) {
                return Some(LayoutNode::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: first.clone(),
                    second: Box::new(new_second),
                });
            }
            None
        }
    }
}

pub(crate) fn split_layout(
    root: &Layout,
    dragged: ModuleType,
    target: ModuleType,
    side: DropSide,
) -> Layout {
    if matches!(root, Layout::Empty)
        || dragged == target
        || !layout_has_module(root, dragged)
        || !layout_has_module(root, target)
    {
        return root.clone();
    }
    let remaining = remove_layout_module(root, dragged);
    let Layout::Node(remaining_node) = remaining else {
        return root.clone();
    };
    let direction = match side {
        DropSide::Top | DropSide::Bottom => SplitDirection::Column,
        DropSide::Left | DropSide::Right => SplitDirection::Row,
    };
    let dragged_leaf = LayoutNode::Leaf { id: dragged };
    let target_leaf = LayoutNode::Leaf { id: target };
    let dragged_first = matches!(side, DropSide::Left | DropSide::Top);
    let split = LayoutNode::Split {
        direction,
        ratio: DEFAULT_SPLIT_RATIO,
        first: Box::new(if dragged_first {
            dragged_leaf.clone()
        } else {
            target_leaf.clone()
        }),
        second: Box::new(if dragged_first {
            target_leaf
        } else {
            dragged_leaf
        }),
    };
    replace_layout_leaf(&remaining_node, target, &split)
        .map(Layout::Node)
        .unwrap_or_else(|| root.clone())
}

fn split_at_path<'a>(node: &'a LayoutNode, path: &[usize]) -> Option<&'a LayoutNode> {
    let mut current = node;
    for &step in path {
        let LayoutNode::Split { first, second, .. } = current else {
            return None;
        };
        current = match step {
            0 => first,
            1 => second,
            _ => return None,
        };
    }
    matches!(current, LayoutNode::Split { .. }).then_some(current)
}

fn replace_node_at_path(
    node: &LayoutNode,
    path: &[usize],
    replacement: LayoutNode,
) -> LayoutNode {
    if path.is_empty() {
        return replacement;
    }
    let LayoutNode::Split {
        direction,
        ratio,
        first,
        second,
    } = node
    else {
        return node.clone();
    };
    match path[0] {
        0 => LayoutNode::Split {
            direction: *direction,
            ratio: *ratio,
            first: Box::new(replace_node_at_path(first, &path[1..], replacement)),
            second: second.clone(),
        },
        1 => LayoutNode::Split {
            direction: *direction,
            ratio: *ratio,
            first: first.clone(),
            second: Box::new(replace_node_at_path(second, &path[1..], replacement)),
        },
        _ => node.clone(),
    }
}

fn is_valid_extent(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn walk_layout_geometry(
    node: &LayoutNode,
    rect: Rect,
    path: &mut Vec<usize>,
    gap: f64,
    overrides: Option<&BTreeMap<Vec<usize>, f64>>,
    result: &mut LayoutGeometry,
) -> bool {
    if !is_valid_extent(rect.width) || !is_valid_extent(rect.height) {
        return false;
    }
    match node {
        LayoutNode::Leaf { id } => {
            result.slots.push(LayoutSlot { id: *id, rect });
            true
        }
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let ratio = overrides
                .and_then(|map| map.get(path))
                .copied()
                .unwrap_or(*ratio);
            if !(ratio > 0.0) || ratio >= 1.0 {
                return false;
            }
            let is_row = *direction == SplitDirection::Row;
            let main_size = if is_row { rect.width } else { rect.height };
            let cross_size = if is_row { rect.height } else { rect.width };
            let available = main_size - gap;
            if !is_valid_extent(available) {
                return false;
            }
            let first_size = available * ratio;
            let second_size = available - first_size;
            if !is_valid_extent(first_size) || !is_valid_extent(second_size) {
                return false;
            }
            let (first_rect, second_rect, gutter_rect) = if is_row {
                (
                    Rect {
                        left: rect.left,
                        top: rect.top,
                        width: first_size,
                        height: cross_size,
                    },
                    Rect {
                        left: rect.left + first_size + gap,
                        top: rect.top,
                        width: second_size,
                        height: cross_size,
                    },
                    Rect {
                        left: rect.left + first_size,
                        top: rect.top,
                        width: gap,
                        height: cross_size,
                    },
                )
            } else {
                (
                    Rect {
                        left: rect.left,
                        top: rect.top,
                        width: cross_size,
                        height: first_size,
                    },
                    Rect {
                        left: rect.left,
                        top: rect.top + first_size + gap,
                        width: cross_size,
                        height: second_size,
                    },
                    Rect {
                        left: rect.left,
                        top: rect.top + first_size,
                        width: cross_size,
                        height: gap,
                    },
                )
            };
            result.gutters.push(LayoutGutter {
                rect: gutter_rect,
                path: path.clone(),
                direction: *direction,
                ratio,
                parent: rect,
            });
            path.push(0);
            let ok_first = walk_layout_geometry(first, first_rect, path, gap, overrides, result);
            path.pop();
            if !ok_first {
                return false;
            }
            path.push(1);
            let ok_second = walk_layout_geometry(second, second_rect, path, gap, overrides, result);
            path.pop();
            ok_second
        }
    }
}

pub(crate) fn layout_geometry(
    root: &Layout,
    width: f64,
    height: f64,
    gap: f64,
    overrides: Option<&BTreeMap<Vec<usize>, f64>>,
) -> LayoutGeometry {
    let Layout::Node(node) = root else {
        return LayoutGeometry::default();
    };
    if !is_valid_extent(width) || !is_valid_extent(height) || !gap.is_finite() || gap < 0.0 {
        return LayoutGeometry::default();
    }
    let mut result = LayoutGeometry::default();
    let mut path = Vec::new();
    let rect = Rect {
        left: 0.0,
        top: 0.0,
        width,
        height,
    };
    if walk_layout_geometry(node, rect, &mut path, gap, overrides, &mut result) {
        result
    } else {
        LayoutGeometry::default()
    }
}

pub(crate) fn find_drop_slot(
    slots: &[LayoutSlot],
    x: f64,
    y: f64,
    dragged: Option<ModuleType>,
) -> Option<LayoutSlot> {
    slots
        .iter()
        .find(|slot| {
            dragged != Some(slot.id)
                && x >= slot.rect.left
                && x <= slot.rect.left + slot.rect.width
                && y >= slot.rect.top
                && y <= slot.rect.top + slot.rect.height
        })
        .copied()
}

pub(crate) fn drop_side_for_point(slot: &LayoutSlot, x: f64, y: f64) -> DropSide {
    let dx = if slot.rect.width > 0.0 {
        (x - slot.rect.left) / slot.rect.width - 0.5
    } else {
        0.0
    };
    let dy = if slot.rect.height > 0.0 {
        (y - slot.rect.top) / slot.rect.height - 0.5
    } else {
        0.0
    };
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            DropSide::Right
        } else {
            DropSide::Left
        }
    } else if dy >= 0.0 {
        DropSide::Bottom
    } else {
        DropSide::Top
    }
}

pub(crate) fn drop_preview_rect(slot: &LayoutSlot, side: DropSide) -> Rect {
    let half_w = slot.rect.width / 2.0;
    let half_h = slot.rect.height / 2.0;
    match side {
        DropSide::Left => Rect {
            left: slot.rect.left,
            top: slot.rect.top,
            width: half_w,
            height: slot.rect.height,
        },
        DropSide::Right => Rect {
            left: slot.rect.left + half_w,
            top: slot.rect.top,
            width: half_w,
            height: slot.rect.height,
        },
        DropSide::Top => Rect {
            left: slot.rect.left,
            top: slot.rect.top,
            width: slot.rect.width,
            height: half_h,
        },
        DropSide::Bottom => Rect {
            left: slot.rect.left,
            top: slot.rect.top + half_h,
            width: slot.rect.width,
            height: half_h,
        },
    }
}

fn rightmost_leaf(node: &LayoutNode) -> ModuleType {
    match node {
        LayoutNode::Leaf { id } => *id,
        LayoutNode::Split { second, .. } => rightmost_leaf(second),
    }
}

fn leftmost_leaf(node: &LayoutNode) -> ModuleType {
    match node {
        LayoutNode::Leaf { id } => *id,
        LayoutNode::Split { first, .. } => leftmost_leaf(first),
    }
}

fn find_boundary(node: &LayoutNode, path: &[usize]) -> Option<(ModuleType, ModuleType)> {
    let target = split_at_path(node, path)?;
    let LayoutNode::Split { first, second, .. } = target else {
        return None;
    };
    Some((rightmost_leaf(first), leftmost_leaf(second)))
}

fn find_split_path_by_boundary(
    node: &LayoutNode,
    left_leaf: ModuleType,
    right_leaf: ModuleType,
    path: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    let LayoutNode::Split { first, second, .. } = node else {
        return None;
    };
    if rightmost_leaf(first) == left_leaf && leftmost_leaf(second) == right_leaf {
        return Some(path.clone());
    }
    path.push(0);
    if let Some(found) = find_split_path_by_boundary(first, left_leaf, right_leaf, path) {
        return Some(found);
    }
    path.pop();
    path.push(1);
    if let Some(found) = find_split_path_by_boundary(second, left_leaf, right_leaf, path) {
        return Some(found);
    }
    path.pop();
    None
}

pub(crate) fn insert_layout_module_at_gutter(
    root: &Layout,
    dragged: ModuleType,
    path: &[usize],
) -> Layout {
    let Layout::Node(node) = root else {
        return root.clone();
    };
    let Some((left_leaf, right_leaf)) = find_boundary(node, path) else {
        return root.clone();
    };
    if dragged == left_leaf || dragged == right_leaf || !layout_has_module(root, dragged) {
        return root.clone();
    }
    let Layout::Node(remaining) = remove_layout_module(root, dragged) else {
        return root.clone();
    };
    let mut search_path = Vec::new();
    let Some(target_path) =
        find_split_path_by_boundary(&remaining, left_leaf, right_leaf, &mut search_path)
    else {
        return root.clone();
    };
    let Some(LayoutNode::Split {
        direction,
        first,
        second,
        ..
    }) = split_at_path(&remaining, &target_path)
    else {
        return root.clone();
    };
    let inserted = LayoutNode::Split {
        direction: *direction,
        ratio: 1.0 / 3.0,
        first: first.clone(),
        second: Box::new(LayoutNode::Split {
            direction: *direction,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { id: dragged }),
            second: second.clone(),
        }),
    };
    Layout::Node(replace_node_at_path(&remaining, &target_path, inserted))
}

#[derive(Clone, Copy)]
enum ResizeEdge {
    Start,
    End,
}

fn edge_resize_bounds(
    node: &LayoutNode,
    direction: SplitDirection,
    edge: ResizeEdge,
    extent: f64,
    gap: f64,
    minimum: f64,
) -> (f64, f64) {
    match node {
        LayoutNode::Leaf { .. } => (extent.min(minimum), f64::INFINITY),
        LayoutNode::Split {
            direction: node_dir,
            ratio,
            first,
            second,
        } => {
            if *node_dir != direction {
                let (min_a, max_a) =
                    edge_resize_bounds(first, direction, edge, extent, gap, minimum);
                let (min_b, max_b) =
                    edge_resize_bounds(second, direction, edge, extent, gap, minimum);
                return (min_a.max(min_b), max_a.min(max_b));
            }
            let available = extent - gap;
            let first_len = available * *ratio;
            let second_len = available - first_len;
            let (fixed, moving, moving_len) = match edge {
                ResizeEdge::End => (first_len, second.as_ref(), second_len),
                ResizeEdge::Start => (second_len, first.as_ref(), first_len),
            };
            let (sub_min, sub_max) =
                edge_resize_bounds(moving, direction, edge, moving_len, gap, minimum);
            (
                (gap + fixed + sub_min).max(gap + fixed / MAX_SPLIT_RATIO),
                (gap + fixed + sub_max).min(gap + fixed / MIN_SPLIT_RATIO),
            )
        }
    }
}

fn resize_from_edge(
    node: &LayoutNode,
    direction: SplitDirection,
    edge: ResizeEdge,
    extent: f64,
    next_extent: f64,
    gap: f64,
) -> LayoutNode {
    match node {
        LayoutNode::Leaf { .. } => node.clone(),
        LayoutNode::Split {
            direction: node_dir,
            ratio,
            first,
            second,
        } => {
            if *node_dir != direction {
                return LayoutNode::Split {
                    direction: *node_dir,
                    ratio: *ratio,
                    first: Box::new(resize_from_edge(
                        first,
                        direction,
                        edge,
                        extent,
                        next_extent,
                        gap,
                    )),
                    second: Box::new(resize_from_edge(
                        second,
                        direction,
                        edge,
                        extent,
                        next_extent,
                        gap,
                    )),
                };
            }
            let first_len = (extent - gap) * *ratio;
            let second_len = extent - gap - first_len;
            let next_available = next_extent - gap;
            match edge {
                ResizeEdge::End => LayoutNode::Split {
                    direction: *node_dir,
                    ratio: clamp_split_ratio(first_len / next_available),
                    first: first.clone(),
                    second: Box::new(resize_from_edge(
                        second,
                        direction,
                        edge,
                        second_len,
                        next_available - first_len,
                        gap,
                    )),
                },
                ResizeEdge::Start => LayoutNode::Split {
                    direction: *node_dir,
                    ratio: clamp_split_ratio((next_available - second_len) / next_available),
                    first: Box::new(resize_from_edge(
                        first,
                        direction,
                        edge,
                        first_len,
                        next_available - second_len,
                        gap,
                    )),
                    second: second.clone(),
                },
            }
        }
    }
}

pub(crate) fn resize_layout_locally(
    root: &Layout,
    path: &[usize],
    ratio: f64,
    metrics: LayoutMetrics,
) -> Layout {
    let Layout::Node(root_node) = root else {
        return root.clone();
    };
    let Some(LayoutNode::Split {
        direction,
        ratio: current_ratio,
        first: target_first,
        second: target_second,
    }) = split_at_path(root_node, path)
    else {
        return root.clone();
    };
    if !is_valid_extent(metrics.width)
        || !is_valid_extent(metrics.height)
        || !metrics.gap.is_finite()
        || metrics.gap < 0.0
    {
        return root.clone();
    }
    let direction = *direction;
    let gap = metrics.gap;
    let root_extent = if direction == SplitDirection::Row {
        metrics.width
    } else {
        metrics.height
    };
    let mut extent = root_extent;
    let mut cursor = root_node;
    for &step in path {
        let LayoutNode::Split {
            direction: node_dir,
            ratio: node_ratio,
            first,
            second,
        } = cursor
        else {
            return root.clone();
        };
        if *node_dir == direction {
            extent = (extent - gap)
                * if step == 0 {
                    *node_ratio
                } else {
                    1.0 - *node_ratio
                };
        }
        cursor = match step {
            0 => first,
            1 => second,
            _ => return root.clone(),
        };
    }
    let available = extent - gap;
    if !is_valid_extent(available) {
        return root.clone();
    }
    let first_len = available * *current_ratio;
    let second_len = available - first_len;
    let min_module = root_extent * MIN_SPLIT_RATIO;
    let (first_min, first_max) =
        edge_resize_bounds(target_first, direction, ResizeEdge::End, first_len, gap, min_module);
    let (second_min, second_max) = edge_resize_bounds(
        target_second,
        direction,
        ResizeEdge::Start,
        second_len,
        gap,
        min_module,
    );
    let lower = (first_min - first_len).max(second_len - second_max);
    let upper = (first_max - first_len).min(second_len - second_min);
    if lower > upper {
        return root.clone();
    }
    let requested = (clamp_split_ratio(ratio) - *current_ratio) * available;
    let delta = requested.clamp(lower, upper);
    if !delta.is_finite() || delta.abs() < 1e-6 {
        return root.clone();
    }
    let resized = LayoutNode::Split {
        direction,
        ratio: clamp_split_ratio((first_len + delta) / available),
        first: Box::new(resize_from_edge(
            target_first,
            direction,
            ResizeEdge::End,
            first_len,
            first_len + delta,
            gap,
        )),
        second: Box::new(resize_from_edge(
            target_second,
            direction,
            ResizeEdge::Start,
            second_len,
            second_len - delta,
            gap,
        )),
    };
    Layout::Node(replace_node_at_path(root_node, path, resized))
}

#[derive(Clone, Debug)]
pub(crate) struct ModuleLayoutState {
    pub(crate) preferences: UiPreferences,
    pub(crate) rail_side: RailSide,
    pub(crate) scale: u16,
    pub(crate) active_module: Option<ModuleType>,
    pub(crate) maximized: Option<ModuleType>,
    pub(crate) revision: u64,
    pub(crate) error: Option<String>,
}

impl Default for ModuleLayoutState {
    fn default() -> Self {
        let preferences = default_ui_preferences();
        let active_module = default_active_module(&preferences.layout);
        Self {
            preferences,
            rail_side: RailSide::Right,
            scale: appearance::Settings::default().scale,
            active_module,
            maximized: None,
            revision: 0,
            error: None,
        }
    }
}

fn default_active_module(layout: &Layout) -> Option<ModuleType> {
    let ids = flatten_layout(layout);
    if ids.contains(&ModuleType::Editor) {
        Some(ModuleType::Editor)
    } else {
        ids.into_iter().next()
    }
}

impl ModuleLayoutState {
    pub(crate) fn layout(&self) -> &Layout {
        &self.preferences.layout
    }

    pub(crate) fn ensure_editor_visible(&mut self) {
        if !self.has_module(ModuleType::Editor) {
            self.add(ModuleType::Editor);
        }
        self.activate(ModuleType::Editor);
    }

    pub(crate) fn items(&self) -> Vec<ModuleType> {
        flatten_layout(&self.preferences.layout)
    }

    pub(crate) fn has_module(&self, id: ModuleType) -> bool {
        layout_has_module(&self.preferences.layout, id)
    }

    #[allow(dead_code)]
    pub(crate) fn reset_local(&mut self) {
        self.revision += 1;
        self.preferences = default_ui_preferences();
        self.rail_side = RailSide::Right;
        self.scale = appearance::Settings::default().scale;
        self.active_module = default_active_module(&self.preferences.layout);
        self.maximized = None;
        self.error = None;
    }

    pub(crate) fn load_from_workspace(
        &mut self,
        ui_preferences: Option<&CorePreferences>,
        appearance: &AppearanceSettings,
    ) {
        self.revision += 1;
        self.preferences = from_core_preferences(ui_preferences);
        self.rail_side = RailSide::from_setting(&appearance.rail);
        self.scale = appearance.scale;
        self.maximized = None;
        self.active_module = default_active_module(&self.preferences.layout);
        self.error = None;
    }

    fn sync_focus_after_layout_change(&mut self) {
        let ids = self.items();
        if let Some(max) = self.maximized {
            if !ids.contains(&max) {
                self.maximized = None;
            }
        }
        if let Some(active) = self.active_module {
            if !ids.contains(&active) {
                self.active_module = default_active_module(&self.preferences.layout);
            }
        } else {
            self.active_module = default_active_module(&self.preferences.layout);
        }
    }

    pub(crate) fn activate(&mut self, id: ModuleType) {
        if let Some(max) = self.maximized {
            if max != id {
                return;
            }
        }
        if self.has_module(id) {
            self.active_module = Some(id);
        }
    }

    pub(crate) fn toggle_maximize(&mut self, id: ModuleType) {
        if !self.has_module(id) {
            return;
        }
        if self.maximized == Some(id) {
            self.maximized = None;
        } else {
            self.maximized = Some(id);
            self.active_module = Some(id);
        }
    }

    pub(crate) fn restore_maximized(&mut self) -> bool {
        if self.maximized.take().is_some() {
            true
        } else {
            false
        }
    }

    pub(crate) fn add(&mut self, id: ModuleType) -> bool {
        if self.has_module(id) || self.items().len() >= MAX_MODULES {
            return false;
        }
        self.preferences.layout = append_layout_module(&self.preferences.layout, id);
        if self.maximized.is_some() && self.maximized != Some(id) {
            self.maximized = None;
        }
        self.active_module = Some(id);
        self.error = None;
        true
    }

    pub(crate) fn remove(&mut self, id: ModuleType) -> bool {
        if !self.has_module(id) {
            return false;
        }
        self.preferences.layout = remove_layout_module(&self.preferences.layout, id);
        self.sync_focus_after_layout_change();
        self.error = None;
        true
    }

    pub(crate) fn split(&mut self, dragged: ModuleType, target: ModuleType, side: DropSide) -> bool {
        let next = split_layout(&self.preferences.layout, dragged, target, side);
        if next == self.preferences.layout {
            return false;
        }
        self.preferences.layout = next;
        self.active_module = Some(dragged);
        self.error = None;
        true
    }

    pub(crate) fn insert_at_gutter(&mut self, dragged: ModuleType, path: &[usize]) -> bool {
        let next = insert_layout_module_at_gutter(&self.preferences.layout, dragged, path);
        if next == self.preferences.layout {
            return false;
        }
        self.preferences.layout = next;
        self.active_module = Some(dragged);
        self.error = None;
        true
    }

    pub(crate) fn move_to_edge(&mut self, dragged: ModuleType, side: EdgeSide) -> bool {
        let next = move_layout_module_to_edge(&self.preferences.layout, dragged, side);
        if next == self.preferences.layout {
            return false;
        }
        self.preferences.layout = next;
        self.active_module = Some(dragged);
        self.error = None;
        true
    }

    pub(crate) fn resize(&mut self, path: &[usize], ratio: f64, metrics: LayoutMetrics) -> bool {
        let next = resize_layout_locally(&self.preferences.layout, path, ratio, metrics);
        if next == self.preferences.layout {
            return false;
        }
        self.preferences.layout = next;
        self.error = None;
        true
    }

    pub(crate) fn save_layout(&mut self, name: &str) -> bool {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.chars().count() > MAX_LAYOUT_NAME_CHARS {
            return false;
        }
        self.preferences
            .saved_layouts
            .retain(|item| item.name != trimmed);
        self.preferences.saved_layouts.push(SavedLayout {
            name: trimmed.to_owned(),
            layout: self.preferences.layout.clone(),
        });
        if self.preferences.saved_layouts.len() > MAX_SAVED_LAYOUTS {
            self.preferences.saved_layouts.remove(0);
        }
        self.error = None;
        true
    }

    pub(crate) fn apply_saved_layout(&mut self, name: &str) -> bool {
        let Some(saved) = self
            .preferences
            .saved_layouts
            .iter()
            .find(|item| item.name == name)
            .cloned()
        else {
            return false;
        };
        if self.preferences.layout == saved.layout {
            return false;
        }
        self.preferences.layout = saved.layout;
        self.sync_focus_after_layout_change();
        self.error = None;
        true
    }

    pub(crate) fn remove_saved_layout(&mut self, name: &str) -> bool {
        let before = self.preferences.saved_layouts.len();
        self.preferences
            .saved_layouts
            .retain(|item| item.name != name);
        if self.preferences.saved_layouts.len() == before {
            return false;
        }
        self.error = None;
        true
    }

    pub(crate) fn apply_preset(&mut self, id: &str) -> bool {
        let Some(preset) = LAYOUT_PRESETS.iter().find(|preset| preset.id == id) else {
            return false;
        };
        let next = preset.layout();
        if self.preferences.layout == next {
            return false;
        }
        self.preferences.layout = next;
        self.sync_focus_after_layout_change();
        self.error = None;
        true
    }

    pub(crate) fn set_skin(&mut self, skin: &str) -> bool {
        let normalized = normalize_skin(Some(skin));
        if self.preferences.skin == normalized {
            return false;
        }
        self.preferences.skin = normalized;
        self.error = None;
        true
    }

    pub(crate) fn set_rail(&mut self, rail: &str) {
        self.rail_side = if rail == "right" {
            RailSide::Right
        } else {
            RailSide::Left
        };
    }

    pub(crate) fn reset(&mut self) -> bool {
        let next = default_layout();
        if self.preferences.layout == next {
            return false;
        }
        self.preferences.layout = next;
        self.sync_focus_after_layout_change();
        self.error = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn leaf(id: ModuleType) -> LayoutNode {
        LayoutNode::Leaf { id }
    }

    fn split(
        direction: SplitDirection,
        ratio: f64,
        first: LayoutNode,
        second: LayoutNode,
    ) -> LayoutNode {
        LayoutNode::Split {
            direction,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    #[test]
    fn normalizes_defaults_empty_roots_and_legacy_fixture_layouts() {
        assert_eq!(
            normalize_ui_preferences(&json!({"layout":"broken","skin":"nope"})),
            default_ui_preferences()
        );

        let empty = normalize_ui_preferences(&json!({
            "layout": {"type": "empty"},
            "modules": [{"id": "calendar"}],
            "skin": "jade",
            "savedLayouts": [{"name": "空白", "layout": {"type": "empty"}}]
        }));
        assert_eq!(empty.layout, Layout::Empty);
        assert_eq!(empty.skin, "jade");
        assert_eq!(empty.saved_layouts.len(), 1);
        assert_eq!(empty.saved_layouts[0].layout, Layout::Empty);

        let legacy_core: CorePreferences = serde_json::from_str(
            r#"{"axis":"column","order":["calendar","editor","history"],"skin":"jade"}"#,
        )
        .unwrap();
        let migrated_core = workreport_core::ui::normalize(legacy_core);
        let restored = from_core_preferences(Some(&migrated_core));
        assert_eq!(restored.skin, "jade");
        assert_eq!(
            flatten_layout(&restored.layout),
            vec![ModuleType::Calendar, ModuleType::Editor, ModuleType::History]
        );
        let geo = layout_geometry(&restored.layout, 600.0, 912.0, 6.0, None);
        assert_eq!(geo.slots.len(), 3);
        assert!((geo.slots[0].rect.height - 302.0).abs() < 1e-6);
        assert!((geo.slots[1].rect.height - 299.0).abs() < 1e-6);
        assert!((geo.slots[2].rect.height - 299.0).abs() < 1e-6);
    }

    #[test]
    fn merges_legacy_editor_previews_and_clamps_split_ratios() {
        let legacy = normalize_ui_preferences(&json!({
            "layout": {
                "type": "split",
                "direction": "row",
                "ratio": 0.001,
                "first": {"type": "leaf", "id": "editor-9"},
                "second": {"type": "leaf", "id": "editor"}
            }
        }));
        assert_eq!(legacy.layout, Layout::Node(leaf(ModuleType::Editor)));

        let merged = normalize_ui_preferences(&json!({
            "layout": {
                "type": "split",
                "direction": "row",
                "ratio": 5,
                "first": {"type": "leaf", "id": "editor-3"},
                "second": {"type": "leaf", "id": "calendar"}
            }
        }));
        assert_eq!(
            merged.layout,
            Layout::Node(split(
                SplitDirection::Row,
                MAX_SPLIT_RATIO,
                leaf(ModuleType::Editor),
                leaf(ModuleType::Calendar),
            ))
        );
    }

    #[test]
    fn splits_inserts_moves_to_edge_and_resizes_locally_without_moving_far_modules() {
        let mut state = ModuleLayoutState::default();
        assert!(state.add(ModuleType::Overview));
        assert_eq!(
            state.items(),
            vec![ModuleType::Calendar, ModuleType::Editor, ModuleType::Overview]
        );
        assert!(state.split(ModuleType::Overview, ModuleType::Calendar, DropSide::Bottom));
        assert_eq!(
            state.items(),
            vec![ModuleType::Calendar, ModuleType::Overview, ModuleType::Editor]
        );
        assert!(state.move_to_edge(ModuleType::Editor, EdgeSide::Left));
        assert_eq!(
            state.items(),
            vec![ModuleType::Editor, ModuleType::Calendar, ModuleType::Overview]
        );

        let three_col = Layout::Node(split(
            SplitDirection::Row,
            0.5,
            leaf(ModuleType::Overview),
            split(
                SplitDirection::Row,
                0.5,
                leaf(ModuleType::History),
                leaf(ModuleType::Calendar),
            ),
        ));
        let inserted = insert_layout_module_at_gutter(&three_col, ModuleType::Overview, &[1]);
        assert_eq!(
            flatten_layout(&inserted),
            vec![ModuleType::History, ModuleType::Overview, ModuleType::Calendar]
        );

        let complex = Layout::Node(split(
            SplitDirection::Row,
            0.3,
            leaf(ModuleType::Calendar),
            split(
                SplitDirection::Row,
                0.6,
                split(
                    SplitDirection::Column,
                    0.5,
                    leaf(ModuleType::Editor),
                    leaf(ModuleType::History),
                ),
                leaf(ModuleType::Overview),
            ),
        ));
        let metrics = LayoutMetrics {
            width: 1000.0,
            height: 900.0,
            gap: 6.0,
        };
        let far_before = layout_geometry(&complex, 1000.0, 900.0, 6.0, None)
            .slots
            .into_iter()
            .find(|slot| slot.id == ModuleType::Overview)
            .unwrap()
            .rect;
        let resized = resize_layout_locally(&complex, &[], 0.6, metrics);
        let far_after = layout_geometry(&resized, 1000.0, 900.0, 6.0, None)
            .slots
            .into_iter()
            .find(|slot| slot.id == ModuleType::Overview)
            .unwrap()
            .rect;
        assert!((far_before.left - far_after.left).abs() < 1e-6);
        assert!((far_before.width - far_after.width).abs() < 1e-6);
        assert!((far_before.top - far_after.top).abs() < 1e-6);
        assert!((far_before.height - far_after.height).abs() < 1e-6);
    }

    #[test]
    fn saved_layouts_presets_and_round_trip_to_core_preferences() {
        let mut state = ModuleLayoutState::default();
        assert!(state.save_layout("日常布局"));
        assert!(!state.save_layout("   "));
        assert!(state.remove(ModuleType::Calendar));
        assert!(state.remove(ModuleType::Editor));
        assert_eq!(*state.layout(), Layout::Empty);
        assert!(state.save_layout("空白"));
        assert!(state.apply_preset("compact-writing"));
        assert_eq!(
            state.items(),
            vec![ModuleType::Calendar, ModuleType::Overview, ModuleType::Editor]
        );
        assert!(state.set_skin("jade"));

        let core = to_core_preferences(&state.preferences);
        let round_trip = from_core_preferences(Some(&core));
        assert_eq!(round_trip, state.preferences);
        assert!(state.apply_saved_layout("空白"));
        assert_eq!(*state.layout(), Layout::Empty);
        assert!(state.remove_saved_layout("空白"));
        assert!(state.apply_saved_layout("日常布局"));
        assert_eq!(
            state.items(),
            vec![ModuleType::Calendar, ModuleType::Editor]
        );
    }
}
