use super::block_on;
use crate::{Bounds, ElementInfo, Point, Result};
use serde_json::Value;
use std::time::Duration;
use tracing::trace;

/// 阻塞控件句柄。
#[derive(Debug)]
pub struct Element {
    /// 底层异步 Element 实例。
    pub(super) inner: crate::Element,
}

impl Element {
    /// 获取控件的指定属性值。
    ///
    /// # 参数
    ///
    /// * `name` - 属性名称（如 `"content-desc"`、`"text"` 等）
    pub fn attribute(&self, name: &str) -> Result<Value> {
        block_on(self.inner.attribute(name))?
    }

    /// 获取控件的唯一标识 ID。
    pub fn id(&self) -> Result<String> {
        block_on(self.inner.id())?
    }

    /// 获取控件的键值（用于区分同类型控件）。
    pub fn key(&self) -> Result<String> {
        block_on(self.inner.key())?
    }

    /// 获取控件的类型名称（如 `Button`、`Text`、`Image` 等）。
    pub fn type_name(&self) -> Result<String> {
        block_on(self.inner.type_name())?
    }

    /// 获取控件的文本内容。
    pub fn text(&self) -> Result<String> {
        block_on(self.inner.text())?
    }

    /// 获取控件的描述信息（对应 `content-desc` 属性）。
    pub fn description(&self) -> Result<String> {
        block_on(self.inner.description())?
    }

    /// 获取控件的提示文本（placeholder）。
    pub fn hint(&self) -> Result<String> {
        block_on(self.inner.hint())?
    }

    /// 判断控件是否处于选中状态。
    pub fn is_selected(&self) -> Result<bool> {
        block_on(self.inner.is_selected())?
    }

    /// 判断控件是否处于勾选状态。
    pub fn is_checked(&self) -> Result<bool> {
        block_on(self.inner.is_checked())?
    }

    /// 判断控件是否处于启用状态。
    pub fn is_enabled(&self) -> Result<bool> {
        block_on(self.inner.is_enabled())?
    }

    /// 判断控件是否处于焦点状态。
    pub fn is_focused(&self) -> Result<bool> {
        block_on(self.inner.is_focused())?
    }

    /// 判断控件是否可以被勾选。
    pub fn is_checkable(&self) -> Result<bool> {
        block_on(self.inner.is_checkable())?
    }

    /// 判断控件是否可以被点击。
    pub fn is_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_clickable())?
    }

    /// 判断控件是否可以被长按。
    pub fn is_long_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_long_clickable())?
    }

    /// 判断控件是否可滚动。
    pub fn is_scrollable(&self) -> Result<bool> {
        block_on(self.inner.is_scrollable())?
    }

    /// 获取控件的边界矩形（左上角坐标 + 宽高）。
    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    /// 获取控件边界矩形的中心点坐标。
    ///
    /// 常用于执行点击操作。
    pub fn bounds_center(&self) -> Result<Point> {
        block_on(self.inner.bounds_center())?
    }

    /// 获取控件的完整信息集合（包含各种属性和状态）。
    pub fn info(&self) -> Result<ElementInfo> {
        block_on(self.inner.info())?
    }

    /// 点击控件（点击中心点）。
    pub fn click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::click");
        block_on(self.inner.click())?
    }

    /// 双击控件。
    pub fn double_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::double_click");
        block_on(self.inner.double_click())?
    }

    /// 长按控件。
    pub fn long_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::long_click");
        block_on(self.inner.long_click())?
    }

    /// 在控件中输入文本。
    ///
    /// 文本将输入到当前控件中，等同于逐字符输入。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    /// 清除控件中的文本内容。
    pub fn clear_text(&self) -> Result<()> {
        block_on(self.inner.clear_text())?
    }

    /// 将控件滚动到顶部。
    ///
    /// 使用默认速度。
    pub fn scroll_to_top(&self) -> Result<()> {
        block_on(self.inner.scroll_to_top())?
    }

    /// 以指定速度将控件滚动到顶部。
    pub fn scroll_to_top_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_top_with_speed(speed))?
    }

    /// 将控件滚动到底部。
    ///
    /// 使用默认速度。
    pub fn scroll_to_bottom(&self) -> Result<()> {
        block_on(self.inner.scroll_to_bottom())?
    }

    /// 以指定速度将控件滚动到底部。
    pub fn scroll_to_bottom_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_bottom_with_speed(speed))?
    }

    /// 在当前控件（如列表）中滚动搜索目标元素。
    ///
    /// 通过逐页滚动查找匹配选择器的子元素。
    pub fn scroll_search(&self, selector: &crate::Selector) -> Result<Option<Element>> {
        Ok(block_on(self.inner.scroll_search(selector))??.map(|inner| Element { inner }))
    }

    /// 在控件中滚动搜索目标元素，支持自定义方向与偏移。
    ///
    /// # 参数
    ///
    /// * `selector` - 目标元素选择器
    /// * `vertical` - `true` 表示垂直滚动，`false` 表示水平滚动
    /// * `offset` - 可选的滚动偏移量（像素）
    pub fn scroll_search_with_options(
        &self,
        selector: &crate::Selector,
        vertical: bool,
        offset: Option<u32>,
    ) -> Result<Option<Element>> {
        Ok(block_on(
            self.inner
                .scroll_search_with_options(selector, vertical, offset),
        )??
        .map(|inner| Element { inner }))
    }

    /// 将当前控件拖拽到目标控件上。
    pub fn drag_to(&self, target: &Element) -> Result<()> {
        block_on(self.inner.drag_to(&target.inner))?
    }

    /// 在控件上执行捏合缩小操作。
    ///
    /// # 参数
    ///
    /// * `scale` - 缩放比例（0.0 ~ 1.0）
    pub fn pinch_in(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_in(scale))?
    }

    /// 在控件上执行捏合放大操作。
    ///
    /// # 参数
    ///
    /// * `scale` - 缩放比例（>= 1.0）
    pub fn pinch_out(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_out(scale))?
    }

    /// 等待直到控件从界面上消失。
    ///
    /// 返回 `true` 表示控件已在超时时间内消失，`false` 表示超时后控件仍在。
    pub fn wait_until_gone(&self, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_until_gone(timeout))?
    }

    /// 等待直到控件的指定属性达到预期值。
    ///
    /// # 参数
    ///
    /// * `name` - 属性名
    /// * `expected` - 期望的属性值
    /// * `timeout` - 等待超时时间
    pub fn wait_for_attribute(
        &self,
        name: &str,
        expected: &Value,
        timeout: Duration,
    ) -> Result<bool> {
        block_on(self.inner.wait_for_attribute(name, expected, timeout))?
    }
}
