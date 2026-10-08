//! UI 布局快照解析与主机查询。
//!
//! [`UiNode`] 保存 `dumpLayout` 的属性、子节点和扩展字段，支持深度优先谓词/Selector
//! 查找、子节点索引路径、类型路径、相对路径和 JSON 保存/加载。
//! 查询返回快照节点的借用；重新采集当前页面使用 [`crate::HmDriver::ui_tree`]。

use crate::{Bounds, DriverError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use tracing::trace;

/// `uitest dumpLayout` 返回的一个 UI 节点。
///
/// 节点及子树是主机快照；本地查询和路径遍历返回对该快照的借用，不发送设备请求。
/// 遍历先检查当前节点，再按 `children` 顺序深度优先访问子节点。
///
/// ```
/// use hm_driver_rs::{Selector, UiNode};
/// use serde_json::json;
/// # fn main() -> hm_driver_rs::Result<()> {
/// let tree = UiNode::from_layout_json(json!({"root": {
///     "attributes": {"type": "Column"},
///     "children": [{"attributes": {"type": "Text", "text": "确定"}}]
/// }}))?;
/// let node = tree.find_by_selector(&Selector::new().text("确定"))?.unwrap();
/// assert_eq!(node.attribute_str("text"), Some("确定"));
/// assert_eq!(tree.at_hierarchy(&[0]).unwrap().node_type().as_deref(), Some("Text"));
/// # Ok(())
/// # }
/// ```
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
    /// 以借用形式读取字符串属性，避免遍历 UI 树时重复分配字符串。
    ///
    /// 属性值不是 JSON 字符串时返回 `None`；需要兼容布尔、数值等值时请使用
    /// [`attribute`](Self::attribute)。
    pub fn attribute_str(&self, name: &str) -> Option<&str> {
        self.attributes
            .get(name)
            .or_else(|| self.extra.get(name))
            .and_then(Value::as_str)
    }

    /// 读取布尔属性，兼容 dumpLayout 使用的 JSON 布尔值和 `"true"`/`"false"` 字符串。
    pub fn attribute_bool(&self, name: &str) -> Option<bool> {
        match self.attributes.get(name).or_else(|| self.extra.get(name)) {
            Some(Value::Bool(value)) => Some(*value),
            Some(Value::String(value)) => value.parse().ok(),
            _ => None,
        }
    }

    /// 读取节点属性，优先使用 `attributes` 对象。
    ///
    /// 缺少标准属性时读取 `extra`；字符串、布尔和数值转换为字符串，其他 JSON 类型返回 `None`。
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
    ///
    /// 缺失、无效或无法解析时返回 `None`，支持格式见 [`Bounds::parse_value`]。
    pub fn bounds(&self) -> Option<Bounds> {
        self.attributes
            .get("bounds")
            .or_else(|| self.extra.get("bounds"))
            .and_then(Bounds::parse_value)
    }

    /// 深度优先查找第一个满足 `predicate` 的节点，包含当前根节点。
    ///
    /// 返回快照节点的借用；未匹配返回 `None`。
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
    ///
    /// 包含当前根节点，保持遍历顺序；未匹配返回空列表。
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
    ///
    /// 遵循 [`crate::Selector::index`]，未匹配返回 `None`。本地关系条件返回
    /// [`DriverError::Unsupported`]，非法正则返回 [`DriverError::InvalidArgument`]。
    pub fn find_by_selector(&self, selector: &crate::Selector) -> Result<Option<&UiNode>> {
        let matches = self.find_all_by_selector(selector)?;
        Ok(matches.into_iter().nth(selector.selected_index()))
    }

    /// 使用 Selector 查找当前 UI 树快照中的全部匹配节点。
    ///
    /// 忽略 [`crate::Selector::index`]，无匹配返回空列表；条件支持和错误见
    /// [`find_by_selector`](Self::find_by_selector)。
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
    ///
    /// 索引从 `0` 开始，空路径返回当前根节点；任何一段越界返回 `None`。
    pub fn at_hierarchy(&self, hierarchy: &[usize]) -> Option<&UiNode> {
        let mut current = self;
        for &index in hierarchy {
            current = current.children.get(index)?;
        }
        Some(current)
    }

    /// 按 `/0/1/2` 形式的子节点索引路径读取节点。
    ///
    /// 空路径或 `/` 返回当前节点，索引越界返回 `None`，格式错误返回 [`DriverError::InvalidArgument`]。
    pub fn at_hierarchy_path(&self, path: &str) -> Result<Option<&UiNode>> {
        let hierarchy = parse_hierarchy_path(path)?;
        Ok(self.at_hierarchy(&hierarchy))
    }

    /// 按子节点类型路径读取节点，例如 `/Column/Flex/Text[2]`。
    ///
    /// 各段从当前节点的子节点开始匹配；`[n]` 是同类型子节点中从 `0` 开始的索引，
    /// 省略等价于 `[0]`，空路径返回当前节点。类型或索引未匹配返回 `None`，
    /// 段格式错误返回 [`DriverError::InvalidArgument`]。
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
    ///
    /// 搜索包含根节点，根节点对应空路径；未匹配返回 `None`。
    pub fn find_hierarchy(
        &self,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Option<(&UiNode, Vec<usize>)> {
        let mut path = Vec::new();
        self.find_hierarchy_ref(&predicate, &mut path)
    }

    /// 查找首个匹配节点对应的可点击目标。
    ///
    /// 优先返回匹配节点自身或其最近的、带 bounds 的可点击祖先。若父级没有声明
    /// `clickable=true`，则回退到匹配节点本身（只要它具有 bounds）。这适用于
    /// ArkUI 将可点击性挂在容器、文字位于子节点的常见布局。
    pub fn find_click_target(&self, predicate: impl Fn(&UiNode) -> bool) -> Option<&UiNode> {
        let (matched, mut hierarchy) = self.find_hierarchy(predicate)?;
        loop {
            let candidate = self.at_hierarchy(&hierarchy)?;
            if candidate.attribute_bool("clickable") == Some(true) && candidate.bounds().is_some() {
                return Some(candidate);
            }
            if hierarchy.is_empty() {
                break;
            }
            hierarchy.pop();
        }
        matched.bounds().map(|_| matched)
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
    ///
    /// 路径越过根节点或目标索引越界返回 `None`，段格式错误返回 [`DriverError::InvalidArgument`]。
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
    ///
    /// 以首个匹配节点为锚点；锚点或相对目标不存在返回 `None`。路径规则见
    /// [`relative_from`](Self::relative_from)。
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
    ///
    /// 同步写入主机文件，覆盖已有内容；返回 I/O 或 JSON 序列化错误。
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self).map_err(DriverError::Json)
    }

    /// 从 JSON 文件加载 UI 树快照。
    ///
    /// 同时接受直接根节点和 `{ "root": ... }` 包装格式。
    ///
    /// 同步读取主机文件，返回 I/O 或 JSON 解析错误。
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
    ///
    /// 解析失败返回 [`DriverError::Json`]；缺省的属性和子节点为空集合，额外字段保留在 `extra`。
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
    fn 借用字符串属性并找到可点击祖先() {
        let root: UiNode = serde_json::from_value(json!({
            "attributes": {"type": "Root", "visible": true},
            "children": [{
                "attributes": {
                    "type": "Column", "clickable": "true",
                    "bounds": "[0,0][100,50]"
                },
                "children": [{
                    "attributes": {
                        "type": "Text", "text": "新鲜应用",
                        "bounds": "[10,10][50,30]"
                    },
                    "children": []
                }]
            }]
        }))
        .unwrap();

        assert_eq!(
            root.find(|node| node.attribute_str("text") == Some("新鲜应用"))
                .unwrap()
                .attribute_str("text"),
            Some("新鲜应用")
        );
        assert_eq!(
            root.at_hierarchy(&[0]).unwrap().attribute_bool("clickable"),
            Some(true)
        );
        assert_eq!(root.attribute_bool("visible"), Some(true));
        let target = root
            .find_click_target(|node| node.attribute_str("text") == Some("新鲜应用"))
            .unwrap();
        assert_eq!(target.node_type().as_deref(), Some("Column"));
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
