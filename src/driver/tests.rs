use super::app::{parse_ability_infos, select_main_ability, shell_quote};
use super::device::{parse_screen_state, parse_wlan_ip, should_toggle_for_screen_off};
use super::session::{extract_four_part_version, singleness_pids};
use super::*;
use crate::rpc::ApiDialect;
use crate::types::{Position, ScreenState};
use crate::{DriverError, Gesture, GesturePath, NormalizedPoint};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;

#[test]
fn extracts_one_strict_four_part_version() {
    assert_eq!(
        extract_four_part_version("UITest version 6.0.2.2\n").unwrap(),
        "6.0.2.2"
    );
    assert!(matches!(
        extract_four_part_version("6.0.2"),
        Err(DriverError::InvalidUitestVersion)
    ));
    assert!(matches!(
        extract_four_part_version("6.0.2.2 6.0.2.3"),
        Err(DriverError::InvalidUitestVersion)
    ));
}

#[test]
fn only_matches_exact_singleness_process() {
    let output =
        "shell 100 1 0 uitest start-daemon singleness\nshell 101 1 0 uitest start-daemon demo\n";
    assert_eq!(singleness_pids(output).collect::<Vec<_>>(), vec!["100"]);
}

#[test]
fn discovers_and_ranks_all_abilities() {
    let value = json!({
        "mainEntry": "entry",
        "hapModuleInfos": [
            {
                "moduleName": "feature",
                "mainAbility": "FeatureAbility",
                "abilityInfos": [{"name": "FeatureAbility", "moduleName": "feature", "skills": []}]
            },
            {
                "moduleName": "entry",
                "mainAbility": "EntryAbility",
                "abilityInfos": [
                    {"name": "OtherAbility", "moduleName": "entry", "skills": []},
                    {"name": "EntryAbility", "moduleName": "entry", "skills": [{"actions": ["action.system.home"]}]}
                ]
            }
        ]
    });
    let abilities = parse_ability_infos(&value);
    assert_eq!(abilities.len(), 3);
    let selected = select_main_ability(abilities).unwrap();
    assert_eq!(selected.name, "EntryAbility");
    assert!(selected.is_launcher);
    assert_eq!(selected.raw["moduleName"], "entry");
}

#[test]
fn discovers_abilities_inside_wrapped_result() {
    let value = json!({
        "result": {
            "mainEntry": "entry",
            "hapModuleInfos": [{
                "moduleName": "entry",
                "mainAbility": "EntryAbility",
                "abilityInfos": [
                    {"name": "EntryAbility", "moduleName": "entry", "skills": []},
                    {"name": "ShareAbility", "moduleName": "entry", "skills": []}
                ]
            }]
        }
    });
    let abilities = parse_ability_infos(&value);
    assert_eq!(abilities.len(), 2);
    assert_eq!(abilities[0].main_module.as_deref(), Some("entry"));
    assert_eq!(abilities[1].name, "ShareAbility");
}

#[test]
fn parses_screen_state_and_non_loopback_ip() {
    assert_eq!(
        parse_screen_state("Current State: AWAKE\n").unwrap(),
        ScreenState::Awake
    );
    assert_eq!(
        parse_wlan_ip("inet addr:127.0.0.1\ninet 192.168.1.20 netmask 255.255.255.0").unwrap(),
        Some("192.168.1.20".parse().unwrap())
    );
    assert_eq!(
        parse_wlan_ip(
            "rmnet0 Link encap:Ethernet\n  inet addr:10.0.0.2\n\nwlan0 Link encap:Ethernet\n  inet addr:192.168.1.20\n"
        )
        .unwrap(),
        Some("192.168.1.20".parse().unwrap())
    );
}

#[test]
fn screen_off_only_toggles_an_awake_display() {
    assert!(should_toggle_for_screen_off(&ScreenState::Awake).unwrap());
    assert!(!should_toggle_for_screen_off(&ScreenState::Sleep).unwrap());
    assert!(!should_toggle_for_screen_off(&ScreenState::Inactive).unwrap());
    assert!(should_toggle_for_screen_off(&ScreenState::Unknown("DOZE".into())).is_err());
}

#[tokio::test]
async fn generic_wait_honors_condition_and_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _connection = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let rpc = RpcClient::connect(port, Duration::from_secs(1), Duration::from_secs(1), 1024)
        .await
        .unwrap();
    let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
    let attempts = std::sync::atomic::AtomicUsize::new(0);
    assert!(
        driver
            .wait_until_with_interval(Duration::from_secs(1), Duration::from_millis(1), || async {
                Ok(attempts.fetch_add(1, Ordering::Relaxed) >= 2)
            },)
            .await
            .unwrap()
    );
    let started = tokio::time::Instant::now();
    assert!(
        !driver
            .wait_until_with_interval(
                Duration::from_millis(20),
                Duration::from_millis(2),
                || async { Ok(false) },
            )
            .await
            .unwrap()
    );
    assert!(started.elapsed() < Duration::from_millis(150));
}

#[tokio::test]
async fn selector_wait_cancels_a_slow_rpc_at_its_total_deadline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, _writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let _ = lines.next_line().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let rpc = RpcClient::connect(port, Duration::from_secs(1), Duration::from_secs(1), 1024)
        .await
        .unwrap();
    let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
    let started = tokio::time::Instant::now();
    assert!(matches!(
        driver
            .wait_for(&crate::Selector::new(), Duration::from_millis(20))
            .await,
        Err(DriverError::ElementNotFound)
    ));
    assert!(started.elapsed() < Duration::from_millis(150));
}

#[tokio::test]
async fn invalid_component_arrays_queue_references_already_returned() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let request: serde_json::Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let response = json!({
            "request_id": request["request_id"],
            "result": ["Component#1", 7],
            "exception": null
        });
        writer
            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
    });
    let rpc = RpcClient::connect(port, Duration::from_secs(1), Duration::from_secs(1), 1024)
        .await
        .unwrap();
    let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
    assert!(matches!(
        driver.find_remote_references(&crate::Selector::new()).await,
        Err(DriverError::Protocol(_))
    ));
    assert_eq!(driver.queued_reference_count(), 1);
}

#[test]
fn quotes_device_shell_url_as_one_argument() {
    assert_eq!(
        shell_quote("https://example.com/a'b"),
        "'https://example.com/a'\\''b'"
    );
}

#[tokio::test]
async fn submits_pointer_matrix_before_injecting_gesture() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(TokioMutex::new(Vec::new()));
    let server_calls = calls.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            let api = request["params"]["api"].as_str().unwrap().to_owned();
            server_calls.lock().await.push(api.clone());
            let result = match api.as_str() {
                "Driver.getDisplaySize" => json!({"x": 1000, "y": 2000}),
                "PointerMatrix.create" => json!("PointerMatrix#1"),
                _ => serde_json::Value::Null,
            };
            let response = json!({
                "request_id": request["request_id"],
                "result": result,
                "exception": null
            });
            writer
                .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                .await
                .unwrap();
            writer.write_all(b"\n").await.unwrap();
            if api == "Driver.injectMultiPointerAction" {
                break;
            }
        }
    });
    let rpc = RpcClient::connect(
        port,
        Duration::from_secs(1),
        Duration::from_secs(1),
        1024 * 1024,
    )
    .await
    .unwrap();
    let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
    let path = GesturePath::new(
        Position::Normalized(NormalizedPoint::new(0.2, 0.2).unwrap()),
        Duration::from_millis(50),
    )
    .unwrap()
    .move_to(
        Position::Normalized(NormalizedPoint::new(0.8, 0.8).unwrap()),
        Duration::from_millis(50),
    )
    .unwrap();
    driver.perform_gesture(&Gesture::new(path)).await.unwrap();
    let calls = calls.lock().await;
    assert_eq!(calls[0], "Driver.getDisplaySize");
    assert_eq!(calls[1], "PointerMatrix.create");
    assert_eq!(
        calls
            .iter()
            .filter(|api| api.as_str() == "PointerMatrix.setPoint")
            .count(),
        3
    );
    assert_eq!(calls.last().unwrap(), "Driver.injectMultiPointerAction");
}

#[tokio::test]
async fn submits_extended_input_methods_with_official_argument_shapes() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(TokioMutex::new(Vec::new()));
    let server_calls = calls.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        for _ in 0..4 {
            let request: serde_json::Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            server_calls.lock().await.push((
                request["params"]["api"].as_str().unwrap().to_owned(),
                request["params"]["args"].clone(),
            ));
            let response = json!({
                "request_id": request["request_id"],
                "result": null,
                "exception": null
            });
            writer
                .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                .await
                .unwrap();
            writer.write_all(b"\n").await.unwrap();
        }
    });
    let rpc = RpcClient::connect(
        port,
        Duration::from_secs(1),
        Duration::from_secs(1),
        1024 * 1024,
    )
    .await
    .unwrap();
    let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
    driver
        .press_key_combination(&[crate::KeyCode::CtrlLeft, crate::KeyCode::A])
        .await
        .unwrap();
    driver
        .drag(crate::Point::new(1, 2), crate::Point::new(3, 4), 600)
        .await
        .unwrap();
    driver
        .fling(crate::Point::new(5, 6), crate::Point::new(7, 8), 30, 2_000)
        .await
        .unwrap();
    driver
        .wait_for_idle(Duration::from_millis(100), Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(
        *calls.lock().await,
        vec![
            ("Driver.triggerCombineKeys".into(), json!([2072, 2017])),
            ("Driver.drag".into(), json!([1, 2, 3, 4, 600])),
            (
                "Driver.fling".into(),
                json!([{"x": 5, "y": 6}, {"x": 7, "y": 8}, 30, 2000]),
            ),
            ("Driver.waitForIdle".into(), json!([100, 2000])),
        ]
    );
}
