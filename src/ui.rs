use crate::{Bounds, DriverError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// `uitest dumpLayout` 返回的一个 UI 节点。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiNode {
    #[serde(default)]
    /// 节点的标准属性键值对。
    pub attributes: BTreeMap<String, Value>,
    #[serde(default)]
    /// 子节点列表。
    pub children: Vec<UiNode>,
    #[serde(flatten)]
    /// 额外的非标准属性（展平到同一层级）。
    pub extra: BTreeMap<String, Value>,
}

impl UiNode {
    /// 读取节点属性，优先使用 `attributes` 对象。
    pub fn attribute(&self, name: &str) -> Option<String> {
        self.attributes
            .get(name)
            .or_else(|| self.extra.get(name))
            .and_then(value_to_string)
    }

    /// 取得控件类型。
    pub fn node_type(&self) -> Option<String> {
        self.attribute("type")
    }

    /// 解析节点 bounds。
    pub fn bounds(&self) -> Option<Bounds> {
        self.attributes
            .get("bounds")
            .or_else(|| self.extra.get("bounds"))
            .and_then(Bounds::parse_value)
    }

    /// 将 `uitest dumpLayout` 的原始 JSON（可能带有 `root` 包装层）解析为 [`UiNode`]。
    ///
    /// 供在不通过 [`crate::HmDriver::ui_tree`] 的情况下（例如自行用 `raw_shell`/
    /// `pull_file` 取回 dump 文件）复用同样的解析逻辑。
    pub fn from_layout_json(value: Value) -> Result<UiNode> {
        let root = if let Some(root) = value.get("root") {
            root.clone()
        } else {
            value
        };
        serde_json::from_value(root).map_err(DriverError::Json)
    }

    pub(crate) fn attribute_snapshot(&self) -> BTreeMap<String, String> {
        let mut result = BTreeMap::new();
        for (key, value) in self.extra.iter().chain(self.attributes.iter()) {
            if key != "children"
                && key != "attributes"
                && let Some(value) = value_to_string(value)
            {
                result.insert(key.clone(), sanitize_xml_text(&value));
            }
        }
        result
    }
}

pub(crate) fn sanitize_xml_text(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            matches!(*ch, '\u{9}' | '\u{a}' | '\u{d}')
                || (*ch >= '\u{20}' && *ch != '\u{fffe}' && *ch != '\u{ffff}')
        })
        .collect()
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => Some(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_common_bounds_forms() {
        assert_eq!(
            Bounds::parse_value(&json!("[1,2][30,40]")),
            Some(Bounds {
                left: 1,
                top: 2,
                right: 30,
                bottom: 40
            })
        );
        assert_eq!(
            Bounds::parse_value(&json!({"left": 1, "top": 2, "right": 30, "bottom": 40})),
            Some(Bounds {
                left: 1,
                top: 2,
                right: 30,
                bottom: 40
            })
        );
    }

    #[test]
    fn removes_only_xml_incompatible_characters() {
        assert_eq!(sanitize_xml_text("中\u{0}文\n"), "中文\n");
    }
}
