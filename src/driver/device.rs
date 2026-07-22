//! 设备信息、显示与电源状态相关能力。

use super::HmDriver;
use crate::keycode::KeyCode;
use crate::types::{DeviceInfo, DisplayRotation, DisplaySize, Point, ScreenState};
use crate::{DriverError, Result};
use regex::Regex;
use serde_json::json;
use std::net::IpAddr;

impl HmDriver {
    /// 获取当前显示设备的尺寸（宽度 x 高度，单位为像素）。
    pub async fn display_size(&self) -> Result<DisplaySize> {
        let value = self.driver_call("getDisplaySize", json!([])).await?;
        let width = value
            .get("x")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let height = value
            .get("y")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        match (width, height) {
            (Some(width), Some(height)) => Ok(DisplaySize { width, height }),
            _ => Err(DriverError::Protocol("显示尺寸响应无效".into())),
        }
    }

    /// 获取当前显示旋转角度。
    pub async fn display_rotation(&self) -> Result<DisplayRotation> {
        let value = self.driver_call("getDisplayRotation", json!([])).await?;
        DisplayRotation::try_from(
            value
                .as_u64()
                .ok_or_else(|| DriverError::Protocol("显示旋转响应无效".into()))?,
        )
    }

    /// 设置当前显示旋转角度（0°/90°/180°/270°）。
    pub async fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        self.driver_call("setDisplayRotation", json!([rotation as u8]))
            .await
            .map(|_| ())
    }

    /// 收集完整的设备信息（型号、系统版本、CPU ABI、WLAN IP、显示尺寸与旋转角度等）。
    pub async fn device_info(&self) -> Result<DeviceInfo> {
        let product_name = self
            .parameter("const.product.name")
            .await
            .unwrap_or_default();
        let model = self
            .parameter("const.product.model")
            .await
            .unwrap_or_default();
        let brand = self
            .parameter("const.product.brand")
            .await
            .unwrap_or_default();
        let system_version = self
            .parameter("const.product.software.version")
            .await
            .unwrap_or_default();
        let cpu_abi = self
            .parameter("const.product.cpu.abilist")
            .await
            .unwrap_or_default();
        let api_version = self
            .parameter("const.ohos.apiversion")
            .await
            .ok()
            .and_then(|value| value.parse().ok());
        Ok(DeviceInfo {
            product_name,
            model,
            brand,
            api_version,
            system_version,
            cpu_abi,
            wlan_ip: self.wlan_ip().await?,
            display_size: self.display_size().await?,
            display_rotation: self.display_rotation().await?,
        })
    }

    /// 点亮屏幕（通过 `power-shell wakeup`）。
    pub async fn screen_on(&self) -> Result<()> {
        self.inner.hdc.shell("power-shell wakeup").await.map(|_| ())
    }

    /// 熄灭屏幕。仅在屏幕当前为亮屏状态时发送电源键。
    pub async fn screen_off(&self) -> Result<()> {
        if should_toggle_for_screen_off(&self.screen_state().await?)? {
            self.toggle_screen_power().await
        } else {
            Ok(())
        }
    }

    /// 无条件发送一次电源键，用于显式切换屏幕电源状态。
    pub async fn toggle_screen_power(&self) -> Result<()> {
        self.press_key_code(KeyCode::Power).await
    }

    /// 获取当前屏幕电源状态（Awake / Sleep / Inactive）。
    pub async fn screen_state(&self) -> Result<ScreenState> {
        let output = self
            .inner
            .hdc
            .shell("hidumper -s PowerManagerService -a -s")
            .await?;
        parse_screen_state(&output.stdout)
    }

    /// 获取 WLAN 接口的非回环 IPv4/IPv6 地址。
    pub async fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        let output = self.inner.hdc.shell("ifconfig").await?;
        parse_wlan_ip(&output.stdout)
    }

    /// 解锁屏幕：先亮屏，再从底部向上滑动。
    pub async fn unlock(&self) -> Result<()> {
        self.screen_on().await?;
        let size = self.display_size().await?;
        self.swipe(
            Point::new((size.width / 2) as i32, (size.height * 4 / 5) as i32),
            Point::new((size.width / 2) as i32, (size.height / 5) as i32),
            6000,
        )
        .await
    }
}

/// 仅在屏幕为 Awake 时才需要发送电源键关屏。
pub(super) fn should_toggle_for_screen_off(state: &ScreenState) -> Result<bool> {
    match state {
        ScreenState::Awake => Ok(true),
        ScreenState::Inactive | ScreenState::Sleep => Ok(false),
        ScreenState::Unknown(value) => Err(DriverError::Protocol(format!(
            "无法根据未知电源状态 {value} 安全关闭屏幕"
        ))),
    }
}

/// 从 `hidumper -s PowerManagerService -a -s` 的输出中解析屏幕电源状态。
pub(super) fn parse_screen_state(output: &str) -> Result<ScreenState> {
    let pattern = Regex::new(r"Current State:\s*([A-Za-z_]+)")
        .map_err(|error| DriverError::Protocol(error.to_string()))?;
    let raw = pattern
        .captures(output)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_ascii_uppercase())
        .ok_or_else(|| DriverError::Protocol("无法解析屏幕电源状态".into()))?;
    Ok(match raw.as_str() {
        "INACTIVE" => ScreenState::Inactive,
        "SLEEP" => ScreenState::Sleep,
        "AWAKE" => ScreenState::Awake,
        _ => ScreenState::Unknown(raw),
    })
}

/// 从 `ifconfig` 输出中解析 WLAN 接口的非回环 IP 地址。
pub(super) fn parse_wlan_ip(output: &str) -> Result<Option<IpAddr>> {
    let address_pattern = Regex::new(r"(?:inet addr:|inet\s+)([0-9A-Fa-f:.]+)")
        .map_err(|error| DriverError::Protocol(error.to_string()))?;
    let normalized = output.replace("\r\n", "\n");
    let preferred = normalized.split("\n\n").find(|block| {
        block
            .lines()
            .next()
            .map(str::trim_start)
            .is_some_and(|line| line.starts_with("wlan") || line.starts_with("wifi"))
    });
    Ok(
        parse_non_loopback_ip(preferred.unwrap_or(output), &address_pattern)
            .or_else(|| preferred.and_then(|_| parse_non_loopback_ip(output, &address_pattern))),
    )
}

fn parse_non_loopback_ip(output: &str, pattern: &Regex) -> Option<IpAddr> {
    pattern
        .captures_iter(output)
        .filter_map(|capture| capture.get(1)?.as_str().parse::<IpAddr>().ok())
        .find(|address| !address.is_loopback() && !address.is_unspecified())
}
