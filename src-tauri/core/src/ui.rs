use serde::{Deserialize, Serialize};

/// 旧版主界面的排列方向：左右分栏或上下排列。仅用于读取旧文件。
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    #[default]
    Row,
    Column,
}

/// 默认皮肤 id；当前版本只有默认皮肤。
pub const DEFAULT_SKIN: &str = "default";

/// 默认栏数：两列。单列是唯一允许的另一种取值。
pub const DEFAULT_COLUMNS: u8 = 2;

const SINGLE_COLUMN: u8 = 1;

/// 模块数量上限，避免异常文件把首页撑爆。
const MAX_MODULES: usize = 12;

/// 安全 id 的最大长度：小写字母开头，后接最多 31 个字符。
const MAX_ID_LENGTH: usize = 32;

/// 保存布局的数量上限与名称长度上限（按字符数）。
const MAX_SAVED_LAYOUTS: usize = 20;
const MAX_LAYOUT_NAME_CHARS: usize = 50;

/// 单个模块在布局中的位置。旧文件里的 `span` 字段会在读取时被忽略。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    #[serde(default)]
    pub id: String,
}

/// 用户保存的布局快照：名字 + 布局树 JSON。名字只做非空与长度校验，布局树由前端解释。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    pub name: String,
    pub layout: serde_json::Value,
}

/// 纯界面偏好：可嵌套分栏布局与皮肤。与业务设置分开保存，互不校验。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub columns: u8,
    #[serde(default)]
    pub modules: Vec<Placement>,
    #[serde(default)]
    pub skin: String,
    /// 新版前端的嵌套分栏布局树（LayoutNode）。Rust 只做原样透传，不解释结构，
    /// 因此未知或非法节点不会导致反序列化失败；为 None 时省略序列化，
    /// 保证旧文件的精确 JSON 不变。默认树由前端负责生成，这里保持 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<serde_json::Value>,
    /// 用户保存的布局快照，随界面偏好一起保存；空时省略序列化。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_layouts: Vec<SavedLayout>,
    /// 旧版固定两模块布局的方向：只用于读取旧文件，迁移后不再写回。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<Axis>,
    /// 旧版模块顺序：只用于读取旧文件，迁移后不再写回。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            columns: DEFAULT_COLUMNS,
            modules: default_modules(),
            skin: DEFAULT_SKIN.into(),
            layout: None,
            saved_layouts: Vec::new(),
            axis: None,
            order: Vec::new(),
        }
    }
}

fn default_modules() -> Vec<Placement> {
    ["calendar", "editor"]
        .into_iter()
        .map(|id| Placement { id: id.into() })
        .collect()
}

/// 模块与皮肤 id 共用同一套安全规则：小写字母开头，后接小写字母、数字或连字符，最长 32 个字符。
fn is_safe_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_LENGTH {
        return false;
    }
    let mut bytes = id.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// 校验模块 id；不合法时返回 None，由调用方丢弃该模块。
fn placement(id: &str) -> Option<Placement> {
    if !is_safe_id(id) {
        return None;
    }
    Some(Placement { id: id.into() })
}

/// 过滤不安全的模块 id、按首次出现去重，并最多保留 12 个模块。
fn sanitize_modules(modules: Vec<Placement>) -> Vec<Placement> {
    let mut sanitized: Vec<Placement> = Vec::with_capacity(modules.len().min(MAX_MODULES));
    for module in modules {
        if !is_safe_id(&module.id) || sanitized.iter().any(|kept| kept.id == module.id) {
            continue;
        }
        sanitized.push(module);
        if sanitized.len() == MAX_MODULES {
            break;
        }
    }
    sanitized
}

/// 校验并补全旧字段：旧文件迁移、栏数/模块/皮肤回退都在这里完成，返回的值可直接写回磁盘。
/// 新版 `layout` 树由前端负责校验，这里原样保留，不生成也不用旧字段覆盖。
pub fn normalize(mut preferences: Preferences) -> Preferences {
    // 旧版文件没有 modules：按 axis 推导栏数，按 order 迁移出模块顺序。
    if preferences.modules.is_empty() {
        if let Some(axis) = preferences.axis {
            preferences.columns = if axis == Axis::Column { SINGLE_COLUMN } else { DEFAULT_COLUMNS };
        }
        preferences.modules = preferences.order.iter().filter_map(|id| placement(id)).collect();
    }
    // 栏数只支持单列或双列，其余回退双列。
    if preferences.columns != SINGLE_COLUMN {
        preferences.columns = DEFAULT_COLUMNS;
    }
    preferences.modules = sanitize_modules(preferences.modules);
    // 过滤后可能没有可用模块，此时回退到默认布局。
    if preferences.modules.is_empty() {
        preferences.modules = default_modules();
    }
    // 皮肤与模块 id 使用同一安全规则；未知皮肤回退默认值。
    if !is_safe_id(&preferences.skin) {
        preferences.skin = DEFAULT_SKIN.into();
    }
    // layout 是前端负责的兼容载荷：既不解释结构，也不由旧 columns/modules 推断或覆盖。
    // 旧字段只在读取时使用，之后不再保留，保证新写入的文件只有新 schema。
    preferences.axis = None;
    preferences.order.clear();
    preferences.saved_layouts = sanitize_saved_layouts(preferences.saved_layouts);
    preferences
}

/// 过滤空名、超长名与空布局的保存项，最多保留 MAX_SAVED_LAYOUTS 个。
fn sanitize_saved_layouts(saved: Vec<SavedLayout>) -> Vec<SavedLayout> {
    let mut kept: Vec<SavedLayout> = Vec::with_capacity(saved.len().min(MAX_SAVED_LAYOUTS));
    for item in saved {
        let name = item.name.trim();
        if name.is_empty() || name.chars().count() > MAX_LAYOUT_NAME_CHARS {
            continue;
        }
        if item.layout.is_null() {
            continue;
        }
        kept.push(SavedLayout { name: name.to_string(), layout: item.layout });
        if kept.len() == MAX_SAVED_LAYOUTS {
            break;
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn placement(id: &str) -> Placement {
        Placement { id: id.into() }
    }

    fn modules(ids: &[&str]) -> Vec<Placement> {
        ids.iter().map(|id| placement(id)).collect()
    }

    /// 与前端 LayoutNode 一致的嵌套分栏树：row 根节点，第二栏再上下分栏。
    fn sample_layout() -> serde_json::Value {
        json!({
            "type": "split",
            "direction": "row",
            "ratio": 0.5,
            "first": {"type": "leaf", "id": "calendar"},
            "second": {
                "type": "split",
                "direction": "column",
                "ratio": 0.4,
                "first": {"type": "leaf", "id": "editor"},
                "second": {"type": "leaf", "id": "overview"},
            },
        })
    }

    #[test]
    fn defaults_to_two_columns_with_calendar_then_editor() {
        let restored: Preferences = serde_json::from_str("{}").unwrap();
        let normalized = normalize(restored);
        assert_eq!(normalized, Preferences::default());
        assert_eq!(normalized.columns, 2);
        assert_eq!(normalized.modules, modules(&["calendar", "editor"]));
        assert_eq!(normalized.skin, DEFAULT_SKIN);
    }

    #[test]
    fn serializes_only_the_new_schema_without_legacy_or_span_fields() {
        // 旧文件同时带 axis/order 与已废弃的 span，读取后都应被迁移/忽略。
        let legacy: Preferences = serde_json::from_str(
            r#"{"axis":"column","order":["editor","calendar"],"skin":"default"}"#,
        )
        .unwrap();
        let normalized = normalize(legacy);
        assert_eq!(
            serde_json::to_value(&normalized).unwrap(),
            json!({
                "columns": 1,
                "modules": [{"id": "editor"}, {"id": "calendar"}],
                "skin": "default",
            })
        );
        let text = serde_json::to_string(&normalized).unwrap();
        assert_eq!(
            text,
            r#"{"columns":1,"modules":[{"id":"editor"},{"id":"calendar"}],"skin":"default"}"#
        );
        assert!(!text.contains("axis") && !text.contains("order") && !text.contains("span"));
    }

    #[test]
    fn omits_the_layout_field_until_the_frontend_provides_one() {
        // 默认布局树由前端生成，Rust 侧保持 None，序列化时省略 layout。
        assert!(Preferences::default().layout.is_none());
        assert_eq!(
            serde_json::to_string(&Preferences::default()).unwrap(),
            r#"{"columns":2,"modules":[{"id":"calendar"},{"id":"editor"}],"skin":"default"}"#
        );
        // 旧文件迁移后同样没有 layout，精确 JSON 与旧版本保持一致。
        let migrated = normalize(
            serde_json::from_str(r#"{"axis":"row","order":["calendar","editor"],"skin":"default"}"#)
                .unwrap(),
        );
        assert!(migrated.layout.is_none());
        assert!(!serde_json::to_string(&migrated).unwrap().contains("layout"));
    }

    #[test]
    fn round_trips_the_layout_tree_through_saved_json() {
        let layout = sample_layout();
        let preferences: Preferences =
            serde_json::from_value(json!({"skin": "default", "layout": layout.clone()})).unwrap();
        assert_eq!(preferences.layout, Some(layout.clone()));

        // 新 schema 只带 layout/skin：旧字段按原规则回填，layout 原样保留。
        let normalized = normalize(preferences);
        assert_eq!(normalized.layout, Some(layout.clone()));
        assert_eq!(normalized.columns, DEFAULT_COLUMNS);
        assert_eq!(normalized.modules, Preferences::default().modules);

        // 写盘 JSON 再读回，嵌套树逐字段不变。
        let saved = serde_json::to_string(&normalized).unwrap();
        let restored: Preferences = serde_json::from_str(&saved).unwrap();
        let renormalized = normalize(restored);
        assert_eq!(renormalized, normalized);
        assert_eq!(serde_json::to_value(&renormalized).unwrap()["layout"], layout);
    }

    #[test]
    fn keeps_layout_when_legacy_columns_and_modules_are_also_present() {
        let preferences: Preferences = serde_json::from_str(
            r#"{"columns":1,"modules":[{"id":"editor"},{"id":"calendar"}],
                "layout":{"type":"leaf","id":"overview"},"skin":"default"}"#,
        )
        .unwrap();
        let normalized = normalize(preferences);
        let layout = json!({"type": "leaf", "id": "overview"});
        assert_eq!(normalized.layout, Some(layout.clone()));
        // 旧字段仍按原规则归一化，但不会反过来推断/覆盖 layout。
        assert_eq!(normalized.columns, SINGLE_COLUMN);
        assert_eq!(normalized.modules, modules(&["editor", "calendar"]));
        assert_eq!(serde_json::to_value(&normalized).unwrap()["layout"], layout);
    }

    #[test]
    fn passes_unknown_layout_values_through_without_panicking() {
        // layout 是 serde_json::Value 透传：前端未来新增的节点类型也能存回去。
        let unknown = [
            json!("garbage"),
            json!(42),
            json!([]),
            json!({"type": "unknown", "children": [null], "ratio": "nope"}),
        ];
        for layout in unknown {
            let preferences: Preferences =
                serde_json::from_value(json!({"skin": "default", "layout": layout.clone()})).unwrap();
            let normalized = normalize(preferences);
            assert_eq!(normalized.layout, Some(layout.clone()));
            let saved = serde_json::to_string(&normalized).unwrap();
            let restored: Preferences = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored.layout, Some(layout));
        }
        // JSON null 按“未提供”处理，不 panic，也不会把 layout 写回文件。
        let null: Preferences = serde_json::from_str(r#"{"skin":"default","layout":null}"#).unwrap();
        assert!(null.layout.is_none());
        assert!(!serde_json::to_string(&normalize(null)).unwrap().contains("layout"));
    }

    #[test]
    fn migrates_legacy_row_and_column_layouts_keeping_the_order() {
        let row = normalize(
            serde_json::from_str(r#"{"axis":"row","order":["calendar","editor"],"skin":"default"}"#)
                .unwrap(),
        );
        assert_eq!(row.columns, 2);
        assert_eq!(row.modules, modules(&["calendar", "editor"]));

        let column = normalize(
            serde_json::from_str(r#"{"axis":"column","order":["editor","calendar"],"skin":"default"}"#)
                .unwrap(),
        );
        assert_eq!(column.columns, 1);
        assert_eq!(column.modules, modules(&["editor", "calendar"]));

        // 没有 axis 的旧文件按两列迁移；非法与重复的 id 会被过滤。
        let without_axis =
            normalize(serde_json::from_str(r#"{"order":["editor","Bad","calendar","editor"]}"#).unwrap());
        assert_eq!(without_axis.columns, 2);
        assert_eq!(without_axis.modules, modules(&["editor", "calendar"]));
    }

    #[test]
    fn filters_unsafe_and_duplicate_modules_and_ignores_unknown_fields() {
        let preferences: Preferences = serde_json::from_str(
            r#"{
                "columns": 3,
                "modules": [
                    {"id": "calendar", "span": "full"},
                    {"id": "calendar", "span": "half"},
                    {"id": "Calendar", "span": "full"},
                    {"id": "editor", "span": "wide"},
                    {"id": "overview"},
                    {"id": "-bad", "span": "full"},
                    {"id": "has space", "span": "full"},
                    {"id": "9lives", "span": "full"},
                    {"span": "full"}
                ],
                "skin": "Neon!"
            }"#,
        )
        .unwrap();
        let normalized = normalize(preferences);
        assert_eq!(normalized.columns, 2);
        assert_eq!(normalized.modules, modules(&["calendar", "editor", "overview"]));
        assert_eq!(normalized.skin, DEFAULT_SKIN);
        assert_eq!(normalized.modules[0], placement("calendar"));
    }

    #[test]
    fn falls_back_for_unsupported_columns_and_keeps_safe_skin_ids() {
        // 安全的自定义皮肤原样保留，未知皮肤回退默认值。
        let custom = normalize(
            serde_json::from_str(r#"{"columns":0,"modules":[{"id":"calendar"}],"skin":"midnight"}"#)
                .unwrap(),
        );
        assert_eq!(custom.columns, 2);
        assert_eq!(custom.skin, "midnight");
        for unsafe_skin in ["", "Neon", "neon!", "neon skin"] {
            let preferences = Preferences { skin: unsafe_skin.into(), ..Preferences::default() };
            assert_eq!(normalize(preferences).skin, DEFAULT_SKIN);
        }
        // 单列是唯一允许的非默认栏数。
        assert_eq!(normalize(Preferences { columns: 1, ..Preferences::default() }).columns, 1);
        for invalid_columns in [0, 3, 255] {
            let preferences = Preferences { columns: invalid_columns, ..Preferences::default() };
            assert_eq!(normalize(preferences).columns, DEFAULT_COLUMNS);
        }
    }

    #[test]
    fn keeps_at_most_twelve_modules_in_their_original_order() {
        let modules: Vec<serde_json::Value> = (0..13).map(|index| json!({ "id": format!("module-{index}") })).collect();
        let preferences: Preferences =
            serde_json::from_value(json!({ "columns": 2, "modules": modules, "skin": "default" })).unwrap();
        let normalized = normalize(preferences);
        assert_eq!(normalized.modules.len(), 12);
        let ids: Vec<&str> = normalized.modules.iter().map(|module| module.id.as_str()).collect();
        assert_eq!(
            ids,
            (0..12).map(|index| format!("module-{index}")).collect::<Vec<_>>()
        );
    }

    #[test]
    fn falls_back_to_the_default_layout_when_every_module_is_invalid() {
        let preferences: Preferences = serde_json::from_str(
            r#"{"columns":1,"modules":[{"id":"Bad Id!"},{"id":"9lives"}],"skin":"default"}"#,
        )
        .unwrap();
        let normalized = normalize(preferences);
        assert_eq!(normalized.columns, 1);
        assert_eq!(normalized.modules, Preferences::default().modules);
    }
}
