//! 应用安装、启停与信息查询。

use super::HmDriver;
use crate::types::{AbilityInfo, AppIdentifier, OpenUrlMode, validate_ability};
use crate::{DriverError, Result};
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use tracing::{debug, info, trace};
use url::Url;

impl HmDriver {
    /// 安装应用（通过 HDC 发送 APK/HAP 到设备并安装）。
    pub async fn install_app(&self, package: impl AsRef<Path>) -> Result<()> {
        debug!(target: "hm_driver_rs::app", package = %package.as_ref().display(), "安装应用");
        self.inner.hdc.install(package.as_ref()).await.map(|_| ())
    }

    /// 卸载指定包名的应用。
    pub async fn uninstall_app(&self, bundle: &AppIdentifier) -> Result<()> {
        debug!(target: "hm_driver_rs::app", bundle = %bundle.as_str(), "卸载应用");
        self.inner.hdc.uninstall(bundle.as_str()).await.map(|_| ())
    }

    /// 启动应用。
    ///
    /// 如果不指定 ability，会自动查找应用的 main ability。
    pub async fn start_app(&self, bundle: &AppIdentifier, ability: Option<&str>) -> Result<()> {
        info!(target: "hm_driver_rs::app", bundle = %bundle.as_str(), ability = ?ability, "启动应用");
        let ability = match ability {
            Some(value) => {
                validate_ability(value)?;
                value.to_owned()
            }
            None => self
                .main_ability(bundle)
                .await?
                .ok_or_else(|| DriverError::Protocol("应用没有 main ability".into()))?,
        };
        self.inner
            .hdc
            .shell(format!("aa start -a {ability} -b {}", bundle.as_str()))
            .await
            .map(|_| ())
    }

    /// 使用系统浏览器或默认方式打开 URL。
    pub async fn open_url(&self, value: &str, mode: OpenUrlMode) -> Result<()> {
        debug!(target: "hm_driver_rs::app", url = %value, ?mode, "打开 URL");
        let url = Url::parse(value).map_err(|error| DriverError::InvalidUrl(error.to_string()))?;
        if url.scheme().is_empty() {
            return Err(DriverError::InvalidUrl("URL 缺少 scheme".into()));
        }
        let url = shell_quote(url.as_str());
        let command = match mode {
            OpenUrlMode::SystemBrowser => {
                format!("aa start -A ohos.want.action.viewData -e entity.system.browsable -U {url}")
            }
            OpenUrlMode::Default => format!("aa start -U {url}"),
        };
        self.inner.hdc.shell(command).await.map(|_| ())
    }

    /// 强制停止指定应用的进程。
    pub async fn stop_app(&self, bundle: &AppIdentifier) -> Result<()> {
        debug!(target: "hm_driver_rs::app", bundle = %bundle.as_str(), "停止应用");
        self.inner
            .hdc
            .shell(format!("aa force-stop {}", bundle.as_str()))
            .await
            .map(|_| ())
    }

    /// 清除指定应用的用户缓存和数据。
    pub async fn clear_app(&self, bundle: &AppIdentifier) -> Result<()> {
        debug!(target: "hm_driver_rs::app", bundle = %bundle.as_str(), "清除应用数据");
        self.inner
            .hdc
            .shell(format!("bm clean -n {} -c", bundle.as_str()))
            .await?;
        self.inner
            .hdc
            .shell(format!("bm clean -n {} -d", bundle.as_str()))
            .await
            .map(|_| ())
    }

    /// 查询应用的详细信息，返回 `bm dump` 的 JSON 输出。
    pub async fn app_info(&self, bundle: &AppIdentifier) -> Result<Value> {
        let output = self
            .inner
            .hdc
            .shell(format!("bm dump -n {}", bundle.as_str()))
            .await?;
        let Some(start) = output.stdout.find('{') else {
            return Err(DriverError::Protocol("应用信息不包含 JSON 对象".into()));
        };
        let Some(end) = output.stdout.rfind('}') else {
            return Err(DriverError::Protocol("应用信息 JSON 不完整".into()));
        };
        serde_json::from_str(&output.stdout[start..=end]).map_err(DriverError::Json)
    }

    /// 解析应用的 Ability 列表。
    pub async fn app_abilities(&self, bundle: &AppIdentifier) -> Result<Vec<AbilityInfo>> {
        Ok(parse_ability_infos(&self.app_info(bundle).await?))
    }

    /// 查询应用的 main ability 详情。
    pub async fn main_ability_info(&self, bundle: &AppIdentifier) -> Result<Option<AbilityInfo>> {
        let value = self.app_info(bundle).await?;
        Ok(select_main_ability(parse_ability_infos(&value)))
    }

    /// 查询应用的 main ability 名称。
    pub async fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        let value = self.app_info(bundle).await?;
        let abilities = parse_ability_infos(&value);
        Ok(select_main_ability(abilities)
            .map(|ability| ability.name)
            .or_else(|| find_string_key(&value, "mainAbility")))
    }

    /// 获取当前前台应用。
    pub async fn current_app(&self) -> Result<Option<(AppIdentifier, String)>> {
        trace!(target: "hm_driver_rs::app", "获取当前前台应用");
        let output = self.inner.hdc.shell("aa dump -l").await?;
        let bundle_re = Regex::new(r"bundle name \[([A-Za-z0-9_.]+)\]")
            .map_err(|error| DriverError::Protocol(error.to_string()))?;
        let ability_re = Regex::new(r"main name \[([A-Za-z0-9_.]+)\]")
            .map_err(|error| DriverError::Protocol(error.to_string()))?;
        for block in output.stdout.split("Mission ID #") {
            if !block.contains("state #FOREGROUND") {
                continue;
            }
            let bundle = bundle_re.captures(block).and_then(|capture| capture.get(1));
            let ability = ability_re
                .captures(block)
                .and_then(|capture| capture.get(1));
            if let (Some(bundle), Some(ability)) = (bundle, ability) {
                return Ok(Some((
                    AppIdentifier::new(bundle.as_str())?,
                    ability.as_str().to_owned(),
                )));
            }
        }
        Ok(None)
    }
}

/// 单引号包裹 shell 参数，并对参数内的单引号做转义。
pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 从 `bm dump` 的 JSON 值中递归提取所有 Ability 信息。
pub(super) fn parse_ability_infos(value: &Value) -> Vec<AbilityInfo> {
    let mut result = Vec::new();
    collect_ability_infos(value, None, &mut result);
    result
}

/// 从 ability 列表中按优先级（launcher > mainEntry > module mainAbility）筛选最佳项。
pub(super) fn select_main_ability(mut abilities: Vec<AbilityInfo>) -> Option<AbilityInfo> {
    abilities.sort_by_key(|ability| {
        let mut score = 0_u8;
        if ability.module_main_ability.as_deref() == Some(ability.name.as_str()) {
            score += 1;
        }
        if ability.main_module.as_deref() == Some(ability.module_name.as_str()) {
            score += 1;
        }
        (
            std::cmp::Reverse(ability.is_launcher),
            std::cmp::Reverse(score),
        )
    });
    abilities.into_iter().next()
}

fn collect_ability_infos(
    value: &Value,
    inherited_main_module: Option<&str>,
    result: &mut Vec<AbilityInfo>,
) {
    let Value::Object(object) = value else {
        if let Value::Array(values) = value {
            for value in values {
                collect_ability_infos(value, inherited_main_module, result);
            }
        }
        return;
    };
    let main_module = object
        .get("mainEntry")
        .and_then(Value::as_str)
        .or(inherited_main_module);
    if let Some(modules) = object.get("hapModuleInfos").and_then(Value::as_array) {
        collect_modules(modules, main_module, result);
    }
    for (key, child) in object {
        if key != "hapModuleInfos" {
            collect_ability_infos(child, main_module, result);
        }
    }
}

fn collect_modules(modules: &[Value], main_module: Option<&str>, result: &mut Vec<AbilityInfo>) {
    for module in modules {
        let module_main_ability = module
            .get("mainAbility")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let module_name = module
            .get("moduleName")
            .or_else(|| module.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(abilities) = module.get("abilityInfos").and_then(Value::as_array) else {
            continue;
        };
        for raw in abilities {
            let Some(name) = raw.get("name").and_then(Value::as_str) else {
                continue;
            };
            let ability_module = raw
                .get("moduleName")
                .and_then(Value::as_str)
                .unwrap_or(module_name);
            result.push(AbilityInfo {
                name: name.to_owned(),
                module_name: ability_module.to_owned(),
                module_main_ability: module_main_ability.clone(),
                main_module: main_module.map(str::to_owned),
                is_launcher: is_launcher_ability(raw),
                raw: raw.clone(),
            });
        }
    }
}

fn is_launcher_ability(value: &Value) -> bool {
    value
        .get("skills")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|skill| skill.get("actions").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .any(|action| action == "action.system.home")
}

fn find_string_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key).and_then(Value::as_str) {
                return Some(value.to_owned());
            }
            object
                .values()
                .find_map(|value| find_string_key(value, key))
        }
        Value::Array(values) => values.iter().find_map(|value| find_string_key(value, key)),
        _ => None,
    }
}
