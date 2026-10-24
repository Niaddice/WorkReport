use serde::{Deserialize, Serialize};

/// 主界面的排列方向：左右分栏或上下排列。
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    #[default]
    Row,
    Column,
}

/// 可以拖动重排的两个模块。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Calendar,
    Editor,
}

/// 纯界面偏好：模块排列与皮肤。与业务设置分开保存，互不校验。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub axis: Axis,
    pub order: Vec<Module>,
    /// 当前皮肤 id；目前只有默认皮肤。
    pub skin: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            axis: Axis::Row,
            order: vec![Module::Calendar, Module::Editor],
            skin: DEFAULT_SKIN.into(),
        }
    }
}

pub const DEFAULT_SKIN: &str = "default";

/// 排序必须包含日历和编辑区各一次；未知皮肤回落默认值。
pub fn normalize(mut preferences: Preferences) -> Preferences {
    let valid = preferences.order.len() == 2
        && preferences.order.contains(&Module::Calendar)
        && preferences.order.contains(&Module::Editor);
    if !valid {
        preferences.order = vec![Module::Calendar, Module::Editor];
        preferences.axis = Axis::Row;
    }
    if preferences.skin != DEFAULT_SKIN {
        preferences.skin = DEFAULT_SKIN.into();
    }
    preferences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_a_row_with_calendar_first_and_the_default_skin() {
        let restored: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(restored, Preferences::default());
        assert_eq!(restored.axis, Axis::Row);
        assert_eq!(restored.skin, DEFAULT_SKIN);
    }

    #[test]
    fn accepts_both_orders_and_keeps_the_skin() {
        let preferences: Preferences =
            serde_json::from_str(r#"{"axis":"column","order":["editor","calendar"],"skin":"default"}"#).unwrap();
        assert_eq!(preferences.axis, Axis::Column);
        assert_eq!(normalize(preferences.clone()), preferences);
    }

    #[test]
    fn falls_back_for_incomplete_orders_and_unknown_skins() {
        let preferences: Preferences =
            serde_json::from_str(r#"{"order":["calendar"],"skin":"neon"}"#).unwrap();
        let normalized = normalize(preferences);
        assert_eq!(normalized.order, vec![Module::Calendar, Module::Editor]);
        assert_eq!(normalized.axis, Axis::Row);
        assert_eq!(normalized.skin, DEFAULT_SKIN);
    }
}
