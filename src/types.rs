use crate::{DriverError, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;

/// 设备序列号。格式化和调试输出始终脱敏。
#[derive(Clone)]
pub struct DeviceSerial(SecretString);

impl DeviceSerial {
    /// 创建序列号包装类型。
    pub fn new(value: impl Into<Box<str>>) -> Self {
        Self(SecretString::from(value.into()))
    }

    /// 显式取得原始序列号。调用方不得将结果写入日志。
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl PartialEq for DeviceSerial {
    fn eq(&self, other: &Self) -> bool {
        self.expose_secret() == other.expose_secret()
    }
}

impl Eq for DeviceSerial {}

impl fmt::Debug for DeviceSerial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceSerial(<redacted>)")
    }
}

impl fmt::Display for DeviceSerial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Builder 的设备选择策略。
#[derive(Clone, Debug, Default)]
pub enum DeviceSelector {
    /// 仅有一台在线设备时自动选择。
    #[default]
    Auto,
    /// 使用指定序列号。
    Serial(DeviceSerial),
}

/// HDC 报告的设备状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceStatus {
    /// 设备在线。
    Online,
    /// 设备离线。
    Offline,
    /// 设备未授权。
    Unauthorized,
    /// 未知状态。
    Unknown(String),
}

/// 发现到的设备摘要。
#[derive(Clone, Debug)]
pub struct DeviceDescriptor {
    /// 设备序列号。
    pub serial: DeviceSerial,
    /// 设备状态。
    pub status: DeviceStatus,
    /// 设备详情。
    pub details: Vec<String>,
}

/// 屏幕绝对坐标。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// 横坐标。
    pub x: i32,
    /// 纵坐标。
    pub y: i32,
}

impl Point {
    /// 使用给定的 x、y 坐标创建一个绝对坐标点。
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 0 到 1 范围内的归一化坐标。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedPoint {
    /// 归一化后的横坐标。
    pub x: f64,
    /// 归一化后的纵坐标。
    pub y: f64,
}

impl NormalizedPoint {
    /// 创建一个归一化坐标点，坐标值必须在 0 到 1 之间。
    pub fn new(x: f64, y: f64) -> Result<Self> {
        if x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
            Ok(Self { x, y })
        } else {
            Err(DriverError::InvalidCoordinate(
                "归一化坐标必须位于 0 到 1".into(),
            ))
        }
    }

    /// 按显示区域换算为有效的绝对像素坐标。
    pub fn resolve(self, display: DisplaySize) -> Result<Point> {
        let max_x = display
            .width
            .checked_sub(1)
            .ok_or_else(|| DriverError::InvalidCoordinate("显示宽度不能为 0".into()))?;
        let max_y = display
            .height
            .checked_sub(1)
            .ok_or_else(|| DriverError::InvalidCoordinate("显示高度不能为 0".into()))?;
        let max_x = i32::try_from(max_x)
            .map_err(|_| DriverError::InvalidCoordinate("显示宽度超出范围".into()))?;
        let max_y = i32::try_from(max_y)
            .map_err(|_| DriverError::InvalidCoordinate("显示高度超出范围".into()))?;
        Ok(Point::new(
            (self.x * f64::from(max_x)).round() as i32,
            (self.y * f64::from(max_y)).round() as i32,
        ))
    }
}

/// 可接受绝对或归一化坐标的位置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    /// 绝对坐标。
    Absolute(Point),
    /// 归一化坐标。
    Normalized(NormalizedPoint),
}

impl Position {
    /// 使用绝对坐标创建一个位置。
    pub const fn absolute(x: i32, y: i32) -> Self {
        Self::Absolute(Point::new(x, y))
    }

    /// 使用归一化坐标创建一个位置。
    pub fn normalized(x: f64, y: f64) -> Result<Self> {
        NormalizedPoint::new(x, y).map(Self::Normalized)
    }

    /// 按显示区域解析绝对或归一化坐标。
    pub fn resolve(self, display: DisplaySize) -> Result<Point> {
        match self {
            Self::Absolute(point) => Ok(point),
            Self::Normalized(point) => point.resolve(display),
        }
    }
}

impl From<Point> for Position {
    fn from(value: Point) -> Self {
        Self::Absolute(value)
    }
}

impl From<NormalizedPoint> for Position {
    fn from(value: NormalizedPoint) -> Self {
        Self::Normalized(value)
    }
}

/// 控件边界。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    /// 左边界。
    pub left: i32,
    /// 上边界。
    pub top: i32,
    /// 右边界。
    pub right: i32,
    /// 下边界。
    pub bottom: i32,
}

impl Bounds {
    /// 返回控件边界的中心点坐标。
    pub const fn center(self) -> Point {
        Point::new((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    /// 检查边界是否有效（right >= left 且 bottom >= top）。
    pub const fn is_valid(self) -> bool {
        self.right >= self.left && self.bottom >= self.top
    }

    /// 返回边界的宽度（right - left）。
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    /// 返回边界的高度（bottom - top）。
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    /// 解析官方 `uitest` 常见的 bounds JSON 形式：
    /// `{"left":..,"top":..,"right":..,"bottom":..}` 或 `[left, top, right, bottom]`。
    pub fn parse_value(value: &serde_json::Value) -> Option<Bounds> {
        match value {
            serde_json::Value::Object(object) => {
                let integer = |name: &str| {
                    object
                        .get(name)?
                        .as_i64()
                        .and_then(|v| i32::try_from(v).ok())
                };
                Some(Bounds {
                    left: integer("left")?,
                    top: integer("top")?,
                    right: integer("right")?,
                    bottom: integer("bottom")?,
                })
            }
            serde_json::Value::Array(values) if values.len() == 4 => {
                let mut numbers = [0_i32; 4];
                for (index, value) in values.iter().enumerate() {
                    numbers[index] = i32::try_from(value.as_i64()?).ok()?;
                }
                Some(Bounds {
                    left: numbers[0],
                    top: numbers[1],
                    right: numbers[2],
                    bottom: numbers[3],
                })
            }
            serde_json::Value::String(value) => Bounds::parse_text(value),
            _ => None,
        }
        .filter(|bounds| bounds.is_valid())
    }

    /// 解析官方 `uitest` 常见的 bounds 文本形式，例如 `[1,2][30,40]`。
    pub fn parse_text(value: &str) -> Option<Bounds> {
        let numbers: Vec<i32> = value
            .split(|ch: char| !ch.is_ascii_digit() && ch != '-')
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse().ok())
            .collect();
        if numbers.len() != 4 {
            return None;
        }
        let bounds = Bounds {
            left: numbers[0],
            top: numbers[1],
            right: numbers[2],
            bottom: numbers[3],
        };
        bounds.is_valid().then_some(bounds)
    }
}

/// 显示区域大小。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySize {
    /// 显示宽度（像素）。
    pub width: u32,
    /// 显示高度（像素）。
    pub height: u32,
}

/// 显示旋转方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DisplayRotation {
    /// 默认方向（0 度）。
    Rotation0 = 0,
    /// 顺时针旋转 90 度。
    Rotation90 = 1,
    /// 顺时针旋转 180 度。
    Rotation180 = 2,
    /// 顺时针旋转 270 度。
    Rotation270 = 3,
}

impl TryFrom<u64> for DisplayRotation {
    type Error = DriverError;

    fn try_from(value: u64) -> Result<Self> {
        match value {
            0 => Ok(Self::Rotation0),
            1 => Ok(Self::Rotation90),
            2 => Ok(Self::Rotation180),
            3 => Ok(Self::Rotation270),
            _ => Err(DriverError::Protocol("显示旋转值超出范围".into())),
        }
    }
}

/// 设备系统与显示信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// 产品名称。
    pub product_name: String,
    /// 设备型号。
    pub model: String,
    /// 设备品牌。
    pub brand: String,
    /// API 版本。
    pub api_version: Option<u32>,
    /// 系统版本。
    pub system_version: String,
    /// CPU ABI。
    pub cpu_abi: String,
    /// WLAN IP 地址。
    pub wlan_ip: Option<IpAddr>,
    /// 显示区域大小。
    pub display_size: DisplaySize,
    /// 显示旋转方向。
    pub display_rotation: DisplayRotation,
}

/// 设备当前屏幕电源状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenState {
    /// 屏幕未激活。
    Inactive,
    /// 屏幕休眠。
    Sleep,
    /// 屏幕唤醒。
    Awake,
    /// 未知状态。
    Unknown(String),
}

/// 打开 URL 时使用的目标。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenUrlMode {
    /// 通过系统浏览器打开。
    #[default]
    SystemBrowser,
    /// 使用系统默认的 URL 路由规则。
    Default,
}

/// 截图命令选择。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenshotMethod {
    /// 优先使用 snapshot_display，失败时回退到 UITest screenCap。
    #[default]
    Auto,
    /// 使用 snapshot display 截图。
    SnapshotDisplay,
    /// 使用 UITest screenCap 截图。
    ScreenCap,
}

/// HDC forward 的端点。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForwardEndpoint {
    /// TCP 端点。
    Tcp(u16),
    /// 本地抽象套接字端点。
    LocalAbstract(String),
    /// 其他类型的端点。
    Other(String),
}

/// 一条 HDC forward 映射。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardEntry {
    /// 本地端点。
    pub local: ForwardEndpoint,
    /// 远程端点。
    pub remote: ForwardEndpoint,
}

/// 页面滑动方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeDirection {
    /// 向上滑动。
    Up,
    /// 向下滑动。
    Down,
    /// 向左滑动。
    Left,
    /// 向右滑动。
    Right,
}

/// 鼠标按键。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MouseButton {
    /// 鼠标左键。
    #[default]
    Left = 0,
    /// 鼠标右键。
    Right = 1,
    /// 鼠标中键。
    Middle = 2,
}

impl MouseButton {
    pub(crate) const fn value(self) -> u8 {
        self as u8
    }
}

/// UI 事件监听类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEventType {
    /// Toast 出现。
    ToastShow,
    /// Dialog 出现。
    DialogShow,
}

impl UiEventType {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ToastShow => "toastShow",
            Self::DialogShow => "dialogShow",
        }
    }
}

/// UITest Agent 返回的一次 UI 事件。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiEvent {
    /// 产生事件的应用包名。
    #[serde(rename = "bundleName", default)]
    pub bundle_name: Option<String>,
    /// 事件关联文本。
    #[serde(default)]
    pub text: Option<String>,
    /// 控件或事件类型，如 `Toast`、`AlertDialog`。
    #[serde(rename = "type", default)]
    pub event_type: Option<String>,
    /// Agent 返回的其他字段。
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// 窗口显示模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowMode {
    /// 全屏窗口。
    Fullscreen = 0,
    /// 主分屏窗口。
    Primary = 1,
    /// 次分屏窗口。
    Secondary = 2,
    /// 悬浮窗口。
    Floating = 3,
}

impl TryFrom<i64> for WindowMode {
    type Error = DriverError;

    fn try_from(value: i64) -> Result<Self> {
        match value {
            0 => Ok(Self::Fullscreen),
            1 => Ok(Self::Primary),
            2 => Ok(Self::Secondary),
            3 => Ok(Self::Floating),
            _ => Err(DriverError::Protocol(format!("未知窗口模式：{value}"))),
        }
    }
}

/// 调整窗口大小时使用的方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ResizeDirection {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
    LeftUp = 4,
    LeftDown = 5,
    RightUp = 6,
    RightDown = 7,
}

impl ResizeDirection {
    pub(crate) const fn value(self) -> u8 {
        self as u8
    }
}

/// 用于查找窗口的组合条件。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focused: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actived: Option<bool>,
}

impl WindowFilter {
    /// 创建空窗口过滤器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 按窗口标题过滤。
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 按应用包名过滤。
    pub fn bundle(mut self, bundle: &AppIdentifier) -> Self {
        self.bundle_name = Some(bundle.as_str().to_owned());
        self
    }

    /// 按焦点状态过滤。
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = Some(focused);
        self
    }

    /// 按活动状态过滤。
    pub fn active(mut self, active: bool) -> Self {
        self.actived = Some(active);
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.bundle_name.is_none()
            && self.focused.is_none()
            && self.actived.is_none()
    }
}

/// 方向滑动使用的屏幕区域。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SwipeArea {
    /// 全屏区域。
    #[default]
    FullScreen,
    /// 绝对坐标区域。
    Absolute(Bounds),
    /// 归一化坐标区域。
    Normalized {
        /// 左上角归一化坐标。
        top_left: NormalizedPoint,
        /// 右下角归一化坐标。
        bottom_right: NormalizedPoint,
    },
}

impl SwipeArea {
    /// 使用归一化坐标创建一个方向滑动区域。
    pub fn normalized(left: f64, top: f64, right: f64, bottom: f64) -> Result<Self> {
        let top_left = NormalizedPoint::new(left, top)?;
        let bottom_right = NormalizedPoint::new(right, bottom)?;
        if right <= left || bottom <= top {
            return Err(DriverError::InvalidCoordinate(
                "归一化滑动区域必须具有正宽度和正高度".into(),
            ));
        }
        Ok(Self::Normalized {
            top_left,
            bottom_right,
        })
    }

    pub(crate) fn resolve(self, display: DisplaySize) -> Result<Bounds> {
        let width = i32::try_from(display.width)
            .map_err(|_| DriverError::InvalidCoordinate("显示宽度超出范围".into()))?;
        let height = i32::try_from(display.height)
            .map_err(|_| DriverError::InvalidCoordinate("显示高度超出范围".into()))?;
        let bounds = match self {
            Self::FullScreen => Bounds {
                left: 0,
                top: 0,
                right: width.saturating_sub(1),
                bottom: height.saturating_sub(1),
            },
            Self::Absolute(bounds) => bounds,
            Self::Normalized {
                top_left,
                bottom_right,
            } => {
                let top_left = top_left.resolve(display)?;
                let bottom_right = bottom_right.resolve(display)?;
                Bounds {
                    left: top_left.x,
                    top: top_left.y,
                    right: bottom_right.x,
                    bottom: bottom_right.y,
                }
            }
        };
        if !bounds.is_valid() || bounds.width() <= 0 || bounds.height() <= 0 {
            return Err(DriverError::InvalidCoordinate("滑动区域无效".into()));
        }
        Ok(bounds)
    }
}

/// 从 bundle 元数据中解析出的 Ability。
#[derive(Clone, Debug, PartialEq)]
pub struct AbilityInfo {
    /// Ability 名称。
    pub name: String,
    /// 模块名称。
    pub module_name: String,
    /// 模块主 Ability。
    pub module_main_ability: Option<String>,
    /// 主模块名称。
    pub main_module: Option<String>,
    /// 是否为启动器 Ability。
    pub is_launcher: bool,
    /// `bm dump` 中对应 Ability 的原始对象，保留平台扩展字段。
    pub raw: serde_json::Value,
}

/// 已校验的 HarmonyOS bundle 标识。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIdentifier(String);

impl AppIdentifier {
    /// 创建一个新的应用标识符，同时校验是否满足 HarmonyOS 标识符规则。
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if is_valid_identifier(&value) {
            Ok(Self(value))
        } else {
            Err(DriverError::InvalidIdentifier(value))
        }
    }

    /// 返回原始 bundle 标识字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 校验 Ability 类名是否符合 HarmonyOS 标识符规则，供在 [`crate::HmDriver::start_app`]
/// 之前预先检查使用。
pub fn validate_ability(value: &str) -> Result<()> {
    if is_valid_identifier(value) {
        Ok(())
    } else {
        Err(DriverError::InvalidIdentifier(value.to_owned()))
    }
}

fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 127
                && part.chars().enumerate().all(|(index, ch)| {
                    ch == '_' || ch.is_ascii_alphanumeric() && (index > 0 || !ch.is_ascii_digit())
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_format_is_always_redacted() {
        let serial = DeviceSerial::new("sensitive-device-id");
        assert_eq!(serial.to_string(), "<redacted>");
        assert_eq!(format!("{serial:?}"), "DeviceSerial(<redacted>)");
        assert_eq!(serial.expose_secret(), "sensitive-device-id");
    }

    #[test]
    fn validates_application_identifiers() {
        assert!(AppIdentifier::new("com.example.demo").is_ok());
        assert!(AppIdentifier::new("com.example;rm").is_err());
        assert!(AppIdentifier::new("1com.example").is_err());
    }

    #[test]
    fn resolves_normalized_swipe_area() {
        let area = SwipeArea::normalized(0.1, 0.2, 0.9, 0.8).unwrap();
        assert_eq!(
            area.resolve(DisplaySize {
                width: 1000,
                height: 2000,
            })
            .unwrap(),
            Bounds {
                left: 100,
                top: 400,
                right: 899,
                bottom: 1599,
            }
        );
    }

    #[test]
    fn normalized_position_stays_inside_display() {
        let display = DisplaySize {
            width: 1000,
            height: 2000,
        };
        assert_eq!(
            Position::normalized(1.0, 1.0)
                .unwrap()
                .resolve(display)
                .unwrap(),
            Point::new(999, 1999)
        );
        assert!(
            NormalizedPoint::new(0.5, 0.5)
                .unwrap()
                .resolve(DisplaySize {
                    width: 0,
                    height: 2000,
                })
                .is_err()
        );
    }
}
