//! 点击、滑动、按键与手势注入。

use super::HmDriver;
use crate::gesture::Gesture;
use crate::keycode::KeyCode;
use crate::types::{MouseButton, Point, Position, SwipeArea, SwipeDirection};
use crate::{DriverError, Result};
use serde_json::{Value, json};
use std::time::Duration;
use tracing::{debug, trace};

impl HmDriver {
    /// 通过按键码（原始值）发送按键事件。
    ///
    /// 按键码范围 0–3200；需要类型安全的按键码请使用 [`press_key_code`] 或 [`KeyCode`]。
    pub async fn press_key(&self, key_code: u32) -> Result<()> {
        trace!(target: "hm_driver_rs::input", key_code, "按键事件");
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
        trace!(target: "hm_driver_rs::input", key = %key_code.value(), "按键(KeyCode)");
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
        trace!(target: "hm_driver_rs::input", "返回键");
        self.press_key_code(KeyCode::Back).await
    }

    /// 发送主页键。
    pub async fn go_home(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::input", "主页键");
        self.press_key_code(KeyCode::Home).await
    }

    /// 在指定绝对坐标处点击。
    pub async fn click(&self, point: Point) -> Result<()> {
        trace!(target: "hm_driver_rs::input", x = point.x, y = point.y, "点击");
        self.coordinate_call("click", json!([point.x, point.y]))
            .await
    }

    /// 在指定绝对或归一化坐标处点击。
    pub async fn click_position(&self, position: Position) -> Result<()> {
        self.click(self.absolute_position(position).await?).await
    }

    /// 在指定绝对坐标处双击。
    pub async fn double_click(&self, point: Point) -> Result<()> {
        trace!(target: "hm_driver_rs::input", x = point.x, y = point.y, "双击");
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
        trace!(target: "hm_driver_rs::input", x = point.x, y = point.y, "长按");
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
        trace!(target: "hm_driver_rs::input", from = %format!("({},{})", from.x, from.y), to = %format!("({},{})", to.x, to.y), speed, "滑动");
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
        trace!(target: "hm_driver_rs::input", from = %format!("({},{})", from.x, from.y), to = %format!("({},{})", to.x, to.y), speed, "拖拽");
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
        debug!(target: "hm_driver_rs::input", "执行手势");
        let reference = self.create_pointer_matrix(gesture).await?;
        let result = async {
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

    /// 鼠标单击，支持同时按住最多两个键盘按键。
    pub async fn mouse_click(
        &self,
        point: Point,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        let args = mouse_button_args(point, button, keys)?;
        self.driver_call("mouseClick", Value::Array(args))
            .await
            .map(|_| ())
    }

    /// 在绝对或归一化位置执行鼠标单击。
    pub async fn mouse_click_position(
        &self,
        position: Position,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        self.mouse_click(self.absolute_position(position).await?, button, keys)
            .await
    }

    /// 鼠标双击，支持同时按住最多两个键盘按键。
    pub async fn mouse_double_click(
        &self,
        point: Point,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        let args = mouse_button_args(point, button, keys)?;
        self.driver_call("mouseDoubleClick", Value::Array(args))
            .await
            .map(|_| ())
    }

    /// 鼠标长按，支持同时按住最多两个键盘按键。
    pub async fn mouse_long_click(
        &self,
        point: Point,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        let args = mouse_button_args(point, button, keys)?;
        self.driver_call("mouseLongClick", Value::Array(args))
            .await
            .map(|_| ())
    }

    /// 通过系统输入命令执行指定时长的鼠标长按。
    pub async fn mouse_long_click_for(
        &self,
        point: Point,
        button: MouseButton,
        duration: Duration,
    ) -> Result<()> {
        let millis = duration_millis(duration, "鼠标长按时长")?;
        self.inner
            .hdc
            .shell(format!(
                "uinput -M -m {} {} -d {} -i {} -u {}",
                point.x,
                point.y,
                button.value(),
                millis,
                button.value()
            ))
            .await
            .map(|_| ())
    }

    /// 滚动鼠标滚轮。正数向前/向上，负数向后/向下。
    pub async fn mouse_scroll(&self, point: Point, distance: i32, keys: &[KeyCode]) -> Result<()> {
        if distance == 0 {
            return Err(DriverError::InvalidArgument("鼠标滚轮距离不能为 0".into()));
        }
        validate_mouse_keys(keys)?;
        let mut args = vec![
            json!(point),
            json!(distance < 0),
            json!(distance.unsigned_abs()),
        ];
        args.extend(keys.iter().map(|key| json!(key.value())));
        self.driver_call("mouseScroll", Value::Array(args))
            .await
            .map(|_| ())
    }

    /// 将鼠标指针直接移动到指定坐标。
    pub async fn mouse_move_to(&self, point: Point) -> Result<()> {
        self.driver_call("mouseMoveTo", json!([point]))
            .await
            .map(|_| ())
    }

    /// 按给定速度沿轨迹移动鼠标。
    pub async fn mouse_move(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        validate_motion_speed(speed)?;
        self.driver_call("mouseMoveWithTrack", json!([from, to, speed]))
            .await
            .map(|_| ())
    }

    /// 按住鼠标左键拖拽。
    pub async fn mouse_drag(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        validate_motion_speed(speed)?;
        self.driver_call("mouseDrag", json!([from, to, speed]))
            .await
            .map(|_| ())
    }

    /// 触控笔点击。
    pub async fn pen_click(&self, point: Point) -> Result<()> {
        self.driver_call("penClick", json!([point]))
            .await
            .map(|_| ())
    }

    /// 触控笔双击。
    pub async fn pen_double_click(&self, point: Point) -> Result<()> {
        self.driver_call("penDoubleClick", json!([point]))
            .await
            .map(|_| ())
    }

    /// 触控笔长按，可指定 0 到 1 的压力。
    pub async fn pen_long_click(&self, point: Point, pressure: Option<f64>) -> Result<()> {
        validate_pressure(pressure)?;
        self.driver_call("penLongClick", json!([point, pressure]))
            .await
            .map(|_| ())
    }

    /// 触控笔滑动，可指定速度和压力。
    pub async fn pen_swipe(
        &self,
        from: Point,
        to: Point,
        speed: u32,
        pressure: Option<f64>,
    ) -> Result<()> {
        validate_motion_speed(speed)?;
        validate_pressure(pressure)?;
        self.driver_call("penSwipe", json!([from, to, speed, pressure]))
            .await
            .map(|_| ())
    }

    /// 使用触控笔注入自定义轨迹。
    pub async fn perform_pen_gesture(
        &self,
        gesture: &Gesture,
        pressure: Option<f64>,
    ) -> Result<()> {
        validate_pressure(pressure)?;
        let reference = self.create_pointer_matrix(gesture).await?;
        let result = self
            .driver_call(
                "injectPenPointerAction",
                json!([reference, gesture.injection_speed_value(), pressure]),
            )
            .await
            .map(|_| ());
        self.queue_remote_reference(reference, self.generation());
        result
    }

    /// 模拟触控板多指滑动。
    pub async fn touchpad_swipe(
        &self,
        direction: SwipeDirection,
        fingers: u8,
        hold_at_end: bool,
        speed: Option<u32>,
    ) -> Result<()> {
        if !(1..=10).contains(&fingers) {
            return Err(DriverError::InvalidArgument(
                "触控板手指数量必须位于 1 到 10".into(),
            ));
        }
        if let Some(speed) = speed {
            validate_motion_speed(speed)?;
        }
        let mut options = serde_json::Map::new();
        options.insert("stay".into(), json!(hold_at_end));
        if let Some(speed) = speed {
            options.insert("speed".into(), json!(speed));
        }
        self.driver_call(
            "touchPadMultiFingerSwipe",
            json!([fingers, touchpad_direction(direction), options]),
        )
        .await
        .map(|_| ())
    }

    /// 旋转手表表冠。正步数为顺时针，负步数为逆时针。
    pub async fn rotate_crown(&self, steps: i32, speed: Option<u16>) -> Result<()> {
        if steps == 0 {
            return Err(DriverError::InvalidArgument("表冠步数不能为 0".into()));
        }
        if let Some(speed) = speed
            && !(1..=500).contains(&speed)
        {
            return Err(DriverError::InvalidArgument(
                "表冠速度必须位于 1 到 500 格/秒".into(),
            ));
        }
        let mut args = vec![json!(steps)];
        if let Some(speed) = speed {
            args.push(json!(speed));
        }
        self.driver_call("crownRotate", Value::Array(args))
            .await
            .map(|_| ())
    }

    /// 输入文本到当前焦点控件。
    pub async fn input_text(&self, text: &str) -> Result<()> {
        trace!(target: "hm_driver_rs::input", text, "输入文本");
        self.coordinate_call("inputText", json!([{"x": 1, "y": 1}, text]))
            .await
    }

    /// 等待 UI 连续空闲指定时长，最长等待 `timeout`。
    pub async fn wait_for_idle(&self, idle_time: Duration, timeout: Duration) -> Result<()> {
        trace!(target: "hm_driver_rs::input", ?idle_time, ?timeout, "等待 UI 空闲");
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

    async fn create_pointer_matrix(&self, gesture: &Gesture) -> Result<String> {
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
            Ok(())
        }
        .await;
        if let Err(error) = result {
            self.queue_remote_reference(reference, self.generation());
            return Err(error);
        }
        Ok(reference)
    }
}

fn mouse_button_args(point: Point, button: MouseButton, keys: &[KeyCode]) -> Result<Vec<Value>> {
    validate_mouse_keys(keys)?;
    let mut args = vec![json!(point), json!(button.value())];
    args.extend(keys.iter().map(|key| json!(key.value())));
    Ok(args)
}

fn validate_mouse_keys(keys: &[KeyCode]) -> Result<()> {
    if keys.len() <= 2 {
        Ok(())
    } else {
        Err(DriverError::InvalidArgument(
            "鼠标组合操作最多支持两个键盘按键".into(),
        ))
    }
}

fn validate_pressure(pressure: Option<f64>) -> Result<()> {
    if pressure.is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value)) {
        Ok(())
    } else {
        Err(DriverError::InvalidArgument(
            "触控笔压力必须位于 0 到 1".into(),
        ))
    }
}

fn touchpad_direction(direction: SwipeDirection) -> u8 {
    match direction {
        SwipeDirection::Left => 0,
        SwipeDirection::Right => 1,
        SwipeDirection::Up => 2,
        SwipeDirection::Down => 3,
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

#[cfg(test)]
mod extended_input_tests {
    use super::*;
    use crate::rpc::{ApiDialect, RpcClient};
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn sends_mouse_pen_touchpad_and_crown_calls() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            for _ in 0..4 {
                let request: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                server_calls.lock().await.push((
                    request["params"]["api"].as_str().unwrap().to_owned(),
                    request["params"]["args"].clone(),
                ));
                let response = json!({
                    "request_id": request["request_id"],
                    "result": null,
                    "exception": null
                });
                writer
                    .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                    .await
                    .unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
        });
        let rpc = RpcClient::connect(port, Duration::from_secs(1), Duration::from_secs(1), 4096)
            .await
            .unwrap();
        let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
        driver
            .mouse_click(Point::new(10, 20), MouseButton::Right, &[KeyCode::CtrlLeft])
            .await
            .unwrap();
        driver
            .pen_swipe(Point::new(1, 2), Point::new(30, 40), 800, Some(0.5))
            .await
            .unwrap();
        driver
            .touchpad_swipe(SwipeDirection::Up, 3, true, Some(2_000))
            .await
            .unwrap();
        driver.rotate_crown(-5, Some(60)).await.unwrap();

        assert_eq!(
            *calls.lock().await,
            vec![
                (
                    "Driver.mouseClick".into(),
                    json!([{"x": 10, "y": 20}, 1, KeyCode::CtrlLeft.value()])
                ),
                (
                    "Driver.penSwipe".into(),
                    json!([{"x": 1, "y": 2}, {"x": 30, "y": 40}, 800, 0.5])
                ),
                (
                    "Driver.touchPadMultiFingerSwipe".into(),
                    json!([3, 2, {"stay": true, "speed": 2000}])
                ),
                ("Driver.crownRotate".into(), json!([-5, 60]))
            ]
        );
    }
}
