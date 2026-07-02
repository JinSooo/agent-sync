use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AgentSyncError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("validation failed: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, AgentSyncError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
}

impl PlatformInfo {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SafetyClass {
    SafeConfig,
    McpConfig,
    MemoryKnowledge,
    SecretBearing,
    RawSession,
    Database,
    Executable,
    BinaryOrCache,
    Unknown,
}

impl SafetyClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SafeConfig => "safe_config",
            Self::McpConfig => "mcp_config",
            Self::MemoryKnowledge => "memory_knowledge",
            Self::SecretBearing => "secret_bearing",
            Self::RawSession => "raw_session",
            Self::Database => "database",
            Self::Executable => "executable",
            Self::BinaryOrCache => "binary_or_cache",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    LowMedium,
    Medium,
    MediumHigh,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LowMedium => "low-medium",
            Self::Medium => "medium",
            Self::MediumHigh => "medium-high",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Classification {
    pub safety_class: SafetyClass,
    pub risk: RiskLevel,
    pub reason: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Directory,
    Symlink,
    File,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub portable_path: String,
    pub kind: FileKind,
    pub depth: usize,
    pub size: Option<u64>,
    pub mtime: Option<DateTime<Utc>>,
    pub safety_class: SafetyClass,
    pub risk: RiskLevel,
    pub reason: String,
    pub recommendation: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootRecord {
    pub path: String,
    pub scope: String,
    pub exists: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub id: String,
    pub name: String,
    pub detected: bool,
    #[serde(default)]
    pub capabilities: AdapterCapabilities,
    pub roots: Vec<RootRecord>,
    pub findings: Vec<Finding>,
    pub sessions: Vec<SessionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AdapterCapabilities {
    pub can_export_config: bool,
    pub can_import_config: bool,
    pub can_export_memory: bool,
    pub can_import_memory: bool,
    pub can_list_sessions: bool,
    pub can_export_sessions: bool,
    pub can_import_sessions: bool,
    pub can_remap_session_project: bool,
    pub requires_app_stopped_for_session_apply: bool,
    pub supports_transactional_apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SnapshotSummary {
    pub agents_detected: usize,
    pub findings: usize,
    pub by_safety_class: BTreeMap<String, usize>,
    pub by_risk: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub schema_version: String,
    pub id: Uuid,
    pub generated_at: DateTime<Utc>,
    pub platform: PlatformInfo,
    pub inputs: SnapshotInputs,
    pub summary: SnapshotSummary,
    pub projects: Vec<ProjectIdentity>,
    pub agents: Vec<AgentSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotInputs {
    pub home: String,
    pub project: String,
    pub max_depth: usize,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIdentity {
    pub id: Uuid,
    pub canonical_path: String,
    pub physical_path: Option<String>,
    pub git_remote: Option<String>,
    pub git_root_fingerprint: Option<String>,
    pub package_name: Option<String>,
    pub agent_project_keys: Vec<AgentProjectKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProjectKey {
    pub agent_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub agent_id: String,
    pub title: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub source_project: Option<Uuid>,
    pub storage_refs: Vec<StorageRef>,
    pub visibility: SessionVisibility,
    pub content_policy: ContentPolicy,
    pub import_capabilities: SessionImportCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageRef {
    pub kind: String,
    pub portable_path: String,
    pub physical_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionVisibility {
    Visible,
    Archived,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentPolicy {
    MetadataOnly,
    ExplicitRawPayloadRequired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionImportCapabilities {
    pub import_as_archive: bool,
    pub import_as_new_session: bool,
    pub identity_rewrite: bool,
    pub requires_app_stopped: bool,
}

pub fn portable_path(path: &Path, home: &Path, project: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(project) {
        return join_portable("<project>", stripped);
    }
    if let Ok(stripped) = path.strip_prefix(home) {
        return join_portable("~", stripped);
    }
    normalize_path(path)
}

pub fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn join_portable(prefix: &str, tail: &Path) -> String {
    let tail = normalize_path(tail);
    if tail.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}/{tail}")
    }
}

pub fn classify_path(path: &Path, kind: &FileKind, agent_id: &str) -> Classification {
    let rel = normalize_path(path);
    let lower = rel.to_lowercase();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let ext = PathBuf::from(&file_name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}").to_lowercase())
        .unwrap_or_default();

    if contains_any_segment(
        &lower,
        &[
            "auth",
            "token",
            "tokens",
            "secret",
            "secrets",
            "credential",
            "credentials",
            "keychain",
            "apikey",
            "api_key",
            "oauth",
            "cookie",
            "cookies",
            "session_token",
        ],
    ) || lower.contains("/.env")
        || lower.ends_with("/.env")
        || file_name == ".env"
        || file_name.starts_with(".env.")
    {
        return classification(
            SafetyClass::SecretBearing,
            RiskLevel::Critical,
            "path suggests credentials, tokens, cookies, OAuth state, or env secrets",
            "detect only; never print contents or copy by default",
        );
    }

    if contains_any_segment(
        &lower,
        &[
            "session",
            "sessions",
            "conversation",
            "conversations",
            "chat-history",
            "chat_history",
            "workspacestorage",
            "workspace-storage",
            "history",
        ],
    ) {
        return classification(
            SafetyClass::RawSession,
            RiskLevel::High,
            "path suggests raw chat, session, history, or workspace state",
            "inventory only; migrate only through explicit offline adapter with backups",
        );
    }

    if matches!(
        ext.as_str(),
        ".sqlite" | ".sqlite3" | ".db" | ".ldb" | ".log"
    ) || lower.contains("/leveldb/")
        || lower.contains("/indexeddb/")
    {
        return classification(
            SafetyClass::Database,
            RiskLevel::High,
            "database-like storage can corrupt or mismatch across app versions and paths",
            "do not live-sync; use offline export/import only",
        );
    }

    if contains_any_segment(&lower, &["mcp", "mcpservers", "mcp_servers", "mcp_config"])
        || lower.ends_with(".mcp.json")
    {
        return classification(
            SafetyClass::McpConfig,
            RiskLevel::High,
            "MCP config can expose tools, commands, paths, and env references",
            "diff structure and require per-tool approval on target machine",
        );
    }

    if contains_any_segment(&lower, &["hook", "hooks", "script", "scripts", "bin"])
        || matches!(
            ext.as_str(),
            ".sh"
                | ".bash"
                | ".zsh"
                | ".fish"
                | ".ps1"
                | ".bat"
                | ".cmd"
                | ".js"
                | ".mjs"
                | ".cjs"
                | ".ts"
                | ".py"
                | ".rb"
                | ".pl"
        )
    {
        return classification(
            SafetyClass::Executable,
            RiskLevel::High,
            "file may run commands as part of hooks, scripts, skills, or plugins",
            "show path and hash in future; install from trusted source rather than blind copy",
        );
    }

    if contains_any_segment(
        &lower,
        &[
            "cache",
            "cached",
            "tmp",
            "temp",
            "node_modules",
            "dist",
            "build",
            "out",
            "log",
            "logs",
            "lockfile",
            "blob_storage",
            "gpucache",
            "code cache",
        ],
    ) || matches!(
        ext.as_str(),
        ".zip"
            | ".tar"
            | ".gz"
            | ".tgz"
            | ".rar"
            | ".7z"
            | ".bin"
            | ".exe"
            | ".dll"
            | ".dylib"
            | ".so"
            | ".wasm"
            | ".node"
            | ".png"
            | ".jpg"
            | ".jpeg"
            | ".gif"
            | ".webp"
            | ".ico"
            | ".pdf"
    ) {
        return classification(
            SafetyClass::BinaryOrCache,
            RiskLevel::MediumHigh,
            "cache, binary, plugin artifact, or generated file",
            "exclude from migration recipes unless explicitly supported",
        );
    }

    if contains_any_segment(
        &lower,
        &[
            "memory",
            "memories",
            "rules",
            "instructions",
            "agents",
            "skills",
            "prompts",
            "knowledge",
            "claude-mem",
            "memo",
        ],
    ) {
        let risk = if agent_id == "github-copilot" {
            RiskLevel::Medium
        } else {
            RiskLevel::MediumHigh
        };
        return classification(
            SafetyClass::MemoryKnowledge,
            risk,
            "path suggests rules, instructions, skills, prompts, agents, or durable memory",
            "diff carefully; preserve project/team source-of-truth boundaries",
        );
    }

    if matches!(
        ext.as_str(),
        ".md"
            | ".mdx"
            | ".txt"
            | ".json"
            | ".jsonc"
            | ".toml"
            | ".yaml"
            | ".yml"
            | ".ini"
            | ".conf"
            | ".cfg"
    ) || *kind == FileKind::Directory
    {
        return classification(
            SafetyClass::SafeConfig,
            RiskLevel::LowMedium,
            "text config or directory metadata suitable for read-only inventory",
            "safe to include in manifest; review before applying changes",
        );
    }

    classification(
        SafetyClass::Unknown,
        RiskLevel::Medium,
        "unrecognized state surface",
        "manual review before migration",
    )
}

fn classification(
    safety_class: SafetyClass,
    risk: RiskLevel,
    reason: &str,
    recommendation: &str,
) -> Classification {
    Classification {
        safety_class,
        risk,
        reason: reason.to_string(),
        recommendation: recommendation.to_string(),
    }
}

fn contains_any_segment(path: &str, needles: &[&str]) -> bool {
    let segments = path.split(['/', '.', '_', '-']);
    segments
        .into_iter()
        .any(|segment| needles.iter().any(|needle| segment == *needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_secret_before_session() {
        let c = classify_path(
            Path::new("/tmp/.codex/session_token.json"),
            &FileKind::File,
            "codex",
        );
        assert_eq!(c.safety_class, SafetyClass::SecretBearing);
    }

    #[test]
    fn redacts_project_before_home() {
        let p = portable_path(
            Path::new("/Users/me/repo/AGENTS.md"),
            Path::new("/Users/me"),
            Path::new("/Users/me/repo"),
        );
        assert_eq!(p, "<project>/AGENTS.md");
    }
}
