use super::block_on;
use crate::{Bounds, ElementInfo, Point, Result};
use serde_json::Value;
use std::time::Duration;

/// 阻塞控件句柄。
#[derive(Debug)]
pub struct Element {
    pub(super) inner: crate::Element,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Result<Value> {
        block_on(self.inner.attribute(name))?
    }

    pub fn id(&self) -> Result<String> {
        block_on(self.inner.id())?
    }

    pub fn key(&self) -> Result<String> {
        block_on(self.inner.key())?
    }

    pub fn type_name(&self) -> Result<String> {
        block_on(self.inner.type_name())?
    }

    pub fn text(&self) -> Result<String> {
        block_on(self.inner.text())?
    }

    pub fn description(&self) -> Result<String> {
        block_on(self.inner.description())?
    }

    pub fn hint(&self) -> Result<String> {
        block_on(self.inner.hint())?
    }

    pub fn is_selected(&self) -> Result<bool> {
        block_on(self.inner.is_selected())?
    }

    pub fn is_checked(&self) -> Result<bool> {
        block_on(self.inner.is_checked())?
    }

    pub fn is_enabled(&self) -> Result<bool> {
        block_on(self.inner.is_enabled())?
    }

    pub fn is_focused(&self) -> Result<bool> {
        block_on(self.inner.is_focused())?
    }

    pub fn is_checkable(&self) -> Result<bool> {
        block_on(self.inner.is_checkable())?
    }

    pub fn is_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_clickable())?
    }

    pub fn is_long_clickable(&self) -> Result<bool> {
        block_on(self.inner.is_long_clickable())?
    }

    pub fn is_scrollable(&self) -> Result<bool> {
        block_on(self.inner.is_scrollable())?
    }

    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    pub fn bounds_center(&self) -> Result<Point> {
        block_on(self.inner.bounds_center())?
    }

    pub fn info(&self) -> Result<ElementInfo> {
        block_on(self.inner.info())?
    }

    pub fn click(&self) -> Result<()> {
        block_on(self.inner.click())?
    }

    pub fn double_click(&self) -> Result<()> {
        block_on(self.inner.double_click())?
    }

    pub fn long_click(&self) -> Result<()> {
        block_on(self.inner.long_click())?
    }

    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    pub fn clear_text(&self) -> Result<()> {
        block_on(self.inner.clear_text())?
    }

    pub fn scroll_to_top(&self) -> Result<()> {
        block_on(self.inner.scroll_to_top())?
    }

    pub fn scroll_to_top_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_top_with_speed(speed))?
    }

    pub fn scroll_to_bottom(&self) -> Result<()> {
        block_on(self.inner.scroll_to_bottom())?
    }

    pub fn scroll_to_bottom_with_speed(&self, speed: u32) -> Result<()> {
        block_on(self.inner.scroll_to_bottom_with_speed(speed))?
    }

    pub fn scroll_search(&self, selector: &crate::Selector) -> Result<Option<Element>> {
        Ok(block_on(self.inner.scroll_search(selector))??.map(|inner| Element { inner }))
    }

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

    pub fn drag_to(&self, target: &Element) -> Result<()> {
        block_on(self.inner.drag_to(&target.inner))?
    }

    pub fn pinch_in(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_in(scale))?
    }

    pub fn pinch_out(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_out(scale))?
    }

    pub fn wait_until_gone(&self, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_until_gone(timeout))?
    }

    pub fn wait_for_attribute(
        &self,
        name: &str,
        expected: &Value,
        timeout: Duration,
    ) -> Result<bool> {
        block_on(self.inner.wait_for_attribute(name, expected, timeout))?
    }
}
