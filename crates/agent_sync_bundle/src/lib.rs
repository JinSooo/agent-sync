use agent_sync_core::{DeviceSnapshot, ProjectIdentity, SafetyClass, SessionRecord};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const BUNDLE_ENCRYPTION_METHOD_AGE_SCRYPT: &str = "age:scrypt:v1";

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PayloadSelectionRef {
    pub agent_id: String,
    pub portable_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionArchiveEntry {
    pub agent_id: String,
    pub agent_name: String,
    pub session: SessionRecord,
    pub source_project: Option<ProjectIdentity>,
    pub payload_included: bool,
    #[serde(default)]
    pub payloads: Vec<PayloadEntry>,
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
    pub selected_review_payloads: Vec<PayloadSelectionRef>,
    pub include_session_payloads: bool,
    pub selected_session_ids: Vec<String>,
    pub max_session_payload_bytes: u64,
    pub allow_unencrypted_sensitive_payloads: bool,
    pub encryption_passphrase: Option<String>,
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
            required_for_sensitive_payloads: false,
            method: "none:not_required_for_manifest_preview".to_string(),
        },
    }
}

pub fn export_bundle(
    snapshot: &DeviceSnapshot,
    options: &BundleExportOptions,
) -> std::io::Result<SyncBundle> {
    let sensitive_payload_requested = sensitive_payload_requested(snapshot, options);
    let encrypted_export =
        normalized_passphrase(options.encryption_passphrase.as_deref()).is_some();
    if sensitive_payload_requested
        && !encrypted_export
        && !options.allow_unencrypted_sensitive_payloads
    {
        return Err(std::io::Error::other(
            "selected memory/MCP or raw session payloads are sensitive; provide a bundle passphrase or pass explicit unencrypted export acknowledgement or deselect them",
        ));
    }
    let mut manifest = manifest_from_snapshot(snapshot);
    manifest.encryption = BundleEncryptionInfo {
        required_for_sensitive_payloads: sensitive_payload_requested,
        method: if encrypted_export {
            BUNDLE_ENCRYPTION_METHOD_AGE_SCRYPT.to_string()
        } else if sensitive_payload_requested {
            "none:explicit_unencrypted_sensitive_payloads".to_string()
        } else {
            "none:not_required_for_selected_payloads".to_string()
        },
    };
    for selection in &mut manifest.selections {
        if is_explicit_review_payload(selection, &options.selected_review_payloads) {
            selection.include_payload = true;
        }
    }
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
        session_archives: session_archives_from_snapshot(snapshot, options)?,
    })
}

pub fn write_bundle_file(bundle: &SyncBundle, path: impl AsRef<Path>) -> std::io::Result<()> {
    write_bundle_file_with_passphrase(bundle, path, None)
}

pub fn write_bundle_file_with_passphrase(
    bundle: &SyncBundle,
    path: impl AsRef<Path>,
    passphrase: Option<&str>,
) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(bundle).map_err(std::io::Error::other)?;
    let bytes = match normalized_passphrase(passphrase) {
        Some(passphrase) => encrypt_bundle_bytes(&json, passphrase)?,
        None => json,
    };
    fs::write(path, bytes)
}

pub fn read_bundle_file(path: impl AsRef<Path>) -> std::io::Result<SyncBundle> {
    read_bundle_file_with_passphrase(path, None)
}

pub fn read_bundle_file_with_passphrase(
    path: impl AsRef<Path>,
    passphrase: Option<&str>,
) -> std::io::Result<SyncBundle> {
    let bytes = fs::read(path)?;
    match serde_json::from_slice(&bytes) {
        Ok(bundle) => Ok(bundle),
        Err(json_error) => {
            let Some(passphrase) = normalized_passphrase(passphrase) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "bundle is not readable plaintext JSON and may be encrypted; provide a bundle passphrase ({json_error})"
                    ),
                ));
            };
            let plaintext = decrypt_bundle_bytes(&bytes, passphrase)?;
            serde_json::from_slice(&plaintext).map_err(std::io::Error::other)
        }
    }
}

pub fn verify_bundle(bundle: &SyncBundle) -> Vec<String> {
    let mut errors = Vec::new();
    for payload in &bundle.payloads {
        verify_payload(payload, &mut errors);
    }
    for archive in &bundle.session_archives {
        for payload in &archive.payloads {
            verify_payload(payload, &mut errors);
        }
    }
    errors
}

fn verify_payload(payload: &PayloadEntry, errors: &mut Vec<String>) {
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

fn normalized_passphrase(passphrase: Option<&str>) -> Option<&str> {
    passphrase.filter(|value| !value.is_empty())
}

fn encrypt_bundle_bytes(plaintext: &[u8], passphrase: &str) -> std::io::Result<Vec<u8>> {
    let secret = age::secrecy::SecretString::from(passphrase.to_owned());
    let recipient = age::scrypt::Recipient::new(secret);
    age::encrypt(&recipient, plaintext).map_err(std::io::Error::other)
}

fn decrypt_bundle_bytes(ciphertext: &[u8], passphrase: &str) -> std::io::Result<Vec<u8>> {
    let secret = age::secrecy::SecretString::from(passphrase.to_owned());
    let identity = age::scrypt::Identity::new(secret);
    age::decrypt(&identity, ciphertext).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("failed to decrypt bundle with provided passphrase: {error}"),
        )
    })
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

fn is_explicit_review_payload(
    selection: &SelectionRef,
    selected_review_payloads: &[PayloadSelectionRef],
) -> bool {
    matches!(
        selection.safety_class,
        SafetyClass::McpConfig | SafetyClass::MemoryKnowledge
    ) && selected_review_payloads.iter().any(|selected| {
        selected.agent_id == selection.agent_id && selected.portable_path == selection.portable_path
    })
}

fn sensitive_payload_requested(snapshot: &DeviceSnapshot, options: &BundleExportOptions) -> bool {
    let review_payload_requested = !options.selected_review_payloads.is_empty();
    let session_payload_requested = if !options.include_session_payloads {
        false
    } else if options.selected_session_ids.is_empty() {
        snapshot
            .agents
            .iter()
            .any(|agent| !agent.sessions.is_empty())
    } else {
        !options.selected_session_ids.is_empty()
    };
    review_payload_requested || session_payload_requested
}

fn session_archives_from_snapshot(
    snapshot: &DeviceSnapshot,
    options: &BundleExportOptions,
) -> std::io::Result<Vec<SessionArchiveEntry>> {
    let include_all = options.include_session_payloads && options.selected_session_ids.is_empty();
    snapshot
        .agents
        .iter()
        .flat_map(|agent| {
            agent
                .sessions
                .iter()
                .map(move |session| {
                    let selected = include_all
                        || options
                            .selected_session_ids
                            .iter()
                            .any(|id| id == &session.id);
                    let payloads = if options.include_session_payloads && selected {
                        session_payloads(agent.id.as_str(), session, options)?
                    } else {
                        Vec::new()
                    };
                    let payload_included = !payloads.is_empty();
                    Ok::<_, std::io::Error>(
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
                payload_included,
                payloads,
                import_note: if payload_included {
                    "explicitly selected raw session payload included for staging import"
                        .to_string()
                } else {
                    "metadata-only archive; raw transcript/native-session payload is not included"
                        .to_string()
                },
            })
                })
        })
        .collect()
}

fn session_payloads(
    agent_id: &str,
    session: &SessionRecord,
    options: &BundleExportOptions,
) -> std::io::Result<Vec<PayloadEntry>> {
    let mut payloads = Vec::new();
    for storage_ref in &session.storage_refs {
        let Some(path) = physical_path(&storage_ref.portable_path, &options.home, &options.project)
        else {
            continue;
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata)
                if metadata.is_file() && metadata.len() <= options.max_session_payload_bytes =>
            {
                metadata
            }
            _ => continue,
        };
        if metadata.len() > options.max_session_payload_bytes {
            continue;
        }
        let bytes = fs::read(&path)?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        payloads.push(PayloadEntry {
            agent_id: agent_id.to_string(),
            portable_path: storage_ref.portable_path.clone(),
            sha256,
            base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(payloads)
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
                capabilities: Default::default(),
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
                selected_review_payloads: vec![],
                include_session_payloads: false,
                selected_session_ids: vec![],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: None,
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

    #[test]
    fn includes_selected_session_payloads_only_when_requested() {
        let root = std::env::temp_dir().join(format!(
            "agent-sync-session-payload-{}",
            uuid::Uuid::new_v4()
        ));
        let home = root.join("home");
        let project = root.join("project");
        let session_path = home.join(".codex").join("sessions").join("s1.jsonl");
        fs::create_dir_all(session_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(&session_path, "{\"cwd\":\"/tmp/project\"}\n").unwrap();
        let session = SessionRecord {
            id: "codex:session-1".into(),
            agent_id: "codex".into(),
            title: Some("Session 1".into()),
            created_at: None,
            updated_at: None,
            source_project: None,
            storage_refs: vec![agent_sync_core::StorageRef {
                kind: "raw_session_surface".into(),
                portable_path: "~/.codex/sessions/s1.jsonl".into(),
                physical_path: None,
            }],
            visibility: SessionVisibility::Unknown,
            content_policy: ContentPolicy::ExplicitRawPayloadRequired,
            import_capabilities: SessionImportCapabilities {
                import_as_archive: true,
                import_as_new_session: false,
                identity_rewrite: false,
                requires_app_stopped: true,
            },
        };
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
                max_depth: 8,
                max_entries: 10,
            },
            summary: SnapshotSummary::default(),
            projects: vec![],
            agents: vec![AgentSnapshot {
                id: "codex".into(),
                name: "Codex".into(),
                detected: true,
                capabilities: Default::default(),
                roots: vec![],
                findings: vec![],
                sessions: vec![session],
            }],
        };

        let metadata_only = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: home.clone(),
                project: project.clone(),
                max_payload_bytes: 1024,
                selected_review_payloads: vec![],
                include_session_payloads: false,
                selected_session_ids: vec!["codex:session-1".into()],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: None,
            },
        )
        .unwrap();
        assert!(!metadata_only.session_archives[0].payload_included);
        assert!(metadata_only.session_archives[0].payloads.is_empty());

        let without_sensitive_ack = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: home.clone(),
                project: project.clone(),
                max_payload_bytes: 1024,
                selected_review_payloads: vec![],
                include_session_payloads: true,
                selected_session_ids: vec!["codex:session-1".into()],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: None,
            },
        );
        assert!(without_sensitive_ack.is_err());

        let with_payload = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home,
                project,
                max_payload_bytes: 1024,
                selected_review_payloads: vec![],
                include_session_payloads: true,
                selected_session_ids: vec!["codex:session-1".into()],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: true,
                encryption_passphrase: None,
            },
        )
        .unwrap();
        assert!(with_payload.session_archives[0].payload_included);
        assert_eq!(with_payload.session_archives[0].payloads.len(), 1);
        assert!(verify_bundle(&with_payload).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn includes_selected_review_payloads_only_when_explicit() {
        let root = std::env::temp_dir().join(format!(
            "agent-sync-review-payload-{}",
            uuid::Uuid::new_v4()
        ));
        let home = root.join("home");
        let project = root.join("project");
        let memory_path = home.join(".codex").join("memories").join("guide.md");
        fs::create_dir_all(memory_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(&memory_path, "# useful memory\n").unwrap();
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
                max_depth: 4,
                max_entries: 10,
            },
            summary: SnapshotSummary::default(),
            projects: vec![],
            agents: vec![AgentSnapshot {
                id: "codex".into(),
                name: "Codex".into(),
                detected: true,
                capabilities: Default::default(),
                roots: vec![],
                findings: vec![Finding {
                    path: memory_path.display().to_string(),
                    portable_path: "~/.codex/memories/guide.md".into(),
                    kind: FileKind::File,
                    depth: 0,
                    size: Some(16),
                    mtime: None,
                    safety_class: SafetyClass::MemoryKnowledge,
                    risk: RiskLevel::MediumHigh,
                    reason: "test".into(),
                    recommendation: "review".into(),
                    truncated: false,
                }],
                sessions: vec![],
            }],
        };

        let metadata_only = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: home.clone(),
                project: project.clone(),
                max_payload_bytes: 1024,
                selected_review_payloads: vec![],
                include_session_payloads: false,
                selected_session_ids: vec![],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: None,
            },
        )
        .unwrap();
        assert!(metadata_only.payloads.is_empty());
        assert!(!metadata_only.manifest.selections[0].include_payload);

        let without_sensitive_ack = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: home.clone(),
                project: project.clone(),
                max_payload_bytes: 1024,
                selected_review_payloads: vec![PayloadSelectionRef {
                    agent_id: "codex".into(),
                    portable_path: "~/.codex/memories/guide.md".into(),
                }],
                include_session_payloads: false,
                selected_session_ids: vec![],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: None,
            },
        );
        assert!(without_sensitive_ack.is_err());

        let with_encrypted_payload = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home: home.clone(),
                project: project.clone(),
                max_payload_bytes: 1024,
                selected_review_payloads: vec![PayloadSelectionRef {
                    agent_id: "codex".into(),
                    portable_path: "~/.codex/memories/guide.md".into(),
                }],
                include_session_payloads: false,
                selected_session_ids: vec![],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: false,
                encryption_passphrase: Some("correct horse battery staple".to_string()),
            },
        )
        .unwrap();
        assert_eq!(
            with_encrypted_payload.manifest.encryption.method,
            BUNDLE_ENCRYPTION_METHOD_AGE_SCRYPT
        );
        assert_eq!(with_encrypted_payload.payloads.len(), 1);
        assert!(verify_bundle(&with_encrypted_payload).is_empty());

        let encrypted_bundle_path = root.join("memory.asbundle");
        write_bundle_file_with_passphrase(
            &with_encrypted_payload,
            &encrypted_bundle_path,
            Some("correct horse battery staple"),
        )
        .unwrap();
        let encrypted_bytes = fs::read(&encrypted_bundle_path).unwrap();
        assert!(!encrypted_bytes.starts_with(b"{"));
        assert!(!String::from_utf8_lossy(&encrypted_bytes).contains("useful memory"));
        assert!(read_bundle_file(&encrypted_bundle_path).is_err());
        assert!(
            read_bundle_file_with_passphrase(&encrypted_bundle_path, Some("wrong passphrase"))
                .is_err()
        );
        let decrypted = read_bundle_file_with_passphrase(
            &encrypted_bundle_path,
            Some("correct horse battery staple"),
        )
        .unwrap();
        assert_eq!(decrypted, with_encrypted_payload);

        let with_payload = export_bundle(
            &snapshot,
            &BundleExportOptions {
                home,
                project,
                max_payload_bytes: 1024,
                selected_review_payloads: vec![PayloadSelectionRef {
                    agent_id: "codex".into(),
                    portable_path: "~/.codex/memories/guide.md".into(),
                }],
                include_session_payloads: false,
                selected_session_ids: vec![],
                max_session_payload_bytes: 1024,
                allow_unencrypted_sensitive_payloads: true,
                encryption_passphrase: None,
            },
        )
        .unwrap();
        assert!(with_payload.manifest.selections[0].include_payload);
        assert_eq!(with_payload.payloads.len(), 1);
        assert_eq!(
            with_payload.payloads[0].portable_path,
            "~/.codex/memories/guide.md"
        );
        assert!(verify_bundle(&with_payload).is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
