use crate::agent::AgentProfile;
use crate::{DriverError, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct AgentCatalog {
    pub source_package: String,
    pub source_wheel: String,
    pub agents: Vec<AgentProfile>,
}

impl AgentCatalog {
    pub fn load() -> Result<Self> {
        let catalog: Self = serde_json::from_str(include_str!("../assets/agents.json"))?;
        if catalog.source_package != "devecotesting-hypium-6.1.0.210.zip"
            || catalog.source_wheel != "xdevice_devicetest-6.1.0.210-py3-none-any.whl"
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
        assert_eq!(catalog.source_package, "devecotesting-hypium-6.1.0.210.zip");
        assert_eq!(
            catalog.source_wheel,
            "xdevice_devicetest-6.1.0.210-py3-none-any.whl"
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
                "uitest_agent_v1.1.10.so",
                600_246,
                "1c9286456fb003ee15d86fef04e8c93f004027349e6dd2ef972c792e0a6d4bf8",
            ),
            (
                "uitest_agent_v1.1.9.x86_64_so",
                1_460_181,
                "24a14a7841ec376dad4e1fa741de8f9f218c652a7e6d0100077798006367b274",
            ),
            (
                "uitest_agent_v1.2.2.so",
                600_245,
                "e1b8e75fad983aa29640784ee4b457fe7ac1c916a15f72c8b899d5a7716da651",
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
