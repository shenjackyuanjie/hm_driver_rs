//! XPath 快照的同步门面。
//!
//! [`XPathElement`] 的属性、bounds 和存在性检查直接读取主机快照；
//! 点击、长按及文本输入通过共享 runtime 等待底层坐标操作完成。
//! 快照与重新查询语义见 [`crate::XPathElement`]。

use super::block_on;
use crate::{Bounds, Point, Result};
use tracing::trace;

/// 阻塞 XPath 查询结果。
///
/// 属性读取直接访问主机快照，动作同步等待坐标注入完成；快照语义见 [`crate::XPathElement`]。
#[derive(Clone, Debug)]
pub struct XPathElement {
    /// 底层异步 XPathElement 实例。
    pub(super) inner: crate::XPathElement,
}

impl XPathElement {
    /// 判断快照是否含可解析的 bounds。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::exists`]。
    pub fn exists(&self) -> bool {
        self.inner.exists()
    }

    /// 读取查询时保存的属性。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::attribute`]。
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.inner.attribute(name)
    }

    /// 返回查询时保存的全部属性。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::attributes`]。
    pub fn attributes(&self) -> &std::collections::BTreeMap<String, String> {
        self.inner.attributes()
    }

    /// 返回查询时保存的控件边界；缺少有效 bounds 时返回 `None`。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::bounds`]。
    pub fn bounds(&self) -> Option<Bounds> {
        self.inner.bounds()
    }

    /// 返回快照边界中心的绝对像素坐标；缺少有效 bounds 时返回 `None`。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::center`]。
    pub fn center(&self) -> Option<Point> {
        self.inner.center()
    }

    /// 返回控件的 `text` 属性值。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::text`]。
    pub fn text(&self) -> Option<&str> {
        self.inner.text()
    }

    /// 点击该控件的中心位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::click`]。
    pub fn click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 XPathElement::click");
        block_on(self.inner.click())?
    }

    /// 双击该控件的中心位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::double_click`]。
    pub fn double_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 XPathElement::double_click");
        block_on(self.inner.double_click())?
    }

    /// 长按该控件的中心位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::long_click`]。
    pub fn long_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 XPathElement::long_click");
        block_on(self.inner.long_click())?
    }

    /// 先点击该控件，再调用 [`crate::HmDriver::input_text`] 输入指定文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::XPathElement::input_text`]。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }
}
