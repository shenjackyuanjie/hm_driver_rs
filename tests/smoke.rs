//! 真机冒烟测试。
//!
//! 测试默认忽略，并且仅在 `HM_DRIVER_SMOKE=1` 时执行。测试不会输出或持久化设备标识。

use hm_driver_rs::{HmDriver, Selector};

#[tokio::test]
#[ignore = "需要显式连接 HarmonyOS 真机"]
async fn arm64_v122_smoke() -> hm_driver_rs::Result<()> {
    if std::env::var("HM_DRIVER_SMOKE").as_deref() != Ok("1") {
        return Ok(());
    }
    let driver = HmDriver::builder().connect().await?;
    assert_eq!(driver.agent_profile().version, "1.2.2");
    driver.screen_on().await?;
    let size = driver.display_size().await?;
    assert!(size.width > 0 && size.height > 0);
    assert!(!driver.screenshot().await?.is_empty());
    let tree = driver.ui_tree().await?;
    assert!(tree.node_type().is_some() || !tree.children.is_empty());
    let _ = driver.xpath_exists("//*").await?;
    let _ = driver.find_all(&Selector::new().enabled(true)).await?;
    driver.close().await
}
