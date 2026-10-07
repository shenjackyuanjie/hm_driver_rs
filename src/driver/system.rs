//! Hypium 26 系统辅助能力：剪贴板、显示模式、时间时区、字体与网络模拟。

use super::app::shell_quote;
use super::{HmDriver, RemoteFileGuard, next_operation_id};
use crate::types::{BuiltinNetworkScenario, NetworkScenario, NetworkScenarioInfo, ViewMode};
use crate::{DriverError, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;

impl HmDriver {
    /// 设置系统界面为深色或浅色模式。
    ///
    /// 该能力依赖设备端 `testhelper`。
    pub async fn set_view_mode(&self, mode: ViewMode) -> Result<()> {
        let output = self
            .run_testhelper(
                &format!("set-viewmode {}", shell_quote(mode.as_str())),
                "设置界面显示模式",
            )
            .await?;
        let lower = output.to_ascii_lowercase();
        if !has_failure_word(&lower)
            && (has_success_word(&lower) || lower.contains("already set to"))
        {
            Ok(())
        } else {
            Err(unexpected_tool_output("设置界面显示模式", &output))
        }
    }

    /// 将系统时间设置为 `YYYY-MM-DD HH:MM:SS`。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn set_system_time(&self, value: &str) -> Result<()> {
        let value = value.trim();
        if !is_valid_system_time(value) {
            return Err(DriverError::InvalidArgument(
                "系统时间必须是有效的 YYYY-MM-DD HH:MM:SS".into(),
            ));
        }
        let output = self
            .run_testhelper(&format!("set-time {}", shell_quote(value)), "设置系统时间")
            .await?;
        require_successfully("设置系统时间", &output)
    }

    /// 读取系统时间，返回 `YYYY-MM-DD HH:MM:SS`。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn system_time(&self) -> Result<String> {
        let output = self.run_testhelper("get-time", "读取系统时间").await?;
        let value = prefixed_value(&output, "Current system time:")
            .ok_or_else(|| unexpected_tool_output("读取系统时间", &output))?;
        if is_valid_system_time(value) {
            Ok(value.to_owned())
        } else {
            Err(unexpected_tool_output("读取系统时间", &output))
        }
    }

    /// 设置 IANA 时区，例如 `Asia/Shanghai` 或 `Etc/UTC`。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn set_timezone(&self, timezone: &str) -> Result<()> {
        let timezone = validated_text_argument(timezone, "时区")?;
        let output = self
            .run_testhelper(
                &format!("set-timezone {}", shell_quote(timezone)),
                "设置系统时区",
            )
            .await?;
        require_successfully("设置系统时区", &output)
    }

    /// 读取当前 IANA 时区标识。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn timezone(&self) -> Result<String> {
        let output = self.run_testhelper("get-timezone", "读取系统时区").await?;
        prefixed_value(&output, "Current timezone:")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| unexpected_tool_output("读取系统时区", &output))
    }

    /// 将文本写入系统剪贴板。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn write_clipboard(&self, value: &str) -> Result<()> {
        if value.contains('\0') {
            return Err(DriverError::InvalidArgument(
                "剪贴板文本不能包含 NUL 字符".into(),
            ));
        }
        let output = self
            .run_testhelper(
                &format!("set-pastedata {}", shell_quote(value)),
                "写入系统剪贴板",
            )
            .await?;
        require_successfully("写入系统剪贴板", &output)
    }

    /// 读取系统剪贴板文本；剪贴板为空时返回空字符串。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn read_clipboard(&self) -> Result<String> {
        self.ensure_testhelper("读取系统剪贴板").await?;
        let output = self
            .inner
            .hdc
            .shell_with_stdout_prefix("testhelper get-pastedata", "Pasteboard text:")
            .await?;
        clipboard_value(&output.stdout)
            .map(str::to_owned)
            .ok_or_else(|| unexpected_tool_output("读取系统剪贴板", &output.stdout))
    }

    /// 清空系统剪贴板。
    ///
    /// 该能力依赖设备端 `testhelper`，官方要求系统版本不低于 7.0.0。
    pub async fn clear_clipboard(&self) -> Result<()> {
        let output = self
            .run_testhelper("clear-pastedata", "清空系统剪贴板")
            .await?;
        require_successfully("清空系统剪贴板", &output)
    }

    /// 读取本地字体文件声明的字体名称。
    ///
    /// 文件会临时推送到设备，操作完成后会尽力清理。该能力依赖设备端 `testhelper`。
    pub async fn font_name(&self, local: impl AsRef<Path>) -> Result<String> {
        let local = local.as_ref();
        let remote = remote_font_path(local);
        let guard = RemoteFileGuard::new(self.inner.hdc.clone(), remote.clone());
        let result = async {
            self.inner.hdc.send_file(local, &remote).await?;
            let output = self
                .run_testhelper(
                    &format!("get-fontname {}", shell_quote(&remote)),
                    "读取字体名称",
                )
                .await?;
            prefixed_value(&output, "Font name:")
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| unexpected_tool_output("读取字体名称", &output))
        }
        .await;
        guard.cleanup().await;
        result
    }

    /// 安装本地字体文件。
    ///
    /// 文件会临时推送到设备，操作完成后会尽力清理。重复安装同一字体视为成功。
    pub async fn install_font(&self, local: impl AsRef<Path>) -> Result<()> {
        let local = local.as_ref();
        let remote = remote_font_path(local);
        let guard = RemoteFileGuard::new(self.inner.hdc.clone(), remote.clone());
        let result = async {
            self.inner.hdc.send_file(local, &remote).await?;
            match self
                .run_testhelper(
                    &format!("install-font {}", shell_quote(&remote)),
                    "安装字体",
                )
                .await
            {
                Ok(output) => {
                    let lower = output.to_ascii_lowercase();
                    if lower.contains("already installed")
                        || !has_failure_word(&lower)
                            && lower.contains("font installed successfully")
                    {
                        Ok(())
                    } else {
                        Err(unexpected_tool_output("安装字体", &output))
                    }
                }
                Err(DriverError::HdcCommand { message, .. })
                    if message.to_ascii_lowercase().contains("already installed") =>
                {
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        .await;
        guard.cleanup().await;
        result
    }

    /// 按字体名称卸载字体。
    pub async fn uninstall_font(&self, font_name: &str) -> Result<()> {
        let font_name = validated_text_argument(font_name, "字体名称")?;
        let output = self
            .run_testhelper(
                &format!("uninstall-font {}", shell_quote(font_name)),
                "卸载字体",
            )
            .await?;
        let lower = output.to_ascii_lowercase();
        if !has_failure_word(&lower) && lower.contains("font uninstalled successfully") {
            Ok(())
        } else {
            Err(unexpected_tool_output("卸载字体", &output))
        }
    }

    /// 启用设备端网络模拟工具。
    pub async fn enable_network_simulation(&self) -> Result<()> {
        let output = self.run_netcopilot("-e 1", "启用网络模拟").await?;
        require_network_success("启用网络模拟", &output)
    }

    /// 禁用设备端网络模拟工具。
    pub async fn disable_network_simulation(&self) -> Result<()> {
        let output = self.run_netcopilot("-e 0", "禁用网络模拟").await?;
        require_network_success("禁用网络模拟", &output)
    }

    /// 列出设备端可用的网络模拟场景。
    pub async fn network_scenarios(&self) -> Result<Vec<NetworkScenarioInfo>> {
        let output = self.run_netcopilot("-p", "查询网络模拟场景").await?;
        parse_network_scenarios(&output)
    }

    /// 启动已有网络模拟场景。
    pub async fn start_network_scenario(&self, scenario_id: u32) -> Result<()> {
        validate_scenario_id(scenario_id)?;
        self.enable_network_simulation().await?;
        let output = self
            .run_netcopilot(&format!("-s {scenario_id}"), "启动网络模拟场景")
            .await?;
        require_network_success("启动网络模拟场景", &output)
    }

    /// 启动官方内置网络模拟场景。
    pub async fn start_builtin_network_scenario(
        &self,
        scenario: BuiltinNetworkScenario,
    ) -> Result<()> {
        self.start_network_scenario(scenario.id()).await
    }

    /// 新建并启动自定义网络模拟场景，返回设备分配的场景 ID。
    ///
    /// 停止后如不再使用，应显式调用 [`delete_network_scenario`](Self::delete_network_scenario)
    /// 删除该自定义场景。
    pub async fn start_custom_network_scenario(&self, scenario: &NetworkScenario) -> Result<u32> {
        scenario.validate()?;
        self.enable_network_simulation().await?;
        let before: BTreeSet<_> = self
            .network_scenarios()
            .await?
            .into_iter()
            .map(|item| item.id)
            .collect();
        let payload = serde_json::to_string(&json!({
            "scenarioName": &scenario.name,
            "uplinkBandwidth": scenario.uplink_bandwidth,
            "downlinkBandwidth": scenario.downlink_bandwidth,
            "uplinkLatency": scenario.uplink_latency,
            "downlinkLatency": scenario.downlink_latency,
            "uplinkDropRate": scenario.uplink_drop_rate,
            "downlinkDropRate": scenario.downlink_drop_rate,
        }))?;
        let output = self
            .run_netcopilot(
                &format!("-a {}", shell_quote(&payload)),
                "创建自定义网络模拟场景",
            )
            .await?;
        require_network_success("创建自定义网络模拟场景", &output)?;

        let added: BTreeSet<_> = self
            .network_scenarios()
            .await?
            .into_iter()
            .map(|item| item.id)
            .filter(|id| !before.contains(id))
            .collect();
        if added.len() != 1 {
            return Err(DriverError::Protocol(format!(
                "创建自定义网络模拟场景后新增 ID 数量不是 1（实际为 {}）",
                added.len()
            )));
        }
        let scenario_id = *added.first().expect("已检查新增场景数量");
        if let Err(error) = self.start_network_scenario(scenario_id).await {
            let _ = self.delete_network_scenario(scenario_id).await;
            return Err(error);
        }
        Ok(scenario_id)
    }

    /// 停止指定网络模拟场景。
    pub async fn stop_network_scenario(&self, scenario_id: u32) -> Result<()> {
        validate_scenario_id(scenario_id)?;
        self.enable_network_simulation().await?;
        let output = self
            .run_netcopilot(&format!("-c {scenario_id}"), "停止网络模拟场景")
            .await?;
        require_network_success("停止网络模拟场景", &output)
    }

    /// 删除指定自定义网络模拟场景。
    pub async fn delete_network_scenario(&self, scenario_id: u32) -> Result<()> {
        validate_scenario_id(scenario_id)?;
        let output = self
            .run_netcopilot(&format!("-d {scenario_id}"), "删除网络模拟场景")
            .await?;
        require_network_success("删除网络模拟场景", &output)
    }

    pub(super) async fn ensure_testhelper(&self, capability: &str) -> Result<()> {
        self.ensure_device_command("testhelper", capability).await
    }

    async fn run_testhelper(&self, arguments: &str, capability: &str) -> Result<String> {
        self.ensure_testhelper(capability).await?;
        self.inner
            .hdc
            .shell(format!("testhelper {arguments}"))
            .await
            .map(|output| output.stdout)
    }

    async fn run_netcopilot(&self, arguments: &str, capability: &str) -> Result<String> {
        self.require_api_level(20, capability).await?;
        self.ensure_device_command("netcopilot", capability).await?;
        self.inner
            .hdc
            .shell(format!("netcopilot {arguments}"))
            .await
            .map(|output| output.stdout)
    }

    async fn ensure_device_command(&self, command: &str, capability: &str) -> Result<()> {
        let output = self
            .inner
            .hdc
            .shell(format!("command -v {command} || true"))
            .await?;
        if output.stdout.trim().is_empty() {
            Err(DriverError::Unsupported(format!(
                "设备未提供 {command}，无法{capability}"
            )))
        } else {
            Ok(())
        }
    }
}

fn validated_text_argument<'a>(value: &'a str, name: &str) -> Result<&'a str> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(DriverError::InvalidArgument(format!(
            "{name}不能为空或包含控制字符"
        )));
    }
    Ok(value.trim())
}

fn require_successfully(operation: &str, output: &str) -> Result<()> {
    let lower = output.to_ascii_lowercase();
    if !has_failure_word(&lower) && has_success_word(&lower) {
        Ok(())
    } else {
        Err(unexpected_tool_output(operation, output))
    }
}

fn require_network_success(operation: &str, output: &str) -> Result<()> {
    let lower = output.to_ascii_lowercase();
    if !has_failure_word(&lower) && has_success_word(&lower) {
        Ok(())
    } else {
        Err(unexpected_tool_output(operation, output))
    }
}

fn has_success_word(output: &str) -> bool {
    output
        .split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| matches!(word, "success" | "successfully"))
}

fn has_failure_word(output: &str) -> bool {
    output
        .split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| {
            matches!(
                word,
                "fail" | "failed" | "failure" | "error" | "unsuccessful" | "unsuccessfully"
            )
        })
}

fn unexpected_tool_output(operation: &str, output: &str) -> DriverError {
    let output = output.trim();
    let output = if output.is_empty() { "<empty>" } else { output };
    DriverError::Protocol(format!("{operation}返回无法识别的设备输出：{output}"))
}

fn prefixed_value<'a>(output: &'a str, prefix: &str) -> Option<&'a str> {
    output
        .trim_end_matches(&['\r', '\n'][..])
        .strip_prefix(prefix)
        .map(str::trim)
}

fn clipboard_value(output: &str) -> Option<&str> {
    let output = output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output);
    output
        .strip_prefix("Pasteboard text:")
        .map(|value| value.strip_prefix(' ').unwrap_or(value))
}

fn is_valid_system_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b' '
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 4 | 7 | 10 | 13 | 16) && !byte.is_ascii_digit())
    {
        return false;
    }
    let number = |start: usize, end: usize| {
        std::str::from_utf8(&bytes[start..end])
            .ok()
            .and_then(|part| part.parse::<u32>().ok())
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let leap = year % 400 == 0 || year % 4 == 0 && year % 100 != 0;
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=max_day).contains(&day)
}

fn remote_font_path(local: &Path) -> String {
    let extension = local
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 8
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    format!(
        "/data/local/tmp/hm_driver_font_{}{}",
        next_operation_id(),
        extension
    )
}

fn validate_scenario_id(scenario_id: u32) -> Result<()> {
    if scenario_id == 0 {
        Err(DriverError::InvalidArgument(
            "网络模拟场景 ID 必须大于 0".into(),
        ))
    } else {
        Ok(())
    }
}

fn parse_network_scenarios(output: &str) -> Result<Vec<NetworkScenarioInfo>> {
    let mut scenarios = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let digit_count = line.bytes().take_while(u8::is_ascii_digit).count();
        if digit_count == 0 {
            continue;
        }
        let id = line[..digit_count]
            .parse::<u32>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| unexpected_tool_output("查询网络模拟场景", output))?;
        let remainder = &line[digit_count..];
        if !remainder.starts_with(char::is_whitespace) && !remainder.starts_with('|') {
            return Err(unexpected_tool_output("查询网络模拟场景", output));
        }
        let name = remainder
            .trim_start()
            .strip_prefix('|')
            .unwrap_or(remainder)
            .trim();
        if name.is_empty() {
            return Err(unexpected_tool_output("查询网络模拟场景", output));
        } else {
            scenarios.push(NetworkScenarioInfo {
                id,
                name: name.to_owned(),
            });
        }
    }
    if scenarios.is_empty() && !output.trim().is_empty() {
        return Err(unexpected_tool_output("查询网络模拟场景", output));
    }
    Ok(scenarios)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_calendar_time_without_extra_dependency() {
        assert!(is_valid_system_time("2024-02-29 23:59:59"));
        assert!(is_valid_system_time("2026-10-05 12:34:56"));
        assert!(!is_valid_system_time("2023-02-29 12:00:00"));
        assert!(!is_valid_system_time("2026-13-01 00:00:00"));
        assert!(!is_valid_system_time("2026-10-05T12:34:56"));
    }

    #[test]
    fn parses_clipboard_and_prefixed_values() {
        assert_eq!(clipboard_value("Pasteboard text: hello\r\n"), Some("hello"));
        assert_eq!(clipboard_value("Pasteboard text:\r\n"), Some(""));
        assert_eq!(
            prefixed_value("Current timezone: Asia/Shanghai\n", "Current timezone:"),
            Some("Asia/Shanghai")
        );
    }

    #[test]
    fn parses_network_scenario_table() {
        assert_eq!(
            parse_network_scenarios("1 | Elevator\n6  Subway\nignored\n").unwrap(),
            vec![
                NetworkScenarioInfo {
                    id: 1,
                    name: "Elevator".into(),
                },
                NetworkScenarioInfo {
                    id: 6,
                    name: "Subway".into(),
                },
            ]
        );
    }

    #[test]
    fn recognizes_network_command_results() {
        assert!(require_network_success("test", "Success").is_ok());
        assert!(require_network_success("test", "operation successfully completed").is_ok());
        assert!(require_network_success("test", "Failed").is_err());
        assert!(require_network_success("test", "unknown").is_err());
        assert!(require_network_success("test", "unsuccessful").is_err());
        assert!(require_network_success("test", "Error: Success").is_err());
        assert!(require_successfully("test", "not successfully: failed").is_err());
    }

    #[test]
    fn generates_safe_remote_font_path() {
        let path = remote_font_path(Path::new("C:/fonts/a font.ttf"));
        assert!(path.starts_with("/data/local/tmp/hm_driver_font_"));
        assert!(path.ends_with(".ttf"));
        assert!(!path.contains(' '));
        assert_ne!(path, remote_font_path(Path::new("C:/fonts/a font.ttf")));
        assert!(!remote_font_path(Path::new("font.a'b")).contains('\''));
    }

    #[test]
    fn preserves_clipboard_spaces_and_embedded_newlines() {
        assert_eq!(
            clipboard_value("Pasteboard text:   首行\n末行  \n\r\n"),
            Some("  首行\n末行  \n")
        );
        assert!(clipboard_value("unknown").is_none());
        for value in ["", " \t", "Asia/Shanghai\n", "name\0", "name\t"] {
            assert!(validated_text_argument(value, "参数").is_err());
        }
        assert_eq!(
            validated_text_argument(" Asia/Shanghai ", "时区").unwrap(),
            "Asia/Shanghai"
        );
    }

    #[test]
    fn rejects_invalid_scenario_tables_and_ids() {
        assert!(parse_network_scenarios("").unwrap().is_empty());
        for output in [
            "unknown",
            "Failed",
            "0 | Invalid",
            "4294967296 | Overflow",
            "1000 |",
            "1000name",
        ] {
            assert!(parse_network_scenarios(output).is_err(), "{output}");
        }
        assert!(validate_scenario_id(0).is_err());
        assert!(validate_scenario_id(u32::MAX).is_ok());
    }

    struct Step {
        arguments: Vec<String>,
        result: Result<String>,
    }

    fn shell(command: &str, output: &str) -> Step {
        Step {
            arguments: vec!["shell".into(), command.into()],
            result: Ok(output.into()),
        }
    }

    fn helper(arguments: &str, output: &str) -> Vec<Step> {
        vec![
            shell("command -v testhelper || true", "/system/bin/testhelper\n"),
            shell(&format!("testhelper {arguments}"), output),
        ]
    }

    fn network(arguments: &str, output: &str) -> Vec<Step> {
        vec![
            shell("command -v netcopilot || true", "/system/bin/netcopilot\n"),
            shell(&format!("netcopilot {arguments}"), output),
        ]
    }

    async fn mock_driver(
        steps: Vec<Step>,
        api_level: Option<u32>,
    ) -> (
        HmDriver,
        std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Step>>>,
    ) {
        use crate::hdc::{CommandOutput, HdcRunner};
        use crate::rpc::{ApiDialect, RpcClient};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;
        let pending = Arc::new(Mutex::new(std::collections::VecDeque::from(steps)));
        let handler_pending = pending.clone();
        let font_path = Mutex::new(String::new());
        let hdc = HdcRunner::with_test_handler(move |arguments| {
            let mut arguments: Vec<_> = arguments
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect();
            let mut font_path = font_path.lock().unwrap();
            if arguments.first().is_some_and(|arg| arg == "file") {
                assert_eq!(arguments.len(), 4);
                assert!(arguments[3].starts_with("/data/local/tmp/hm_driver_font_"));
                assert!(arguments[3].ends_with(".ttf"));
                *font_path = arguments[3].clone();
            }
            if !font_path.is_empty() {
                for argument in &mut arguments {
                    *argument = argument.replace(font_path.as_str(), "<font>");
                }
            }
            let expected = handler_pending
                .lock()
                .unwrap()
                .pop_front()
                .expect("非预期 HDC 调用");
            assert_eq!(arguments, expected.arguments);
            expected.result.map(|stdout| CommandOutput {
                stdout,
                stderr: String::new(),
                status: 0,
            })
        });
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let rpc = RpcClient::connect(
            listener.local_addr().unwrap().port(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            4096,
        )
        .await
        .unwrap();
        let mut driver = HmDriver::with_test_rpc_api_level(rpc, ApiDialect::Modern, api_level);
        Arc::get_mut(&mut driver.inner).unwrap().hdc = hdc;
        (driver, pending)
    }

    #[tokio::test]
    async fn system_commands_use_official_arguments_and_parse_values() {
        let mut steps = helper("set-viewmode 'dark'", "View mode set successfully");
        steps.extend(helper("set-viewmode 'light'", "already set to light"));
        steps.extend(helper(
            "set-time '2026-10-07 12:34:56'",
            "Time set successfully",
        ));
        steps.extend(helper(
            "get-time",
            "Current system time: 2026-10-07 12:34:56\r\n",
        ));
        steps.extend(helper(
            "set-timezone 'Asia/Shanghai'",
            "Timezone set successfully",
        ));
        steps.extend(helper("get-timezone", "Current timezone: Asia/Shanghai\n"));
        steps.extend(helper(
            "set-pastedata 'a'\\''; echo unsafe\n正文  '",
            "Pasteboard set successfully",
        ));
        steps.extend(helper(
            "get-pastedata",
            "Pasteboard text: a'; echo unsafe\n正文  \n",
        ));
        steps.extend(helper("clear-pastedata", "Pasteboard cleared successfully"));
        steps.extend(helper("get-pastedata", "Pasteboard text:\n"));
        steps.extend(helper("hide-keyboard", "No active keyboard"));
        let (driver, pending) = mock_driver(steps, Some(26)).await;
        driver.set_view_mode(ViewMode::Dark).await.unwrap();
        driver.set_view_mode(ViewMode::Light).await.unwrap();
        driver.set_system_time("2026-10-07 12:34:56").await.unwrap();
        assert_eq!(driver.system_time().await.unwrap(), "2026-10-07 12:34:56");
        driver.set_timezone("Asia/Shanghai").await.unwrap();
        assert_eq!(driver.timezone().await.unwrap(), "Asia/Shanghai");
        driver
            .write_clipboard("a'; echo unsafe\n正文  ")
            .await
            .unwrap();
        assert_eq!(
            driver.read_clipboard().await.unwrap(),
            "a'; echo unsafe\n正文  "
        );
        driver.clear_clipboard().await.unwrap();
        assert_eq!(driver.read_clipboard().await.unwrap(), "");
        driver.hide_keyboard().await.unwrap();
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_arguments_before_running_hdc() {
        let (driver, pending) = mock_driver(Vec::new(), Some(26)).await;
        assert!(matches!(
            driver.set_system_time("2026-02-30 00:00:00").await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.set_timezone("Asia/Shanghai\n").await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.write_clipboard("a\0b").await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.uninstall_font("\t").await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.start_network_scenario(0).await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.stop_network_scenario(0).await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(matches!(
            driver.delete_network_scenario(0).await,
            Err(DriverError::InvalidArgument(_))
        ));
        let mut scenario = NetworkScenario::new("test", 0, 0, 0, 0, 0.0, 1.0).unwrap();
        scenario.uplink_drop_rate = f64::NAN;
        assert!(matches!(
            driver.start_custom_network_scenario(&scenario).await,
            Err(DriverError::InvalidArgument(_))
        ));
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_tools_and_old_api_return_unsupported_without_side_effects() {
        let (driver, pending) = mock_driver(
            vec![
                shell("command -v testhelper || true", ""),
                shell("command -v netcopilot || true", ""),
            ],
            Some(26),
        )
        .await;
        assert!(matches!(
            driver.clear_clipboard().await,
            Err(DriverError::Unsupported(_))
        ));
        assert!(matches!(
            driver.enable_network_simulation().await,
            Err(DriverError::Unsupported(_))
        ));
        assert!(pending.lock().unwrap().is_empty());
        let (driver, _) = mock_driver(Vec::new(), Some(19)).await;
        assert!(matches!(
            driver.enable_network_simulation().await,
            Err(DriverError::Unsupported(_))
        ));
        let (driver, pending) = mock_driver(network("-e 1", "Success"), None).await;
        driver.enable_network_simulation().await.unwrap();
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn command_failures_and_unrecognized_output_are_not_success() {
        let mut steps = helper("clear-pastedata", "unknown");
        steps.extend(helper("get-time", "Current system time: invalid"));
        steps.extend(helper("get-pastedata", "unrecognized format"));
        steps.extend(network("-p", "Failed to read scenarios"));
        steps.push(Step {
            arguments: vec!["shell".into(), "command -v testhelper || true".into()],
            result: Err(DriverError::HdcCommand {
                code: Some(1),
                message: "device offline".into(),
            }),
        });
        let (driver, pending) = mock_driver(steps, Some(26)).await;
        assert!(matches!(
            driver.clear_clipboard().await,
            Err(DriverError::Protocol(_))
        ));
        assert!(matches!(
            driver.system_time().await,
            Err(DriverError::Protocol(_))
        ));
        assert!(matches!(
            driver.read_clipboard().await,
            Err(DriverError::Protocol(_))
        ));
        assert!(matches!(
            driver.network_scenarios().await,
            Err(DriverError::Protocol(_))
        ));
        assert!(matches!(
            driver.clear_clipboard().await,
            Err(DriverError::HdcCommand { .. })
        ));
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn network_commands_create_start_stop_and_delete_explicitly() {
        let scenario =
            NetworkScenario::new("测试'; echo unsafe", 100_000, 500_000, 200, 300, 0.05, 0.01)
                .unwrap();
        let payload = json!({"scenarioName":scenario.name, "uplinkBandwidth":100_000, "downlinkBandwidth":500_000, "uplinkLatency":200, "downlinkLatency":300, "uplinkDropRate":0.05, "downlinkDropRate":0.01});
        let mut steps = network("-e 1", "Success");
        steps.extend(network("-s 6", "Success"));
        steps.extend(network("-e 1", "Success"));
        steps.extend(network("-p", "1 | Elevator\n6 | Subway\n"));
        steps.extend(network(
            &format!("-a {}", shell_quote(&payload.to_string())),
            "Success",
        ));
        steps.extend(network("-p", "1 | Elevator\n6 | Subway\n1000 | Custom\n"));
        steps.extend(network("-e 1", "Success"));
        steps.extend(network("-s 1000", "Success"));
        steps.extend(network("-e 1", "Success"));
        steps.extend(network("-c 1000", "Success"));
        steps.extend(network("-d 1000", "Success"));
        steps.extend(network("-e 0", "Success"));
        let (driver, pending) = mock_driver(steps, Some(26)).await;
        driver
            .start_builtin_network_scenario(BuiltinNetworkScenario::Subway)
            .await
            .unwrap();
        assert_eq!(
            driver
                .start_custom_network_scenario(&scenario)
                .await
                .unwrap(),
            1000
        );
        driver.stop_network_scenario(1000).await.unwrap();
        driver.delete_network_scenario(1000).await.unwrap();
        driver.disable_network_simulation().await.unwrap();
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn custom_network_start_failure_deletes_the_new_scenario() {
        let scenario = NetworkScenario::new("test", 0, 0, 0, 0, 0.0, 0.0).unwrap();
        let payload = json!({"scenarioName":"test", "uplinkBandwidth":0, "downlinkBandwidth":0, "uplinkLatency":0, "downlinkLatency":0, "uplinkDropRate":0.0, "downlinkDropRate":0.0});
        let mut steps = network("-e 1", "Success");
        steps.extend(network("-p", "1 | Elevator\n"));
        steps.extend(network(
            &format!("-a {}", shell_quote(&payload.to_string())),
            "Success",
        ));
        steps.extend(network("-p", "1 | Elevator\n1000 | test\n"));
        steps.extend(network("-e 1", "Success"));
        steps.extend(network("-s 1000", "Failed"));
        steps.extend(network("-d 1000", "Success"));
        let (driver, pending) = mock_driver(steps, Some(26)).await;
        assert!(matches!(
            driver.start_custom_network_scenario(&scenario).await,
            Err(DriverError::Protocol(_))
        ));
        assert!(pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn font_commands_clean_temporary_files_on_success_and_failure() {
        let mut steps = Vec::new();
        for (command, output) in [
            ("get-fontname", "Font name: Example\n"),
            ("install-font", "Error: Font already installed"),
            ("get-fontname", "unknown"),
        ] {
            steps.push(Step {
                arguments: vec![
                    "file".into(),
                    "send".into(),
                    "font.ttf".into(),
                    "<font>".into(),
                ],
                result: Ok("File transferred".into()),
            });
            let mut commands = helper(&format!("{command} '<font>'"), output);
            if command == "install-font" {
                commands[1].result = Err(DriverError::HdcCommand {
                    code: Some(1),
                    message: output.into(),
                });
            }
            steps.extend(commands);
            steps.push(shell("rm -f <font>", ""));
        }
        steps.extend(helper(
            "uninstall-font 'Example'",
            "Font uninstalled successfully",
        ));
        let (driver, pending) = mock_driver(steps, Some(26)).await;
        assert_eq!(driver.font_name("font.ttf").await.unwrap(), "Example");
        driver.install_font("font.ttf").await.unwrap();
        assert!(matches!(
            driver.font_name("font.ttf").await,
            Err(DriverError::Protocol(_))
        ));
        driver.uninstall_font("Example").await.unwrap();
        assert!(pending.lock().unwrap().is_empty());
    }
}
