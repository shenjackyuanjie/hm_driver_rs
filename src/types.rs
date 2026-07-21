use crate::{DriverError, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;

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
    Online,
    Offline,
    Unauthorized,
    Unknown(String),
}

/// 发现到的设备摘要。
#[derive(Clone, Debug)]
pub struct DeviceDescriptor {
    pub serial: DeviceSerial,
    pub status: DeviceStatus,
    pub details: Vec<String>,
}

/// 屏幕绝对坐标。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 0 到 1 范围内的归一化坐标。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedPoint {
    pub x: f64,
    pub y: f64,
}

impl NormalizedPoint {
    pub fn new(x: f64, y: f64) -> Result<Self> {
        if x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
            Ok(Self { x, y })
        } else {
            Err(DriverError::InvalidCoordinate(
                "归一化坐标必须位于 0 到 1".into(),
            ))
        }
    }
}

/// 可接受绝对或归一化坐标的位置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    Absolute(Point),
    Normalized(NormalizedPoint),
}

/// 控件边界。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub const fn center(self) -> Point {
        Point::new((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    pub const fn is_valid(self) -> bool {
        self.right >= self.left && self.bottom >= self.top
    }
}

/// 显示区域大小。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySize {
    pub width: u32,
    pub height: u32,
}

/// 显示旋转方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DisplayRotation {
    Rotation0 = 0,
    Rotation90 = 1,
    Rotation180 = 2,
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
    pub product_name: String,
    pub model: String,
    pub brand: String,
    pub api_version: Option<u32>,
    pub system_version: String,
    pub cpu_abi: String,
    pub display_size: DisplaySize,
    pub display_rotation: DisplayRotation,
}

/// 已校验的 HarmonyOS bundle 标识。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIdentifier(String);

impl AppIdentifier {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if is_valid_identifier(&value) {
            Ok(Self(value))
        } else {
            Err(DriverError::InvalidIdentifier(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn validate_ability(value: &str) -> Result<()> {
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
}
