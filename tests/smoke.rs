//! 真机冒烟测试。
//!
//! 测试默认忽略，并且仅在 `HM_DRIVER_SMOKE=1` 时执行。测试不会输出或持久化设备标识。

use hm_driver_rs::{
    AppIdentifier, DeviceSelector, DeviceSerial, Gesture, GesturePath, HmDriver, KeyCode,
    NormalizedPoint, Position, ScreenshotMethod, Selector, SwipeArea, SwipeDirection,
};
use std::time::Duration;

#[tokio::test]
#[ignore = "需要显式连接 HarmonyOS 真机"]
async fn arm64_v122_smoke() -> hm_driver_rs::Result<()> {
    if std::env::var("HM_DRIVER_SMOKE").as_deref() != Ok("1") {
        return Ok(());
    }
    let serial =
        std::env::var("HM_DRIVER_DEVICE").expect("真机测试要求通过 HM_DRIVER_DEVICE 显式指定设备");
    eprintln!("阶段：连接 Driver");
    let driver = HmDriver::builder()
        .device(DeviceSelector::Serial(DeviceSerial::new(serial)))
        .connect()
        .await?;
    eprintln!("阶段：读取设备状态");
    assert_eq!(driver.agent_profile().version, "1.2.2");
    driver.screen_on().await?;
    let _ = driver.screen_state().await?;
    let _ = driver.wlan_ip().await?;
    let size = driver.display_size().await?;
    assert!(size.width > 0 && size.height > 0);
    eprintln!("阶段：snapshot_display 截图");
    assert!(
        !driver
            .screenshot_with_method(ScreenshotMethod::SnapshotDisplay)
            .await?
            .is_empty()
    );
    eprintln!("阶段：screenCap 截图");
    assert!(
        !driver
            .screenshot_with_method(ScreenshotMethod::ScreenCap)
            .await?
            .is_empty()
    );
    eprintln!("阶段：查询 forward 和 Ability");
    assert!(!driver.list_forwards().await?.is_empty());

    let bundle = AppIdentifier::new("com.chinadaily.har")?;
    let abilities = driver.app_abilities(&bundle).await?;
    assert!(abilities.iter().any(|item| item.name == "EntryAbility"));
    assert_eq!(
        driver
            .main_ability_info(&bundle)
            .await?
            .map(|item| item.name),
        Some("EntryAbility".into())
    );

    eprintln!("阶段：按键、滑动和多指轨迹");
    driver.press_key_code(KeyCode::Home).await?;
    driver
        .swipe_direction(SwipeDirection::Up, SwipeArea::FullScreen, 0.2, 2_000)
        .await?;
    let left = GesturePath::new(
        Position::Normalized(NormalizedPoint::new(0.45, 0.5)?),
        Duration::from_millis(50),
    )?
    .move_to(
        Position::Normalized(NormalizedPoint::new(0.46, 0.5)?),
        Duration::from_millis(50),
    )?;
    let right = GesturePath::new(
        Position::Normalized(NormalizedPoint::new(0.55, 0.5)?),
        Duration::from_millis(50),
    )?
    .move_to(
        Position::Normalized(NormalizedPoint::new(0.54, 0.5)?),
        Duration::from_millis(50),
    )?;
    driver
        .perform_gesture(&Gesture::new(left).add_path(right)?)
        .await?;

    eprintln!("阶段：UI 树和控件便利方法");
    let tree = driver.ui_tree().await?;
    assert!(tree.node_type().is_some() || !tree.children.is_empty());
    let xpath = driver.xpath_optional("//*").await?;
    assert!(xpath.is_some());
    let selector = Selector::new().enabled(true);
    assert!(driver.exists(&selector).await?);
    assert!(driver.count(&selector).await? > 0);
    if let Some(element) = driver.find(&selector).await? {
        assert!(element.is_enabled().await?);
        let _ = element.type_name().await?;
        let _ = element.bounds_center().await?;
    }
    driver.close().await
}
