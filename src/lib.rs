//! HarmonyOS 原生 UI 自动化驱动。
//!
//! 本 crate 通过 HDC 启动官方 UITest Agent，再使用 Hypium JSON RPC 操作设备。

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
    AbilityInfo, AppIdentifier, Bounds, DeviceDescriptor, DeviceInfo, DeviceSelector, DeviceSerial,
    DeviceStatus, DisplayRotation, DisplaySize, ForwardEndpoint, ForwardEntry, MouseButton,
    NormalizedPoint, OpenUrlMode, Point, Position, ResizeDirection, ScreenState, ScreenshotMethod,
    SwipeArea, SwipeDirection, UiEvent, UiEventType, WindowFilter, WindowMode, validate_ability,
};
pub use ui::UiNode;
pub use window::UiWindow;
pub use xpath::XPathElement;

pub use driver::DriverConfig;
