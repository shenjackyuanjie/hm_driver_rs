use crate::agent::AgentProfile;
use crate::{DriverError, Result};
use serde::Deserialize;

/// 随 crate 随带的官方 Agent 清单，可在不连接设备的情况下查阅已验证的 Agent 信息。
#[derive(Debug, Deserialize)]
pub struct AgentCatalog {
    /// 官方来源包文件名。
    pub source_package: String,
    /// 官方来源 wheel 文件名。
    pub source_wheel: String,
    /// 所有官方 Agent 的配置列表。
    pub agents: Vec<AgentProfile>,
}

impl AgentCatalog {
    /// 从编译期嵌入的 JSON 清单中加载官方 Agent catalog。
    ///
    /// 同时验证 source_package、source_wheel 以及 Agent 数量是否与预期一致。
    pub fn load() -> Result<Self> {
        let catalog: Self = serde_json::from_str(include_str!("../assets/agents.json"))?;
        if catalog.source_package != "devecotesting-hypium-26.0.0.400.zip"
            || catalog.source_wheel != "xdevice_devicetest-26.0.0.400-py3-none-any.whl"
        {
            return Err(DriverError::InvalidAgentCatalog(
                "官方包或 wheel 来源字段不匹配".into(),
            ));
        }
        if catalog.agents.len() != 5 {
            return Err(DriverError::InvalidAgentCatalog(
                "官方 Agent 数量必须为 5".into(),
            ));
        }
        Ok(catalog)
    }

    /// 根据版本号与架构在 catalog 中查找对应的 Agent 信息。
    ///
    /// 若未找到匹配项则返回 `DriverError::InvalidAgentCatalog`。
    pub fn profile(&self, version: &str, architecture: &str) -> Result<AgentProfile> {
        self.agents
            .iter()
            .find(|profile| profile.version == version && profile.architecture == architecture)
            .cloned()
            .ok_or_else(|| {
                DriverError::InvalidAgentCatalog(format!(
                    "缺少版本 {version}、架构 {architecture} 的 Agent"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_pins_official_files() {
        let catalog = AgentCatalog::load().unwrap();
        assert_eq!(
            catalog.source_package,
            "devecotesting-hypium-26.0.0.400.zip"
        );
        assert_eq!(
            catalog.source_wheel,
            "xdevice_devicetest-26.0.0.400-py3-none-any.whl"
        );
        let pinned = [
            (
                "uitest_agent_v1.1.3.so",
                149_685,
                "6a76d6047b367b0e00be627daf212d3baa5b20566131cbe298abe7cdf6639b53",
            ),
            (
                "uitest_agent_v1.1.5.so",
                153_781,
                "fc2322feb8145ddda244f2b97046f448d040d886e6f81e546842ee45fa028781",
            ),
            (
                "uitest_agent_v1.1.12.so",
                600_247,
                "ae00607958c2e73624319b3066dc427111b2b8224fb292909c8563f686a6ab2c",
            ),
            (
                "uitest_agent_v1.1.12.x86_64_so",
                1_465_286,
                "b7aee90fa38ea2ee79b2b91a2add750e27a3e0bb7c3e0dcea0e837a99c9d7563",
            ),
            (
                "uitest_agent_v1.2.3.so",
                600_246,
                "5dfbaca7e274a85bece8f225148ceb759b94d7319b956b9a7ea40d37944e4a9b",
            ),
        ];
        for (name, size, sha256) in pinned {
            let profile = catalog
                .agents
                .iter()
                .find(|item| item.file_name == name)
                .unwrap();
            assert_eq!(profile.size, size);
            assert_eq!(profile.sha256, sha256);
            assert!(profile.path.starts_with("devicetest/res/prototype/native/"));
        }
    }
}
