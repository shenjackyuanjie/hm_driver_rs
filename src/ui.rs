use crate::{Bounds, DriverError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use tracing::trace;

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
    /// 在 UI 树中深度优先搜索第一个满足 `predicate` 的节点。
    ///
    /// 返回 `None` 表示未找到。
    pub fn find(&self, predicate: impl Fn(&UiNode) -> bool) -> Option<&UiNode> {
        trace!(target: "hm_driver_rs::ui", "查找节点");
        self.find_ref(&predicate)
    }

    fn find_ref(&self, predicate: &impl Fn(&UiNode) -> bool) -> Option<&UiNode> {
        if predicate(self) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_ref(predicate) {
                return Some(found);
            }
        }
        None
    }

    /// 在 UI 树中深度优先搜索所有满足 `predicate` 的节点。
    pub fn find_all(&self, predicate: impl Fn(&UiNode) -> bool) -> Vec<&UiNode> {
        trace!(target: "hm_driver_rs::ui", "查找所有匹配节点");
        let mut result = Vec::new();
        self.collect_all(&predicate, &mut result);
        result
    }

    /// 使用与远端查询相同的 [`crate::Selector`] 在当前 UI 树快照中查找节点。
    ///
    /// 本地查询支持字符串（含正则）、布尔属性和 `in_window` 条件；
    /// `before`/`after`/`within` 仍应使用远端查询。
    pub fn find_by_selector(&self, selector: &crate::Selector) -> Result<Option<&UiNode>> {
        let matches = self.find_all_by_selector(selector)?;
        Ok(matches.into_iter().nth(selector.selected_index()))
    }

    /// 使用 Selector 查找当前 UI 树快照中的全部匹配节点。
    pub fn find_all_by_selector(&self, selector: &crate::Selector) -> Result<Vec<&UiNode>> {
        let mut result = Vec::new();
        self.collect_selector(selector, &mut result)?;
        Ok(result)
    }

    fn collect_selector<'a>(
        &'a self,
        selector: &crate::Selector,
        result: &mut Vec<&'a UiNode>,
    ) -> Result<()> {
        if selector.matches_node(self)? {
            result.push(self);
        }
        for child in &self.children {
            child.collect_selector(selector, result)?;
        }
        Ok(())
    }

    /// 按从根节点开始的子节点索引路径读取节点。
    pub fn at_hierarchy(&self, hierarchy: &[usize]) -> Option<&UiNode> {
        let mut current = self;
        for &index in hierarchy {
            current = current.children.get(index)?;
        }
        Some(current)
    }

    /// 按 `/0/1/2` 形式的子节点索引路径读取节点。
    pub fn at_hierarchy_path(&self, path: &str) -> Result<Option<&UiNode>> {
        let hierarchy = parse_hierarchy_path(path)?;
        Ok(self.at_hierarchy(&hierarchy))
    }

    /// 按 UiViewer 风格的类型路径读取节点，例如 `/root[2]/Column/Flex/Text[2]`。
    ///
    /// `[n]` 表示同类型子节点中的第 `n` 个，省略时等价于 `[0]`。
    pub fn at_type_path(&self, path: &str) -> Result<Option<&UiNode>> {
        let mut current = self;
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            let (node_type, occurrence) = parse_type_segment(segment)?;
            current = match current
                .children
                .iter()
                .filter(|child| child.node_type().as_deref() == Some(node_type))
                .nth(occurrence)
            {
                Some(node) => node,
                None => return Ok(None),
            };
        }
        Ok(Some(current))
    }

    /// [`at_type_path`](Self::at_type_path) 的 Hypium 兼容命名。
    pub fn at_abspath(&self, path: &str) -> Result<Option<&UiNode>> {
        self.at_type_path(path)
    }

    /// 查找第一个匹配节点，并返回其层级索引路径。
    pub fn find_hierarchy(
        &self,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Option<(&UiNode, Vec<usize>)> {
        let mut path = Vec::new();
        self.find_hierarchy_ref(&predicate, &mut path)
    }

    fn find_hierarchy_ref<'a>(
        &'a self,
        predicate: &impl Fn(&UiNode) -> bool,
        path: &mut Vec<usize>,
    ) -> Option<(&'a UiNode, Vec<usize>)> {
        if predicate(self) {
            return Some((self, path.clone()));
        }
        for (index, child) in self.children.iter().enumerate() {
            path.push(index);
            if let Some(found) = child.find_hierarchy_ref(predicate, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }

    /// 从指定层级路径按相对路径移动。`..` 表示父节点，数字表示子节点索引。
    pub fn relative_from(
        &self,
        hierarchy: &[usize],
        relative_path: &str,
    ) -> Result<Option<&UiNode>> {
        let mut target = hierarchy.to_vec();
        for segment in relative_path
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
        {
            if segment == ".." {
                if target.pop().is_none() {
                    return Ok(None);
                }
            } else {
                target.push(segment.parse::<usize>().map_err(|_| {
                    DriverError::InvalidArgument(format!("无效相对路径片段：{segment}"))
                })?);
            }
        }
        Ok(self.at_hierarchy(&target))
    }

    /// 查找锚点后按相对路径读取目标节点。
    pub fn find_relative(
        &self,
        predicate: impl Fn(&UiNode) -> bool,
        relative_path: &str,
    ) -> Result<Option<&UiNode>> {
        let Some((_, hierarchy)) = self.find_hierarchy(predicate) else {
            return Ok(None);
        };
        self.relative_from(&hierarchy, relative_path)
    }

    /// [`find_relative`](Self::find_relative) 的 Hypium 兼容命名。
    pub fn find_by_relative_path(
        &self,
        predicate: impl Fn(&UiNode) -> bool,
        relative_path: &str,
    ) -> Result<Option<&UiNode>> {
        self.find_relative(predicate, relative_path)
    }

    /// 将 UI 树快照保存为格式化 JSON。
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self).map_err(DriverError::Json)
    }

    /// 从 JSON 文件加载 UI 树快照。
    ///
    /// 同时接受直接根节点和 `{ "root": ... }` 包装格式。
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        Self::from_layout_json(serde_json::from_reader(file)?)
    }

    fn collect_all<'a>(
        &'a self,
        predicate: &impl Fn(&UiNode) -> bool,
        result: &mut Vec<&'a UiNode>,
    ) {
        if predicate(self) {
            result.push(self);
        }
        for child in &self.children {
            child.collect_all(predicate, result);
        }
    }

    /// 将 `uitest dumpLayout` 的原始 JSON（可能带有 `root` 包装层）解析为 [`UiNode`]。
    ///
    /// 供在不通过 [`crate::HmDriver::ui_tree`] 的情况下（例如自行用 `raw_shell`/
    /// `pull_file` 取回 dump 文件）复用同样的解析逻辑。
    pub fn from_layout_json(value: Value) -> Result<UiNode> {
        trace!(target: "hm_driver_rs::ui", "解析布局 JSON");
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

fn parse_hierarchy_path(path: &str) -> Result<Vec<usize>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            segment
                .parse::<usize>()
                .map_err(|_| DriverError::InvalidArgument(format!("无效层级路径片段：{segment}")))
        })
        .collect()
}

fn parse_type_segment(segment: &str) -> Result<(&str, usize)> {
    if let Some(prefix) = segment.strip_suffix(']')
        && let Some((node_type, index)) = prefix.rsplit_once('[')
    {
        if node_type.is_empty() {
            return Err(DriverError::InvalidArgument("类型路径缺少节点类型".into()));
        }
        let index = index
            .parse::<usize>()
            .map_err(|_| DriverError::InvalidArgument(format!("无效类型路径索引：{segment}")))?;
        return Ok((node_type, index));
    }
    if segment.contains(['[', ']']) || segment.is_empty() {
        return Err(DriverError::InvalidArgument(format!(
            "无效类型路径片段：{segment}"
        )));
    }
    Ok((segment, 0))
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

    #[test]
    fn supports_hierarchy_type_and_relative_paths() {
        let root: UiNode = serde_json::from_value(json!({
            "attributes": {"type": "Root"},
            "children": [
                {"attributes": {"type": "Column"}, "children": [
                    {"attributes": {"type": "Text", "text": "first"}, "children": []},
                    {"attributes": {"type": "Text", "text": "second"}, "children": []}
                ]}
            ]
        }))
        .unwrap();
        assert_eq!(
            root.at_hierarchy_path("/0/1")
                .unwrap()
                .unwrap()
                .attribute("text")
                .as_deref(),
            Some("second")
        );
        assert_eq!(
            root.at_type_path("/Column/Text[1]")
                .unwrap()
                .unwrap()
                .attribute("text")
                .as_deref(),
            Some("second")
        );
        assert_eq!(
            root.find_relative(
                |node| node.attribute("text").as_deref() == Some("first"),
                "../1"
            )
            .unwrap()
            .unwrap()
            .attribute("text")
            .as_deref(),
            Some("second")
        );
    }

    #[test]
    fn local_selector_supports_regular_expressions() {
        let root: UiNode = serde_json::from_value(json!({
            "attributes": {"type": "Root"},
            "children": [
                {"attributes": {"type": "Text", "text": "设置 123"}, "children": []}
            ]
        }))
        .unwrap();
        let selector =
            crate::Selector::new().text(crate::MatchPattern::Regex(r"设置\s+\d+".into()));
        assert_eq!(
            root.find_by_selector(&selector)
                .unwrap()
                .unwrap()
                .attribute("text")
                .as_deref(),
            Some("设置 123")
        );
    }

    #[test]
    fn saves_and_loads_tree_snapshot() {
        let root: UiNode = serde_json::from_value(json!({
            "attributes": {"type": "Root", "text": "离线快照"},
            "children": []
        }))
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tree.json");
        root.save_json(&path).unwrap();
        let loaded = UiNode::load_json(&path).unwrap();
        assert_eq!(loaded.attribute("text").as_deref(), Some("离线快照"));
    }
}
