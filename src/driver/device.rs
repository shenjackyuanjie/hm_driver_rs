//! 设备信息、显示与电源状态相关能力。

use super::HmDriver;
use crate::keycode::KeyCode;
use crate::types::{DeviceInfo, DisplayRotation, DisplaySize, Point, ScreenState};
use crate::{DriverError, Result};
use regex::Regex;
use serde_json::json;
use std::net::IpAddr;

impl HmDriver {
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

    pub async fn display_rotation(&self) -> Result<DisplayRotation> {
        let value = self.driver_call("getDisplayRotation", json!([])).await?;
        DisplayRotation::try_from(
            value
                .as_u64()
                .ok_or_else(|| DriverError::Protocol("显示旋转响应无效".into()))?,
        )
    }

    pub async fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        self.driver_call("setDisplayRotation", json!([rotation as u8]))
            .await
            .map(|_| ())
    }

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

    pub async fn screen_on(&self) -> Result<()> {
        self.inner.hdc.shell("power-shell wakeup").await.map(|_| ())
    }

    pub async fn screen_off(&self) -> Result<()> {
        self.press_key_code(KeyCode::Power).await
    }

    pub async fn screen_state(&self) -> Result<ScreenState> {
        let output = self
            .inner
            .hdc
            .shell("hidumper -s PowerManagerService -a -s")
            .await?;
        parse_screen_state(&output.stdout)
    }

    pub async fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        let output = self.inner.hdc.shell("ifconfig").await?;
        parse_wlan_ip(&output.stdout)
    }

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
