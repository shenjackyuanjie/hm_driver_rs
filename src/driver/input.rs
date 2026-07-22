//! 点击、滑动、按键与手势注入。

use super::HmDriver;
use crate::gesture::Gesture;
use crate::keycode::KeyCode;
use crate::types::{Point, Position, SwipeArea, SwipeDirection};
use crate::{DriverError, Result};
use serde_json::json;
use std::time::Duration;

impl HmDriver {
    /// 通过按键码（原始值）发送按键事件。
    ///
    /// 按键码范围 0–3200；需要类型安全的按键码请使用 [`press_key_code`] 或 [`KeyCode`]。
    pub async fn press_key(&self, key_code: u32) -> Result<()> {
        if key_code > 3200 {
            return Err(DriverError::InvalidCoordinate("按键码超过 3200".into()));
        }
        self.send_key_code(i32::try_from(key_code).expect("按键码已经限制为 3200 以下"))
            .await
    }

    async fn send_key_code(&self, key_code: i32) -> Result<()> {
        if !(-1..=3200).contains(&key_code) {
            return Err(DriverError::InvalidCoordinate(
                "按键码必须位于 -1 到 3200".into(),
            ));
        }
        self.inner
            .hdc
            .shell(format!("uitest uiInput keyEvent {key_code}"))
            .await
            .map(|_| ())
    }

    /// 通过 [`KeyCode`] 枚举发送按键事件。
    pub async fn press_key_code(&self, key_code: KeyCode) -> Result<()> {
        self.send_key_code(key_code.value()).await
    }

    /// 同时触发两个或三个组合键。
    pub async fn press_key_combination(&self, key_codes: &[KeyCode]) -> Result<()> {
        if !(2..=3).contains(&key_codes.len()) {
            return Err(DriverError::InvalidArgument(
                "组合键必须包含两个或三个按键".into(),
            ));
        }
        let values: Vec<_> = key_codes.iter().map(|key| key.value()).collect();
        self.driver_call("triggerCombineKeys", json!(values))
            .await
            .map(|_| ())
    }

    /// 发送返回键。
    pub async fn go_back(&self) -> Result<()> {
        self.press_key_code(KeyCode::Back).await
    }

    /// 发送主页键。
    pub async fn go_home(&self) -> Result<()> {
        self.press_key_code(KeyCode::Home).await
    }

    /// 在指定绝对坐标处点击。
    pub async fn click(&self, point: Point) -> Result<()> {
        self.coordinate_call("click", json!([point.x, point.y]))
            .await
    }

    /// 在指定绝对或归一化坐标处点击。
    pub async fn click_position(&self, position: Position) -> Result<()> {
        self.click(self.absolute_position(position).await?).await
    }

    /// 在指定绝对坐标处双击。
    pub async fn double_click(&self, point: Point) -> Result<()> {
        self.coordinate_call("doubleClick", json!([point.x, point.y]))
            .await
    }

    /// 在指定绝对或归一化坐标处双击。
    pub async fn double_click_position(&self, position: Position) -> Result<()> {
        self.double_click(self.absolute_position(position).await?)
            .await
    }

    /// 在指定绝对坐标处长按。
    pub async fn long_click(&self, point: Point) -> Result<()> {
        self.coordinate_call("longClick", json!([point.x, point.y]))
            .await
    }

    /// 在指定绝对或归一化坐标处长按。
    pub async fn long_click_position(&self, position: Position) -> Result<()> {
        self.long_click(self.absolute_position(position).await?)
            .await
    }

    /// 从起点滑动到终点，`speed` 为滑动速度（200–40000 像素/秒）。
    pub async fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        if !(200..=40_000).contains(&speed) {
            return Err(DriverError::InvalidCoordinate(
                "滑动速度必须位于 200 到 40000".into(),
            ));
        }
        self.coordinate_call("swipe", json!([from.x, from.y, to.x, to.y, speed]))
            .await
    }

    /// 从归一化或绝对坐标位置滑动到目标位置。
    pub async fn swipe_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        let size = self.display_size().await?;
        self.swipe(from.resolve(size)?, to.resolve(size)?, speed)
            .await
    }

    /// 从一个绝对坐标拖拽到另一个绝对坐标。
    pub async fn drag(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        validate_motion_speed(speed)?;
        self.driver_call("drag", json!([from.x, from.y, to.x, to.y, speed]))
            .await
            .map(|_| ())
    }

    /// 接受绝对或归一化坐标的拖拽操作。
    pub async fn drag_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        let size = self.display_size().await?;
        self.drag(from.resolve(size)?, to.resolve(size)?, speed)
            .await
    }

    /// 执行带固定步长的抛滑操作。
    pub async fn fling(&self, from: Point, to: Point, step_length: u32, speed: u32) -> Result<()> {
        validate_motion_speed(speed)?;
        if step_length == 0 {
            return Err(DriverError::InvalidArgument("抛滑步长必须大于 0".into()));
        }
        self.driver_call("fling", json!([from, to, step_length, speed]))
            .await
            .map(|_| ())
    }

    /// 接受绝对或归一化坐标的抛滑操作。
    pub async fn fling_positions(
        &self,
        from: Position,
        to: Position,
        step_length: u32,
        speed: u32,
    ) -> Result<()> {
        let size = self.display_size().await?;
        self.fling(from.resolve(size)?, to.resolve(size)?, step_length, speed)
            .await
    }

    /// 在指定区域内按方向滑动一定比例。
    ///
    /// `area` 指定滑动区域，`scale` 为滑动距离与区域尺寸的比例（0–1）。
    pub async fn swipe_direction(
        &self,
        direction: SwipeDirection,
        area: SwipeArea,
        scale: f64,
        speed: u32,
    ) -> Result<()> {
        if !scale.is_finite() || !(0.0..=1.0).contains(&scale) || scale == 0.0 {
            return Err(DriverError::InvalidCoordinate(
                "方向滑动比例必须位于 0 到 1 之间".into(),
            ));
        }
        let bounds = area.resolve(self.display_size().await?)?;
        let center = bounds.center();
        let horizontal = (f64::from(bounds.width()) * scale / 2.0).round() as i32;
        let vertical = (f64::from(bounds.height()) * scale / 2.0).round() as i32;
        let (from, to) = match direction {
            SwipeDirection::Up => (
                Point::new(center.x, center.y + vertical),
                Point::new(center.x, center.y - vertical),
            ),
            SwipeDirection::Down => (
                Point::new(center.x, center.y - vertical),
                Point::new(center.x, center.y + vertical),
            ),
            SwipeDirection::Left => (
                Point::new(center.x + horizontal, center.y),
                Point::new(center.x - horizontal, center.y),
            ),
            SwipeDirection::Right => (
                Point::new(center.x - horizontal, center.y),
                Point::new(center.x + horizontal, center.y),
            ),
        };
        self.swipe(from, to, speed).await
    }

    /// 执行一个多指手势轨迹。
    pub async fn perform_gesture(&self, gesture: &Gesture) -> Result<()> {
        let matrix = gesture.compile(self.display_size().await?)?;
        let total_points = matrix.first().map(Vec::len).unwrap_or_default();
        let reference = self
            .call_api_raw(
                "PointerMatrix.create",
                None,
                json!([matrix.len(), total_points]),
            )
            .await?
            .as_str()
            .ok_or_else(|| DriverError::Protocol("PointerMatrix.create 未返回远端引用".into()))?
            .to_owned();
        let result = async {
            for (finger_index, points) in matrix.iter().enumerate() {
                for (point_index, point) in points.iter().enumerate() {
                    self.call_api_raw(
                        "PointerMatrix.setPoint",
                        Some(&reference),
                        json!([
                            finger_index,
                            point_index,
                            {"x": point.encoded_x()?, "y": point.point.y}
                        ]),
                    )
                    .await?;
                }
            }
            self.driver_call(
                "injectMultiPointerAction",
                json!([reference, gesture.injection_speed_value()]),
            )
            .await
            .map(|_| ())
        }
        .await;
        self.queue_remote_reference(reference, self.generation());
        result
    }

    /// 输入文本到当前焦点控件。
    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.coordinate_call("inputText", json!([{"x": 1, "y": 1}, text]))
            .await
    }

    /// 等待 UI 连续空闲指定时长，最长等待 `timeout`。
    pub async fn wait_for_idle(&self, idle_time: Duration, timeout: Duration) -> Result<()> {
        let idle_millis = duration_millis(idle_time, "UI 空闲时长")?;
        let timeout_millis = duration_millis(timeout, "UI 空闲等待超时")?;
        let rpc_timeout = timeout
            .checked_add(Duration::from_secs(1))
            .ok_or_else(|| DriverError::InvalidArgument("UI 空闲等待超时过大".into()))?;
        self.driver_call_with_timeout(
            "waitForIdle",
            json!([idle_millis, timeout_millis]),
            rpc_timeout,
        )
        .await
        .map(|_| ())
    }
}

/// 校验拖拽/抛滑的速度参数范围。
fn validate_motion_speed(speed: u32) -> Result<()> {
    if (200..=40_000).contains(&speed) {
        Ok(())
    } else {
        Err(DriverError::InvalidArgument(
            "操作速度必须位于 200 到 40000".into(),
        ))
    }
}

/// 将 Duration 转换为 u32 毫秒，校验非零且不溢出。
fn duration_millis(duration: Duration, name: &str) -> Result<u32> {
    if duration.is_zero() {
        return Err(DriverError::InvalidArgument(format!("{name}必须大于 0")));
    }
    u32::try_from(duration.as_millis())
        .map_err(|_| DriverError::InvalidArgument(format!("{name}超出 u32 毫秒范围")))
}
