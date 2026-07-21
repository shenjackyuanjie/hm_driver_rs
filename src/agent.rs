use crate::catalog::AgentCatalog;
use crate::{DriverError, Result};
#[cfg(feature = "embedded-agents")]
use directories::ProjectDirs;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs;

/// Agent 与主机通信时使用的 HDC 转发类型。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarmonyTransport {
    Tcp { remote_port: u16 },
    LocalAbstract { socket_name: String },
}

/// Agent 分支的验证状态。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    LocallyVerified,
    OfficialReferenceOnly,
}

/// 官方 Agent catalog 中的一条记录。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AgentProfile {
    pub path: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub architecture: String,
    pub version: String,
    pub transport: HarmonyTransport,
    pub condition: String,
    pub compatibility: CompatibilityStatus,
}

/// Agent 二进制的来源。
#[derive(Clone, Debug, Default)]
pub enum AgentSource {
    /// 使用编译进 crate 的官方 Agent。
    #[default]
    Embedded,
    /// 从指定目录读取与 catalog 同名的官方 Agent。
    Directory(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UitestVersion([u32; 4]);

impl FromStr for UitestVersion {
    type Err = DriverError;

    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 4
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()))
        {
            return Err(DriverError::InvalidUitestVersion);
        }
        let mut parsed = [0; 4];
        for (index, part) in parts.into_iter().enumerate() {
            parsed[index] = part
                .parse()
                .map_err(|_| DriverError::InvalidUitestVersion)?;
        }
        Ok(Self(parsed))
    }
}

/// 根据设备架构与 uitest 版本预测将使用的 Agent，无需实际连接设备。
pub struct AgentResolver {
    catalog: AgentCatalog,
}

impl AgentResolver {
    pub fn new() -> Result<Self> {
        Ok(Self {
            catalog: AgentCatalog::load()?,
        })
    }

    pub fn resolve(&self, architecture: &str, version: &str) -> Result<AgentProfile> {
        let architecture = normalize_architecture(architecture)?;
        let version = version.parse::<UitestVersion>()?;
        if architecture == "x86_64" {
            return self.catalog.profile("1.1.9", "x86_64");
        }
        let agent_version = if version > UitestVersion([6, 0, 2, 1]) {
            "1.2.2"
        } else if version > UitestVersion([5, 1, 1, 3]) {
            "1.1.10"
        } else if version > UitestVersion([5, 1, 1, 2]) {
            "1.1.5"
        } else {
            "1.1.3"
        };
        self.catalog.profile(agent_version, "arm64")
    }
}

fn normalize_architecture(value: &str) -> Result<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    if value.contains("x86_64") {
        Ok("x86_64")
    } else if value.contains("arm64") || value.contains("aarch64") || value.contains("armeabi") {
        Ok("arm64")
    } else {
        Err(DriverError::UnsupportedArchitecture(value))
    }
}

pub(crate) async fn materialize_agent(
    source: &AgentSource,
    profile: &AgentProfile,
) -> Result<PathBuf> {
    match source {
        AgentSource::Directory(directory) => {
            let path = directory.join(&profile.file_name);
            verify_file(&path, profile).await?;
            Ok(path)
        }
        AgentSource::Embedded => materialize_embedded(profile).await,
    }
}

async fn materialize_embedded(profile: &AgentProfile) -> Result<PathBuf> {
    #[cfg(not(feature = "embedded-agents"))]
    {
        let _ = profile;
        Err(DriverError::Unsupported(
            "编译时未启用 embedded-agents feature".into(),
        ))
    }
    #[cfg(feature = "embedded-agents")]
    {
        let bytes = embedded_bytes(&profile.file_name).ok_or_else(|| {
            DriverError::InvalidAgentCatalog(format!("未知的内嵌 Agent：{}", profile.file_name))
        })?;
        verify_bytes(bytes, profile)?;
        let project_dirs = ProjectDirs::from("dev", "hm-driver-rs", "hm_driver_rs")
            .ok_or_else(|| DriverError::AgentVerification("无法解析本机缓存目录".into()))?;
        let directory = project_dirs
            .cache_dir()
            .join("agents")
            .join(&profile.sha256);
        fs::create_dir_all(&directory).await?;
        let destination = directory.join(&profile.file_name);
        if verify_file(&destination, profile).await.is_ok() {
            return Ok(destination);
        }
        let temporary = directory.join(format!(".{}.tmp", profile.file_name));
        fs::write(&temporary, bytes).await?;
        verify_file(&temporary, profile).await?;
        if fs::rename(&temporary, &destination).await.is_err() {
            // Windows 不允许 rename 覆盖已有文件；仅移除本 crate 的私有缓存目标。
            if fs::try_exists(&destination).await.unwrap_or(false) {
                fs::remove_file(&destination).await?;
            }
            fs::rename(&temporary, &destination).await?;
        }
        verify_file(&destination, profile).await?;
        Ok(destination)
    }
}

async fn verify_file(path: &Path, profile: &AgentProfile) -> Result<()> {
    if !fs::try_exists(path).await.unwrap_or(false) {
        return Err(DriverError::AgentNotFound(path.to_owned()));
    }
    let bytes = fs::read(path).await?;
    verify_bytes(&bytes, profile)
}

fn verify_bytes(bytes: &[u8], profile: &AgentProfile) -> Result<()> {
    if bytes.len() as u64 != profile.size {
        return Err(DriverError::AgentVerification(
            "Agent 文件大小不匹配".into(),
        ));
    }
    let digest = Sha256::digest(bytes);
    let mut actual = String::with_capacity(64);
    for byte in digest {
        let _ = write!(actual, "{byte:02x}");
    }
    if actual != profile.sha256 {
        return Err(DriverError::AgentVerification(
            "Agent SHA-256 不匹配".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "embedded-agents")]
fn embedded_bytes(file_name: &str) -> Option<&'static [u8]> {
    match file_name {
        "uitest_agent_v1.1.3.so" => Some(include_bytes!("../assets/agents/uitest_agent_v1.1.3.so")),
        "uitest_agent_v1.1.5.so" => Some(include_bytes!("../assets/agents/uitest_agent_v1.1.5.so")),
        "uitest_agent_v1.1.10.so" => {
            Some(include_bytes!("../assets/agents/uitest_agent_v1.1.10.so"))
        }
        "uitest_agent_v1.1.9.x86_64_so" => Some(include_bytes!(
            "../assets/agents/uitest_agent_v1.1.9.x86_64_so"
        )),
        "uitest_agent_v1.2.2.so" => Some(include_bytes!("../assets/agents/uitest_agent_v1.2.2.so")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_preserves_strict_boundaries() {
        let resolver = AgentResolver::new().unwrap();
        assert_eq!(
            resolver.resolve("arm64-v8a", "6.0.2.2").unwrap().version,
            "1.2.2"
        );
        assert_eq!(
            resolver.resolve("arm64-v8a", "6.0.2.1").unwrap().version,
            "1.1.10"
        );
        assert_eq!(
            resolver.resolve("arm64-v8a", "5.1.1.4").unwrap().version,
            "1.1.10"
        );
        assert_eq!(
            resolver.resolve("arm64-v8a", "5.1.1.3").unwrap().version,
            "1.1.5"
        );
        assert_eq!(
            resolver.resolve("arm64-v8a", "5.1.1.2").unwrap().version,
            "1.1.3"
        );
    }

    #[test]
    fn x86_64_has_priority_and_invalid_versions_fail() {
        let resolver = AgentResolver::new().unwrap();
        assert_eq!(
            resolver.resolve("x86_64", "9.9.9.9").unwrap().version,
            "1.1.9"
        );
        assert!(matches!(
            resolver.resolve("arm64", "6.0.2"),
            Err(DriverError::InvalidUitestVersion)
        ));
        assert!(matches!(
            resolver.resolve("mips", "6.0.2.2"),
            Err(DriverError::UnsupportedArchitecture(_))
        ));
    }

    #[tokio::test]
    #[cfg(feature = "embedded-agents")]
    async fn embedded_agents_match_catalog() {
        let catalog = AgentCatalog::load().unwrap();
        for profile in catalog.agents {
            verify_bytes(embedded_bytes(&profile.file_name).unwrap(), &profile).unwrap();
        }
    }
}
