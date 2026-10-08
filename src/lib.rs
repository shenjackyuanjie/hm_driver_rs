//! HarmonyOS 原生 UI 自动化驱动。
//!
//! 本 crate 通过 HDC 启动官方 UITest Agent，再使用 Hypium JSON RPC 操作设备。

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
