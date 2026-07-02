use agent_sync_core::{DeviceSnapshot, ProjectIdentity, SafetyClass, SessionRecord};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncBundleManifest {
    pub schema_version: String,
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub source_snapshot: Uuid,
    pub selections: Vec<SelectionRef>,
    pub redactions: Vec<RedactionRecord>,
    pub encryption: BundleEncryptionInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncBundle {
    pub manifest: SyncBundleManifest,
    pub source_snapshot: DeviceSnapshot,
    pub payloads: Vec<PayloadEntry>,
    #[serde(default)]
    pub session_archives: Vec<SessionArchiveEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadEntry {
    pub agent_id: String,
    pub portable_path: String,
    pub sha256: String,
    pub base64_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionArchiveEntry {
    pub agent_id: String,
    pub agent_name: String,
    pub session: SessionRecord,
    pub source_project: Option<ProjectIdentity>,
    pub payload_included: bool,
    pub import_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionRef {
    pub agent_id: String,
    pub portable_path: String,
    pub safety_class: SafetyClass,
    pub include_payload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactionRecord {
    pub agent_id: String,
    pub portable_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleEncryptionInfo {
    pub required_for_sensitive_payloads: bool,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleExportOptions {
    pub home: PathBuf,
    pub project: PathBuf,
    pub max_payload_bytes: u64,
}

pub fn manifest_from_snapshot(snapshot: &DeviceSnapshot) -> SyncBundleManifest {
    let (selections, redactions) = classify_export_entries(snapshot);
    SyncBundleManifest {
        schema_version: "0.2".to_string(),
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        source_snapshot: snapshot.id,
        selections,
        redactions,
        encryption: BundleEncryptionInfo {
            required_for_sensitive_payloads: true,
            method: "planned:xchacha20-poly1305".to_string(),
        },
    }
}

pub fn export_bundle(
    snapshot: &DeviceSnapshot,
    options: &BundleExportOptions,
) -> std::io::Result<SyncBundle> {
    let manifest = manifest_from_snapshot(snapshot);
    let mut payloads = Vec::new();
    for selection in &manifest.selections {
        if !selection.include_payload {
            continue;
        }
        let Some(path) = physical_path(&selection.portable_path, &options.home, &options.project)
        else {
            continue;
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= options.max_payload_bytes => {
                metadata
            }
            _ => continue,
        };
        if metadata.len() > options.max_payload_bytes {
            continue;
        }
        let bytes = fs::read(&path)?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        payloads.push(PayloadEntry {
            agent_id: selection.agent_id.clone(),
            portable_path: selection.portable_path.clone(),
            sha256,
            base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(SyncBundle {
        manifest,
        source_snapshot: snapshot.clone(),
        payloads,
        session_archives: session_archives_from_snapshot(snapshot),
    })
}

pub fn write_bundle_file(bundle: &SyncBundle, path: impl AsRef<Path>) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(bundle).map_err(std::io::Error::other)?;
    fs::write(path, json)
}

pub fn read_bundle_file(path: impl AsRef<Path>) -> std::io::Result<SyncBundle> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

pub fn verify_bundle(bundle: &SyncBundle) -> Vec<String> {
    let mut errors = Vec::new();
    for payload in &bundle.payloads {
        match base64::engine::general_purpose::STANDARD.decode(&payload.base64_content) {
            Ok(bytes) => {
                let sha256 = format!("{:x}", Sha256::digest(&bytes));
                if sha256 != payload.sha256 {
                    errors.push(format!("checksum mismatch for {}", payload.portable_path));
                }
            }
            Err(error) => errors.push(format!(
                "invalid base64 for {}: {}",
                payload.portable_path, error
            )),
        }
    }
    errors
}

fn classify_export_entries(snapshot: &DeviceSnapshot) -> (Vec<SelectionRef>, Vec<RedactionRecord>) {
    let mut selections = Vec::new();
    let mut redactions = Vec::new();
    for agent in &snapshot.agents {
        for finding in &agent.findings {
            if matches!(finding.safety_class, SafetyClass::SecretBearing) {
                redactions.push(RedactionRecord {
                    agent_id: agent.id.clone(),
                    portable_path: finding.portable_path.clone(),
                    reason: "secret-bearing surfaces are never exported".to_string(),
                });
            } else {
                selections.push(SelectionRef {
                    agent_id: agent.id.clone(),
                    portable_path: finding.portable_path.clone(),
                    safety_class: finding.safety_class.clone(),
                    include_payload: matches!(finding.safety_class, SafetyClass::SafeConfig),
                });
            }
        }
    }
    (selections, redactions)
}

fn session_archives_from_snapshot(snapshot: &DeviceSnapshot) -> Vec<SessionArchiveEntry> {
    snapshot
        .agents
        .iter()
        .flat_map(|agent| {
            agent
                .sessions
                .iter()
                .map(move |session| {
                    SessionArchiveEntry {
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                session: session.clone(),
                source_project: session
                    .source_project
                    .and_then(|id| snapshot.projects.iter().find(|project| project.id == id))
                    .cloned()
                    .or_else(|| {
                        (snapshot.projects.len() == 1).then(|| snapshot.projects[0].clone())
                    }),
                payload_included: false,
                import_note:
                    "metadata-only archive; raw transcript/native-session payload is not included"
                        .to_string(),
            }
                })
        })
        .collect()
}

fn physical_path(portable: &str, home: &Path, project: &Path) -> Option<PathBuf> {
    if portable == "~" {
        return Some(home.to_path_buf());
    }
    if let Some(rest) = portable.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    if portable == "<project>" {
        return Some(project.to_path_buf());
    }
    if let Some(rest) = portable.strip_prefix("<project>/") {
        return Some(project.join(rest));
    }
    Some(PathBuf::from(portable))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_sync_core::{
        AgentSnapshot, ContentPolicy, FileKind, Finding, PlatformInfo, RiskLevel, RootRecord,
        SessionImportCapabilities, SessionRecord, SessionVisibility, SnapshotInputs,
        SnapshotSummary,
    };

    #[test]
    fn exports_safe_payload_and_redacts_secret() {
        let root = std::env::temp_dir().join(format!("agent-sync-bundle-{}", uuid::Uuid::new_v4()));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("AGENTS.md"), "ok").unwrap();
        fs::write(project.join("auth.json"), "secret").unwrap();
        let snapshot = DeviceSnapshot {
            schema_version: "0.2".into(),
            id: Uuid::new_v4(),
            generated_at: Utc::now(),
            platform: PlatformInfo {
                os: "test".into(),
                arch: "test".into(),
            },
            inputs: SnapshotInputs {
                home: "~".into(),
                project: project.display().to_string(),
                max_depth: 1,
                max_entries: 10,
            },
            summary: SnapshotSummary::default(),
            projects: vec![],
            agents: vec![AgentSnapshot {
                id: "codex".into(),
                name: "Codex".into(),
                detected: true,
                roots: vec![RootRecord {
                    path: "<project>/AGENTS.md".into(),
                    scope: "project".into(),
                    exists: true,
                    note: None,
                }],
                findings: vec![
                    Finding {
                        path: "<project>/AGENTS.md".into(),
                        portable_path: "<project>/AGENTS.md".into(),
                        kind: FileKind::File,
                        depth: 0,
                        size: Some(2),
                        mtime: None,
                        safety_class: SafetyClass::SafeConfig,
                        risk: RiskLevel::LowMedium,
                        reason: "r".into(),
                        recommendation: "x".into(),
                        truncated: false,
                    },
                    Finding {
                        path: "<project>/auth.json".into(),
                        portable_path: "<project>/auth.json".into(),
                        kind: FileKind::File,
                        depth: 0,
                        size: Some(6),
                        mtime: None,
                        safety_class: SafetyClass::SecretBearing,
                        risk: RiskLevel::Critical,
                        reason: "r".into(),
                        recommendation: "x".into(),
                        truncated: false,
                    },
                ],
                sessions: vec![SessionRecord {
                    id: "codex:session-1".into(),
                    agent_id: "codex".into(),
                    title: Some("Session 1".into()),
                    created_at: None,
                    updated_at: None,
                    source_project: None,
                    storage_refs: vec![],
                    visibility: SessionVisibility::Unknown,
                    content_policy: ContentPolicy::ExplicitRawPayloadRequired,
                    import_capabilities: SessionImportCapabilities {
                        import_as_archive: true,
                        import_as_new_session: false,
                        identity_rewrite: false,
                        requires_app_stopped: true,
                    },
                }],
            }],
        };
        let bundle = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: root.join("home"),
                project: project.clone(),
                max_payload_bytes: 1024,
            },
        )
        .unwrap();
        assert_eq!(bundle.payloads.len(), 1);
        assert_eq!(bundle.session_archives.len(), 1);
        assert_eq!(bundle.session_archives[0].session.id, "codex:session-1");
        assert!(!bundle.session_archives[0].payload_included);
        assert_eq!(bundle.manifest.redactions.len(), 1);
        assert!(verify_bundle(&bundle).is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
