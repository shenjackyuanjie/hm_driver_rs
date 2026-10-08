//! 远端控件句柄的同步门面。
//!
//! [`Element`] 封装 [`crate::Element`]，通过共享 runtime 同步等待属性读取与控件动作。
//! 恢复后的重新定位和远端引用释放由底层句柄管理，各方法链接对应异步 API。

use super::block_on;
use crate::{Bounds, ElementInfo, Point, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::trace;

/// 阻塞控件句柄。
///
/// 属性和动作在同步线程等待 RPC 完成，定位、恢复及引用清理语义见 [`crate::Element`]。
#[derive(Debug)]
pub struct Element {
    /// 底层异步 Element 实例。
    pub(super) inner: crate::Element,
}

impl Element {
    /// 读取控件的指定属性原始值。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::attribute`]。
    pub fn attribute(&self, name: &str) -> Result<Value> {
        block_on(self.inner.attribute(name))?
    }

    /// 一次 RPC 读取控件公开的全部属性。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::all_properties`]。
    pub fn all_properties(&self) -> Result<BTreeMap<String, Value>> {
        block_on(self.inner.all_properties())?
    }

    /// 读取控件未经展示层转换的原始文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::original_text`]。
    pub fn original_text(&self) -> Result<String> {
        block_on(self.inner.original_text())?
    }

    /// 读取控件的资源 ID。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::id`]。
    pub fn id(&self) -> Result<String> {
        block_on(self.inner.id())?
    }

    /// 读取控件的键值。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::key`]。
    pub fn key(&self) -> Result<String> {
        block_on(self.inner.key())?
    }

    /// 读取控件的类型名称。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::type_name`]。
    pub fn type_name(&self) -> Result<String> {
        block_on(self.inner.type_name())?
    }

    /// 读取控件的文本内容。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::text`]。
    pub fn text(&self) -> Result<String> {
        block_on(self.inner.text())?
    }

    /// 读取控件的描述内容。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::description`]。
    pub fn description(&self) -> Result<String> {
        block_on(self.inner.description())?
    }

    /// 读取控件的提示文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::hint`]。
    pub fn hint(&self) -> Result<String> {
        block_on(self.inner.hint())?
    }

    /// 判断控件是否已被选中。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_selected`]。
    pub fn is_selected(&self) -> Result<bool> {
        block_on(self.inner.is_selected())?
    }

    /// 判断控件是否已被勾选。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_checked`]。
    pub fn is_checked(&self) -> Result<bool> {
        block_on(self.inner.is_checked())?
    }

    /// 判断控件是否已启用。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_enabled`]。
    pub fn is_enabled(&self) -> Result<bool> {
        block_on(self.inner.is_enabled())?
    }

    /// 判断控件是否已获取焦点。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_focused`]。
    pub fn is_focused(&self) -> Result<bool> {
        block_on(self.inner.is_focused())?
    }

    /// 判断控件是否可被勾选。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_checkable`]。
    pub fn is_checkable(&self) -> Result<bool> {
        block_on(self.inner.is_checkable())?
    }

    /// 判断控件是否可被点击。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_clickable`]。
    pub fn is_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_clickable())?
    }

    /// 判断控件是否可被长按。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_long_clickable`]。
    pub fn is_long_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_long_clickable())?
    }

    /// 判断控件是否可滚动。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::is_scrollable`]。
    pub fn is_scrollable(&self) -> Result<bool> {
        block_on(self.inner.is_scrollable())?
    }

    /// 读取控件的边界矩形。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::bounds`]。
    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    /// 读取控件边界矩形的中心点坐标。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::bounds_center`]。
    pub fn bounds_center(&self) -> Result<Point> {
        block_on(self.inner.bounds_center())?
    }

    /// 按控件边界中的相对偏移计算坐标。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::point_at`]。
    pub fn point_at(&self, offset_x: f64, offset_y: f64) -> Result<Point> {
        block_on(self.inner.point_at(offset_x, offset_y))?
    }

    /// 依次读取控件属性并汇总为 [`ElementInfo`]。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::info`]。
    pub fn info(&self) -> Result<ElementInfo> {
        block_on(self.inner.info())?
    }

    /// 点击控件。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::click`]。
    pub fn click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::click");
        block_on(self.inner.click())?
    }

    /// 双击控件。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::double_click`]。
    pub fn double_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::double_click");
        block_on(self.inner.double_click())?
    }

    /// 长按控件。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::long_click`]。
    pub fn long_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 Element::long_click");
        block_on(self.inner.long_click())?
    }

    /// 点击控件内（或控件外）的相对偏移位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::click_at`]。
    pub fn click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        block_on(self.inner.click_at(offset_x, offset_y))?
    }

    /// 双击控件内（或控件外）的相对偏移位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::double_click_at`]。
    pub fn double_click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        block_on(self.inner.double_click_at(offset_x, offset_y))?
    }

    /// 长按控件内（或控件外）的相对偏移位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::long_click_at`]。
    pub fn long_click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        block_on(self.inner.long_click_at(offset_x, offset_y))?
    }

    /// 在控件中心长按给定时长。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::long_click_for`]。
    pub fn long_click_for(&self, duration: std::time::Duration) -> Result<()> {
        block_on(self.inner.long_click_for(duration))?
    }

    /// 在控件内（或控件外）的相对偏移位置长按给定时长。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::long_click_at_for`]。
    pub fn long_click_at_for(
        &self,
        offset_x: f64,
        offset_y: f64,
        duration: std::time::Duration,
    ) -> Result<()> {
        block_on(self.inner.long_click_at_for(offset_x, offset_y, duration))?
    }

    /// 在控件中心执行单次或双次指关节敲击。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::knuckle_knock`]。
    pub fn knuckle_knock(&self, times: u8) -> Result<()> {
        block_on(self.inner.knuckle_knock(times))?
    }

    /// 以控件中心执行指关节闭合圈选。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::knuckle_select`]。
    pub fn knuckle_select(&self, radius: u32, speed: u32) -> Result<()> {
        block_on(self.inner.knuckle_select(radius, speed))?
    }

    /// 向控件输入文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::input_text`]。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    /// 清除控件中的文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::clear_text`]。
    pub fn clear_text(&self) -> Result<()> {
        block_on(self.inner.clear_text())?
    }

    /// 以速度 `600` 滚动到控件顶部。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_to_top`]。
    pub fn scroll_to_top(&self) -> Result<()> {
        block_on(self.inner.scroll_to_top())?
    }

    /// 以指定速度滚动到控件顶部。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_to_top_with_speed`]。
    pub fn scroll_to_top_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_top_with_speed(speed))?
    }

    /// 以速度 `600` 滚动到控件底部。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_to_bottom`]。
    pub fn scroll_to_bottom(&self) -> Result<()> {
        block_on(self.inner.scroll_to_bottom())?
    }

    /// 以指定速度滚动到控件底部。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_to_bottom_with_speed`]。
    pub fn scroll_to_bottom_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_bottom_with_speed(speed))?
    }

    /// 在当前可滚动控件中查找目标控件。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_search`]。
    pub fn scroll_search(&self, selector: &crate::Selector) -> Result<Option<Element>> {
        Ok(block_on(self.inner.scroll_search(selector))??.map(|inner| Element { inner }))
    }

    /// 指定滚动方向及可选边缘偏移后查找目标控件。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::scroll_search_with_options`]。
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

    /// 将当前控件拖拽到目标控件位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::drag_to`]。
    pub fn drag_to(&self, target: &Element) -> Result<()> {
        block_on(self.inner.drag_to(&target.inner))?
    }

    /// 在控件上执行捏合缩小手势。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::pinch_in`]。
    pub fn pinch_in(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_in(scale))?
    }

    /// 在控件上执行捏合放大手势。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::pinch_out`]。
    pub fn pinch_out(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_out(scale))?
    }

    /// 等待控件从界面上消失。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::wait_until_gone`]。
    pub fn wait_until_gone(&self, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_until_gone(timeout))?
    }

    /// 等待控件属性变为指定值，超时返回 `false`。
    ///
    /// 参数、返回值和设备要求见 [`crate::Element::wait_for_attribute`]。
    pub fn wait_for_attribute(
        &self,
        name: &str,
        expected: &Value,
        timeout: Duration,
    ) -> Result<bool> {
        block_on(self.inner.wait_for_attribute(name, expected, timeout))?
    }
}
