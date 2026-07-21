//! HarmonyOS 原生 UI 自动化驱动。
//!
//! 本 crate 通过 HDC 启动官方 UITest Agent，再使用 Hypium JSON RPC 操作设备。

mod agent;
mod catalog;
mod driver;
mod error;
mod hdc;
mod rpc;
mod selector;
mod types;
mod ui;
mod xpath;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use agent::{AgentProfile, AgentSource, CompatibilityStatus, HarmonyTransport};
pub use driver::{HmDriver, HmDriverBuilder};
pub use error::{DriverError, Result};
pub use hdc::{CommandOutput, HdcConfig};
pub use selector::{Element, MatchPattern, Selector};
pub use types::{
    AppIdentifier, Bounds, DeviceDescriptor, DeviceInfo, DeviceSelector, DeviceSerial,
    DeviceStatus, DisplayRotation, DisplaySize, NormalizedPoint, Point, Position,
};
pub use ui::UiNode;
pub use xpath::XPathElement;

pub use driver::DriverConfig;
