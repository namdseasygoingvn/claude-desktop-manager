use serde::{Deserialize, Serialize};

pub const REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub profiles: Vec<Profile>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            profiles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatus {
    pub profile: Profile,
    pub running_pid: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptCandidate {
    pub dir_name: String,
    pub suggested_name: String,
}

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum CdmError {
    #[error("Claude Desktop was not found. Set CDM_CLAUDE_BINARY to its executable.")]
    BinaryNotFound,
    #[error("no profile with id {0}")]
    ProfileNotFound(String),
    #[error("no group with id {0}")]
    GroupNotFound(String),
    #[error("profile {0} is running")]
    ProfileRunning(String),
    #[error("a profile name cannot be empty")]
    NameEmpty,
    #[error("the folder {0} already exists")]
    DirExists(String),
    #[error("registry.json is unusable: {0}")]
    RegistryCorrupt(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for CdmError {
    fn from(err: std::io::Error) -> Self {
        CdmError::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CdmError>;
