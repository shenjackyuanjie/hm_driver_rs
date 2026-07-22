use super::block_on;
use crate::{Bounds, Point, Result};

/// 阻塞 XPath 查询结果。
#[derive(Clone, Debug)]
pub struct XPathElement {
    /// 底层异步 XPathElement 实例。
    pub(super) inner: crate::XPathElement,
}

impl XPathElement {
    /// 判断 XPath 表达式匹配的元素是否存在。
    ///
    /// 与 [`HmDriver::xpath_exists`] 不同，此方法基于已缓存的结果判断，
    /// 无需再次发起远程调用。
    pub fn exists(&self) -> bool {
        self.inner.exists()
    }

    /// 获取指定属性的值。
    ///
    /// # 参数
    ///
    /// * `name` - 属性名（如 `"class"`、`"content-desc"` 等）
    ///
    /// 返回 `None` 表示属性不存在。
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.inner.attribute(name)
    }

    /// 获取所有属性的键值对映射。
    pub fn attributes(&self) -> &std::collections::BTreeMap<String, String> {
        self.inner.attributes()
    }

    /// 获取匹配元素的边界矩形。
    ///
    /// 返回 `None` 表示 XPath 无匹配元素。
    pub fn bounds(&self) -> Option<Bounds> {
        self.inner.bounds()
    }

    /// 获取匹配元素的中心点坐标。
    ///
    /// 返回 `None` 表示 XPath 无匹配元素。
    pub fn center(&self) -> Option<Point> {
        self.inner.center()
    }

    /// 获取匹配元素的文本内容。
    ///
    /// 返回 `None` 表示 XPath 无匹配元素或无文本内容。
    pub fn text(&self) -> Option<&str> {
        self.inner.text()
    }

    /// 点击匹配的元素（点击中心点）。
    ///
    /// 若 XPath 无匹配元素，则返回包含错误信息的 `Err`。
    pub fn click(&self) -> Result<()> {
        block_on(self.inner.click())?
    }

    /// 双击匹配的元素。
    pub fn double_click(&self) -> Result<()> {
        block_on(self.inner.double_click())?
    }

    /// 长按匹配的元素。
    pub fn long_click(&self) -> Result<()> {
        block_on(self.inner.long_click())?
    }

    /// 在匹配的元素中输入文本。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }
}
