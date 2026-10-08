//! HarmonyOS 原生 UI 自动化驱动。
//!
//! 本 crate 通过 HDC 启动官方 UITest Agent，再使用 Hypium JSON RPC 操作设备。
//!
//! # 接入
//!
//! 主机安装 HDC，设备开启调试并完成授权。异步 API 在启用 I/O 和时间驱动的
//! Tokio runtime 中运行；默认启用内嵌 Agent 和同步 `blocking` 门面。
//!
//! ```no_run
//! use hm_driver_rs::{HmDriver, Result, Selector};
//! use std::time::Duration;
//!
//! # fn main() -> Result<()> {
//! let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
//! runtime.block_on(async {
//!     let driver = HmDriver::builder().connect().await?;
//!     let operation = async {
//!         let button = driver.wait_for(&Selector::new().text("确定"), Duration::from_secs(5)).await?;
//!         button.click().await
//!     }.await;
//!     let cleanup = driver.close().await;
//!     operation?;
//!     cleanup
//! })
//! # }
//! ```
//!
//! # API 导航
//!
//! - [`HmDriverBuilder`] / [`HdcConfig`]：设备选择、HDC 路径、Agent 来源和连接配置。
//! - [`HmDriver`]：设备、输入、应用、文件、系统辅助、查询和会话生命周期。
//! - [`Selector`] / [`Element`]：远端条件定位及控件操作。
//! - [`UiNode`] / [`XPathElement`]：UI 树快照、本地查询和基于快照坐标的操作。
//! - [`WindowFilter`] / [`UiWindow`]：窗口定位、属性及窗口管理。
//! - [`GesturePath`] / [`Gesture`]：单指路径和多指轨迹。
//! - [`DriverError`] / [`Result`]：参数、设备工具、RPC 与清理错误。
//!
//! # Features
//!
//! - `blocking`（默认）：同步 API 和进程级 Tokio 多线程 runtime。
//! - `embedded-agents`（默认）：将官方 `.so` 编译进 crate，连接时校验并缓存。
//!
//! 关闭 `embedded-agents` 时，通过 [`AgentSource::Directory`] 指定官方 Agent 目录。
//! 仅使用异步 API 时可以关闭 `blocking`；以下对齐记录在全部 feature 组合中可查阅。

#![doc = include_str!("../docs/hypium-alignment.md")]

/// 当前对齐的官方 Hypium 软件包版本。
///
/// 用于 API 行为核对与 [`AgentCatalog`] 的资源来源校验。
/// 已有功能和待补功能逐项记录在 crate 文档的「Hypium 对齐记录」中。
///
/// ```
/// assert_eq!(hm_driver_rs::HYPIUM_ALIGNMENT_VERSION, "26.0.0.500");
/// ```
pub const HYPIUM_ALIGNMENT_VERSION: &str = "26.0.0.500";

mod agent;
mod catalog;
mod driver;
mod error;
mod gesture;
mod hdc;
mod keycode;
mod rpc;
mod selector;
mod types;
mod ui;
mod window;
mod xpath;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use agent::{AgentProfile, AgentResolver, AgentSource, CompatibilityStatus, HarmonyTransport};
pub use catalog::AgentCatalog;
pub use driver::{HmDriver, HmDriverBuilder};
pub use error::{DriverError, Result};
pub use gesture::{Gesture, GesturePath};
pub use hdc::{CommandOutput, HdcConfig};
pub use keycode::KeyCode;
pub use rpc::ApiDialect;
pub use selector::{Element, ElementInfo, MatchPattern, Selector};
pub use types::{
    AbilityInfo, AppIdentifier, Bounds, BuiltinNetworkScenario, DeviceDescriptor, DeviceInfo,
    DeviceSelector, DeviceSerial, DeviceStatus, DisplayRotation, DisplaySize, ForwardEndpoint,
    ForwardEntry, MouseButton, NetworkScenario, NetworkScenarioInfo, NormalizedPoint, OpenUrlMode,
    Point, Position, ResizeDirection, ScreenState, ScreenshotMethod, SwipeArea, SwipeDirection,
    UiEvent, UiEventType, ViewMode, WindowFilter, WindowMode, validate_ability,
};
pub use ui::UiNode;
pub use window::UiWindow;
pub use xpath::XPathElement;

pub use driver::DriverConfig;
