use crate::driver::HmDriver;
use crate::ui::{UiNode, sanitize_xml_text};
use crate::{Bounds, DriverError, Result};
use regex::Regex;
use std::collections::BTreeMap;
use sxd_document::Package;
use sxd_document::dom::{Document, Element as DomElement};
use sxd_xpath::{Context, Factory, Value};

/// XPath 查询结果的属性和 bounds 快照。
#[derive(Clone)]
pub struct XPathElement {
    pub(crate) driver: HmDriver,
    pub(crate) attributes: BTreeMap<String, String>,
    pub(crate) bounds: Option<Bounds>,
}

impl std::fmt::Debug for XPathElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XPathElement")
            .field("attributes", &self.attributes)
            .field("bounds", &self.bounds)
            .finish()
    }
}

impl XPathElement {
    pub(crate) fn query(driver: HmDriver, root: &UiNode, expression: &str) -> Result<Vec<Self>> {
        let package = Package::new();
        let document = package.as_document();
        let name_pattern = Regex::new(r"^[A-Za-z_][A-Za-z0-9_.-]*$")
            .map_err(|error| DriverError::InvalidXPath(error.to_string()))?;
        let mut snapshots = Vec::new();
        let root_element = create_node(document, root, &name_pattern, &mut snapshots);
        document.root().append_child(root_element);
        let xpath = Factory::new()
            .build(expression)
            .map_err(|error| DriverError::InvalidXPath(error.to_string()))?
            .ok_or_else(|| DriverError::InvalidXPath("空表达式".into()))?;
        let value = xpath
            .evaluate(&Context::new(), document.root())
            .map_err(|error| DriverError::InvalidXPath(error.to_string()))?;
        let Value::Nodeset(nodes) = value else {
            return Err(DriverError::InvalidXPath("表达式结果不是节点集合".into()));
        };
        let mut result = Vec::new();
        for node in nodes.document_order() {
            let Some(element) = node.element() else {
                continue;
            };
            let Some(index) = element
                .attribute_value("data-hm-driver-index")
                .and_then(|value| value.parse::<usize>().ok())
            else {
                continue;
            };
            let (attributes, bounds) = snapshots
                .get(index)
                .cloned()
                .ok_or_else(|| DriverError::Protocol("XPath 节点索引无效".into()))?;
            result.push(Self {
                driver: driver.clone(),
                attributes,
                bounds,
            });
        }
        Ok(result)
    }

    /// 节点是否有可交互的 bounds。
    pub fn exists(&self) -> bool {
        self.bounds.is_some()
    }

    /// 读取查询时保存的属性。
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    /// 返回查询时保存的全部属性。
    pub fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    /// 返回查询时保存的控件边界，若节点不可交互则返回 `None`。
    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// 返回控件边界中心点的绝对坐标，若节点不可交互则返回 `None`。
    pub fn center(&self) -> Option<crate::Point> {
        self.bounds.map(Bounds::center)
    }

    /// 返回控件的 `text` 属性值。
    pub fn text(&self) -> Option<&str> {
        self.attribute("text")
    }

    /// 点击该控件的中心位置。
    pub async fn click(&self) -> Result<()> {
        self.driver.click(self.required_center()?).await
    }

    /// 双击该控件的中心位置。
    pub async fn double_click(&self) -> Result<()> {
        self.driver.double_click(self.required_center()?).await
    }

    /// 长按该控件的中心位置。
    pub async fn long_click(&self) -> Result<()> {
        self.driver.long_click(self.required_center()?).await
    }

    /// 点击该控件并输入指定文本。
    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.click().await?;
        self.driver.input_text(text).await
    }

    fn required_center(&self) -> Result<crate::Point> {
        self.bounds
            .map(Bounds::center)
            .ok_or(DriverError::XPathNotFound)
    }
}

fn create_node<'d>(
    document: Document<'d>,
    node: &UiNode,
    name_pattern: &Regex,
    snapshots: &mut Vec<(BTreeMap<String, String>, Option<Bounds>)>,
) -> DomElement<'d> {
    let attributes = node.attribute_snapshot();
    let original_type = node.node_type().unwrap_or_else(|| "node".into());
    let tag = if name_pattern.is_match(&original_type) {
        original_type.as_str()
    } else {
        "node"
    };
    let element: DomElement<'d> = document.create_element(tag);
    let index = snapshots.len();
    snapshots.push((attributes.clone(), node.bounds()));
    let index_value = index.to_string();
    element.set_attribute_value("data-hm-driver-index", &index_value);
    for (key, value) in attributes {
        if key != "data-hm-driver-index" && name_pattern.is_match(&key) {
            let value = sanitize_xml_text(&value);
            element.set_attribute_value(key.as_str(), &value);
        }
    }
    if tag == "node"
        && !element
            .attributes()
            .iter()
            .any(|item| item.name().local_part() == "type")
    {
        let original_type = sanitize_xml_text(&original_type);
        element.set_attribute_value("type", &original_type);
    }
    for child in &node.children {
        let child = create_node(document, child, name_pattern, snapshots);
        element.append_child(child);
    }
    element
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn xpath_supports_unicode_attributes_relative_paths_and_invalid_tags() {
        let root: UiNode = serde_json::from_value(json!({
            "attributes": {"type": "root invalid", "bounds": "[0,0][100,100]"},
            "children": [{
                "attributes": {"type": "Text", "text": "中文\u{0000}标题", "bounds": "[10,20][30,40]"},
                "children": []
            }]
        })).unwrap();
        // XPath 构造本身不需要连接，测试使用内部占位 Driver 的工作放在 driver 模块。
        let package = Package::new();
        let document = package.as_document();
        let pattern = Regex::new(r"^[A-Za-z_][A-Za-z0-9_.-]*$").unwrap();
        let mut snapshots = Vec::new();
        let root_element = create_node(document, &root, &pattern, &mut snapshots);
        document.root().append_child(root_element);
        let xpath = Factory::new()
            .build("//Text[@text='中文标题']")
            .unwrap()
            .unwrap();
        let value = xpath.evaluate(&Context::new(), document.root()).unwrap();
        let Value::Nodeset(nodes) = value else {
            panic!("XPath 应返回节点集合");
        };
        assert_eq!(nodes.size(), 1);
        assert_eq!(snapshots[1].1.unwrap().center(), crate::Point::new(20, 30));
    }
}
