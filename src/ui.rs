use crate::{Bounds, DriverError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// `uitest dumpLayout` 返回的一个 UI 节点。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiNode {
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    #[serde(default)]
    pub children: Vec<UiNode>,
    #[serde(flatten)]
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
            .and_then(parse_bounds_value)
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

pub(crate) fn parse_layout(value: Value) -> Result<UiNode> {
    let root = if let Some(root) = value.get("root") {
        root.clone()
    } else {
        value
    };
    serde_json::from_value(root).map_err(DriverError::Json)
}

pub(crate) fn parse_bounds_value(value: &Value) -> Option<Bounds> {
    match value {
        Value::Object(object) => {
            let integer = |name: &str| {
                object
                    .get(name)?
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
            };
            Some(Bounds {
                left: integer("left")?,
                top: integer("top")?,
                right: integer("right")?,
                bottom: integer("bottom")?,
            })
        }
        Value::Array(values) if values.len() == 4 => {
            let mut numbers = [0_i32; 4];
            for (index, value) in values.iter().enumerate() {
                numbers[index] = i32::try_from(value.as_i64()?).ok()?;
            }
            Some(Bounds {
                left: numbers[0],
                top: numbers[1],
                right: numbers[2],
                bottom: numbers[3],
            })
        }
        Value::String(value) => parse_bounds_text(value),
        _ => None,
    }
    .filter(|bounds| bounds.is_valid())
}

pub(crate) fn parse_bounds_text(value: &str) -> Option<Bounds> {
    let numbers: Vec<i32> = value
        .split(|ch: char| !ch.is_ascii_digit() && ch != '-')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    if numbers.len() != 4 {
        return None;
    }
    let bounds = Bounds {
        left: numbers[0],
        top: numbers[1],
        right: numbers[2],
        bottom: numbers[3],
    };
    bounds.is_valid().then_some(bounds)
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
            parse_bounds_value(&json!("[1,2][30,40]")),
            Some(Bounds {
                left: 1,
                top: 2,
                right: 30,
                bottom: 40
            })
        );
        assert_eq!(
            parse_bounds_value(&json!({"left": 1, "top": 2, "right": 30, "bottom": 40})),
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
