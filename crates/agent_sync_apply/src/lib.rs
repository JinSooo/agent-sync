use agent_sync_bundle::SyncBundle;
use agent_sync_core::SafetyClass;
use agent_sync_storage::AgentSyncStore;
use agent_sync_transform::{ApplyOperationKind, TransformPlan};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationJournal {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub transform_plan: Uuid,
    pub operations: Vec<JournaledOperation>,
    pub status: JournalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournaledOperation {
    pub operation: agent_sync_transform::ApplyOperation,
    pub backup: Option<BackupRef>,
    pub status: JournalOperationStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRef {
    pub id: Uuid,
    pub original_path: String,
    pub backup_path: String,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalStatus {
    Draft,
    PreflightPassed,
    BackupCreated,
    Applying,
    Verifying,
    Completed,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalOperationStatus {
    Pending,
    Blocked,
    Applied,
    Verified,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreflightReport {
    pub plan_id: Uuid,
    pub passed: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub operations_requiring_review: usize,
    pub operations_requiring_backup: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyContext {
    pub target_home: PathBuf,
    pub target_project: PathBuf,
    pub backup_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionArchiveImportOptions {
    pub selected_session_ids: Vec<String>,
    pub target_project: Option<String>,
    pub target_project_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionArchiveImportJournal {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub bundle_id: Uuid,
    pub source_snapshot: Uuid,
    pub selected: usize,
    pub imported: usize,
    pub skipped: usize,
    pub records: Vec<SessionArchiveImportRecord>,
    pub status: JournalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionArchiveImportRecord {
    pub record_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub session_id: String,
    pub title: Option<String>,
    pub source_project: Option<String>,
    pub target_project: Option<String>,
    pub payload_included: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeImportStageOptions {
    pub selected_session_ids: Vec<String>,
    pub target_project: Option<String>,
    pub staging_dir: PathBuf,
    pub rewrite_project_identity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeImportStageJournal {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub bundle_id: Uuid,
    pub source_snapshot: Uuid,
    pub selected: usize,
    pub staged: usize,
    pub skipped: usize,
    pub records: Vec<SessionNativeImportStageRecord>,
    pub status: JournalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeImportStageRecord {
    pub agent_id: String,
    pub agent_name: String,
    pub session_id: String,
    pub title: Option<String>,
    pub source_project: Option<String>,
    pub target_project: Option<String>,
    pub written_payloads: Vec<StagedPayloadRecord>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedPayloadRecord {
    pub portable_path: String,
    pub staged_path: String,
    pub source_sha256: String,
    pub staged_sha256: String,
    pub project_identity_rewritten: bool,
}

pub fn create_journal(plan: &TransformPlan) -> OperationJournal {
    OperationJournal {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        transform_plan: plan.id,
        operations: plan
            .operations
            .iter()
            .cloned()
            .map(|operation| JournaledOperation {
                operation,
                backup: None,
                status: JournalOperationStatus::Pending,
                message: None,
            })
            .collect(),
        status: JournalStatus::Draft,
    }
}

pub fn import_session_archives(
    store: &AgentSyncStore,
    bundle: &SyncBundle,
    options: &SessionArchiveImportOptions,
) -> rusqlite::Result<SessionArchiveImportJournal> {
    let mut records = Vec::new();
    let selected = options.selected_session_ids.len();
    for archive in &bundle.session_archives {
        if !options
            .selected_session_ids
            .iter()
            .any(|id| id == &archive.session.id)
        {
            continue;
        }
        let record = SessionArchiveImportRecord {
            record_id: String::new(),
            agent_id: archive.agent_id.clone(),
            agent_name: archive.agent_name.clone(),
            session_id: archive.session.id.clone(),
            title: archive.session.title.clone(),
            source_project: archive
                .source_project
                .as_ref()
                .map(|project| project.canonical_path.clone()),
            target_project: options.target_project.clone(),
            payload_included: archive.payload_included,
            note: if archive.payload_included {
                "raw payload archived for adapter-specific import".to_string()
            } else {
                "metadata-only archive imported; native session rewrite not performed".to_string()
            },
        };
        let id = store.save_json("session_archive", None, &record)?;
        records.push(SessionArchiveImportRecord {
            record_id: id,
            ..record
        });
    }

    let imported = records.len();
    Ok(SessionArchiveImportJournal {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        bundle_id: bundle.manifest.id,
        source_snapshot: bundle.source_snapshot.id,
        selected,
        imported,
        skipped: selected.saturating_sub(imported),
        records,
        status: JournalStatus::Completed,
    })
}

pub fn stage_session_native_import(
    bundle: &SyncBundle,
    options: &SessionNativeImportStageOptions,
) -> std::io::Result<SessionNativeImportStageJournal> {
    fs::create_dir_all(&options.staging_dir)?;
    let mut records = Vec::new();
    let selected = options.selected_session_ids.len();

    for archive in &bundle.session_archives {
        if !options
            .selected_session_ids
            .iter()
            .any(|id| id == &archive.session.id)
        {
            continue;
        }
        if archive.payloads.is_empty() {
            continue;
        }

        let session_dir = options
            .staging_dir
            .join(&archive.agent_id)
            .join(sanitize_path(&archive.session.id));
        fs::create_dir_all(&session_dir)?;

        let mut written_payloads = Vec::new();
        for payload in &archive.payloads {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&payload.base64_content)
                .map_err(std::io::Error::other)?;
            let source_sha = format!("{:x}", Sha256::digest(&bytes));
            if source_sha != payload.sha256 {
                return Err(std::io::Error::other(format!(
                    "payload checksum mismatch for {}",
                    payload.portable_path
                )));
            }
            let (next_bytes, rewritten) = rewrite_session_project_identity(
                bytes,
                archive
                    .source_project
                    .as_ref()
                    .map(|project| project.canonical_path.as_str()),
                archive
                    .source_project
                    .as_ref()
                    .and_then(|project| project.physical_path.as_deref()),
                options.target_project.as_deref(),
                options.rewrite_project_identity,
            );
            let staged_sha = format!("{:x}", Sha256::digest(&next_bytes));
            let staged_path = session_dir.join(sanitize_path(&payload.portable_path));
            fs::write(&staged_path, next_bytes)?;
            written_payloads.push(StagedPayloadRecord {
                portable_path: payload.portable_path.clone(),
                staged_path: staged_path.display().to_string(),
                source_sha256: payload.sha256.clone(),
                staged_sha256: staged_sha,
                project_identity_rewritten: rewritten,
            });
        }

        records.push(SessionNativeImportStageRecord {
            agent_id: archive.agent_id.clone(),
            agent_name: archive.agent_name.clone(),
            session_id: archive.session.id.clone(),
            title: archive.session.title.clone(),
            source_project: archive
                .source_project
                .as_ref()
                .map(|project| project.canonical_path.clone()),
            target_project: options.target_project.clone(),
            written_payloads,
            note: "staged only; native Codex/Claude index/database write is not performed"
                .to_string(),
        });
    }

    let staged = records.len();
    Ok(SessionNativeImportStageJournal {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        bundle_id: bundle.manifest.id,
        source_snapshot: bundle.source_snapshot.id,
        selected,
        staged,
        skipped: selected.saturating_sub(staged),
        records,
        status: JournalStatus::Completed,
    })
}

fn rewrite_session_project_identity(
    bytes: Vec<u8>,
    source_canonical_path: Option<&str>,
    source_physical_path: Option<&str>,
    target_project: Option<&str>,
    enabled: bool,
) -> (Vec<u8>, bool) {
    if !enabled {
        return (bytes, false);
    }
    let Some(target_project) = target_project.filter(|value| !value.is_empty()) else {
        return (bytes, false);
    };
    let Ok(mut text) = String::from_utf8(bytes.clone()) else {
        return (bytes, false);
    };
    let mut rewritten = false;
    for candidate in [source_canonical_path, source_physical_path]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty() && *value != target_project)
    {
        if text.contains(candidate) {
            text = text.replace(candidate, target_project);
            rewritten = true;
        }
        let escaped_candidate = candidate.replace('\\', "\\\\");
        if text.contains(&escaped_candidate) {
            text = text.replace(&escaped_candidate, &target_project.replace('\\', "\\\\"));
            rewritten = true;
        }
    }
    if rewritten {
        (text.into_bytes(), true)
    } else {
        (bytes, false)
    }
}

pub fn preflight(plan: &TransformPlan) -> PreflightReport {
    let mut blockers = Vec::new();
    if !plan.blocked.is_empty() {
        blockers.push(format!(
            "{} blocked operations must remain excluded or be resolved through adapter-specific flows",
            plan.blocked.len()
        ));
    }
    if plan.operations.iter().any(|op| !op.requires_backup) {
        blockers.push("all write operations must declare backup requirements".to_string());
    }
    PreflightReport {
        plan_id: plan.id,
        passed: blockers.is_empty(),
        blockers,
        warnings: plan
            .warnings
            .iter()
            .map(|warning| format!("{}: {}", warning.code, warning.message))
            .collect(),
        operations_requiring_review: plan
            .operations
            .iter()
            .filter(|op| op.requires_review)
            .count(),
        operations_requiring_backup: plan
            .operations
            .iter()
            .filter(|op| op.requires_backup)
            .count(),
    }
}

pub fn apply_safe_payloads(
    bundle: &SyncBundle,
    plan: &TransformPlan,
    context: &ApplyContext,
) -> std::io::Result<OperationJournal> {
    fs::create_dir_all(&context.backup_dir)?;
    let payloads = bundle
        .payloads
        .iter()
        .map(|payload| {
            (
                (payload.agent_id.clone(), payload.portable_path.clone()),
                payload,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut journal = create_journal(plan);
    journal.status = JournalStatus::Applying;

    for entry in &mut journal.operations {
        if entry.operation.requires_review
            || entry.operation.safety_class != SafetyClass::SafeConfig
            || !matches!(
                entry.operation.kind,
                ApplyOperationKind::MergeText | ApplyOperationKind::CopyFile
            )
        {
            entry.status = JournalOperationStatus::Blocked;
            entry.message = Some("operation requires review or adapter-specific apply".to_string());
            continue;
        }

        let payload_key = (
            entry.operation.agent_id.clone(),
            entry.operation.path.clone(),
        );
        let Some(payload) = payloads.get(&payload_key) else {
            entry.status = JournalOperationStatus::Failed;
            entry.message = Some("bundle payload missing".to_string());
            continue;
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&payload.base64_content)
            .map_err(std::io::Error::other)?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if sha256 != payload.sha256 {
            entry.status = JournalOperationStatus::Failed;
            entry.message = Some("payload checksum mismatch".to_string());
            continue;
        }

        let target = materialize_path(
            &entry.operation.path,
            &context.target_home,
            &context.target_project,
        );
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.exists() {
            let backup = context.backup_dir.join(format!(
                "{}-{}",
                Uuid::new_v4(),
                sanitize_path(&entry.operation.path)
            ));
            fs::copy(&target, &backup)?;
            entry.backup = Some(BackupRef {
                id: Uuid::new_v4(),
                original_path: target.display().to_string(),
                backup_path: backup.display().to_string(),
                checksum: Some(format!("{:x}", Sha256::digest(fs::read(&target)?))),
            });
        }
        fs::write(&target, &bytes)?;
        let written = fs::read(&target)?;
        let written_sha = format!("{:x}", Sha256::digest(&written));
        if written_sha == payload.sha256 {
            entry.status = JournalOperationStatus::Verified;
            entry.message = Some("applied and checksum verified".to_string());
        } else {
            entry.status = JournalOperationStatus::Failed;
            entry.message = Some("written checksum mismatch".to_string());
        }
    }

    journal.status = if journal
        .operations
        .iter()
        .any(|entry| matches!(entry.status, JournalOperationStatus::Failed))
    {
        JournalStatus::Failed
    } else {
        JournalStatus::Completed
    };
    Ok(journal)
}

fn materialize_path(portable: &str, home: &Path, project: &Path) -> PathBuf {
    if portable == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = portable.strip_prefix("~/") {
        return home.join(rest);
    }
    if portable == "<project>" {
        return project.to_path_buf();
    }
    if let Some(rest) = portable.strip_prefix("<project>/") {
        return project.join(rest);
    }
    PathBuf::from(portable)
}

fn sanitize_path(path: &str) -> String {
    path.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_sync_bundle::PayloadEntry;
    use agent_sync_transform::{ApplyOperation, ChangeType, TransformSummary, TransformWarning};

    #[test]
    fn empty_plan_preflight_passes() {
        let plan = TransformPlan {
            id: uuid::Uuid::new_v4(),
            generated_at: chrono::Utc::now(),
            source_snapshot: uuid::Uuid::new_v4(),
            target_snapshot: uuid::Uuid::new_v4(),
            target_platform: "darwin".into(),
            operations: vec![],
            project_mappings: vec![],
            blocked: vec![],
            warnings: vec![],
            summary: Default::default(),
        };
        let report = preflight(&plan);
        assert!(report.passed);
        let journal = create_journal(&plan);
        assert_eq!(journal.status, JournalStatus::Draft);
    }

    #[test]
    fn applies_safe_payload_with_backup() {
        let root = std::env::temp_dir().join(format!("agent-sync-apply-{}", Uuid::new_v4()));
        let project = root.join("project");
        let backup = root.join("backup");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("AGENTS.md"), "old").unwrap();
        let bytes = b"new".to_vec();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let bundle = SyncBundle {
            manifest: agent_sync_bundle::SyncBundleManifest {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                source_snapshot: Uuid::new_v4(),
                selections: vec![],
                redactions: vec![],
                encryption: agent_sync_bundle::BundleEncryptionInfo {
                    required_for_sensitive_payloads: true,
                    method: "none".into(),
                },
            },
            source_snapshot: agent_sync_core::DeviceSnapshot {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                generated_at: Utc::now(),
                platform: agent_sync_core::PlatformInfo {
                    os: "test".into(),
                    arch: "test".into(),
                },
                inputs: agent_sync_core::SnapshotInputs {
                    home: "~".into(),
                    project: project.display().to_string(),
                    max_depth: 1,
                    max_entries: 10,
                },
                summary: agent_sync_core::SnapshotSummary::default(),
                projects: vec![],
                agents: vec![],
            },
            payloads: vec![PayloadEntry {
                agent_id: "codex".into(),
                portable_path: "<project>/AGENTS.md".into(),
                sha256: sha,
                base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
            }],
            session_archives: vec![],
        };
        let plan = TransformPlan {
            id: Uuid::new_v4(),
            generated_at: Utc::now(),
            source_snapshot: Uuid::new_v4(),
            target_snapshot: Uuid::new_v4(),
            target_platform: "darwin".into(),
            operations: vec![ApplyOperation {
                id: Uuid::new_v4(),
                agent_id: "codex".into(),
                agent_name: "Codex".into(),
                path: "<project>/AGENTS.md".into(),
                kind: ApplyOperationKind::MergeText,
                safety_class: SafetyClass::SafeConfig,
                risk: "low-medium".into(),
                rationale: "test".into(),
                change_type: ChangeType::ChangedBetweenSnapshots,
                path_warnings: vec![],
                requires_review: false,
                requires_backup: true,
            }],
            project_mappings: vec![],
            blocked: vec![],
            warnings: vec![TransformWarning {
                path: None,
                code: "test".into(),
                message: "test".into(),
            }],
            summary: TransformSummary::default(),
        };
        let journal = apply_safe_payloads(
            &bundle,
            &plan,
            &ApplyContext {
                target_home: root.join("home"),
                target_project: project.clone(),
                backup_dir: backup.clone(),
            },
        )
        .unwrap();
        assert_eq!(journal.status, JournalStatus::Completed);
        assert_eq!(
            fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "new"
        );
        assert!(backup.read_dir().unwrap().next().is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn imports_selected_session_archives_to_store() {
        let store = AgentSyncStore::open_memory().unwrap();
        let session = agent_sync_core::SessionRecord {
            id: "codex:session-1".into(),
            agent_id: "codex".into(),
            title: Some("Session 1".into()),
            created_at: None,
            updated_at: None,
            source_project: None,
            storage_refs: vec![],
            visibility: agent_sync_core::SessionVisibility::Unknown,
            content_policy: agent_sync_core::ContentPolicy::MetadataOnly,
            import_capabilities: agent_sync_core::SessionImportCapabilities::default(),
        };
        let bundle = SyncBundle {
            manifest: agent_sync_bundle::SyncBundleManifest {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                source_snapshot: Uuid::new_v4(),
                selections: vec![],
                redactions: vec![],
                encryption: agent_sync_bundle::BundleEncryptionInfo {
                    required_for_sensitive_payloads: true,
                    method: "none".into(),
                },
            },
            source_snapshot: agent_sync_core::DeviceSnapshot {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                generated_at: Utc::now(),
                platform: agent_sync_core::PlatformInfo {
                    os: "test".into(),
                    arch: "test".into(),
                },
                inputs: agent_sync_core::SnapshotInputs {
                    home: "~".into(),
                    project: "/tmp/project".into(),
                    max_depth: 1,
                    max_entries: 10,
                },
                summary: agent_sync_core::SnapshotSummary::default(),
                projects: vec![],
                agents: vec![],
            },
            payloads: vec![],
            session_archives: vec![agent_sync_bundle::SessionArchiveEntry {
                agent_id: "codex".into(),
                agent_name: "Codex".into(),
                session,
                source_project: None,
                payload_included: false,
                payloads: vec![],
                import_note: "metadata-only".into(),
            }],
        };

        let journal = import_session_archives(
            &store,
            &bundle,
            &SessionArchiveImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_project: Some("/target/project".into()),
                target_project_id: None,
            },
        )
        .unwrap();

        assert_eq!(journal.imported, 1);
        assert_eq!(journal.skipped, 0);
        assert_eq!(
            journal.records[0].target_project.as_deref(),
            Some("/target/project")
        );
        assert_eq!(store.list("session_archive").unwrap().len(), 1);
    }

    #[test]
    fn stages_session_payloads_with_project_rewrite() {
        let root = std::env::temp_dir().join(format!("agent-sync-stage-{}", Uuid::new_v4()));
        let staging = root.join("staging");
        let source_project = "/Users/me/source-project";
        let target_project = "C:/Users/me/target-project";
        let bytes = format!("{{\"cwd\":\"{}\",\"text\":\"hello\"}}\n", source_project).into_bytes();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let session = agent_sync_core::SessionRecord {
            id: "codex:session-1".into(),
            agent_id: "codex".into(),
            title: Some("Session 1".into()),
            created_at: None,
            updated_at: None,
            source_project: None,
            storage_refs: vec![],
            visibility: agent_sync_core::SessionVisibility::Unknown,
            content_policy: agent_sync_core::ContentPolicy::ExplicitRawPayloadRequired,
            import_capabilities: agent_sync_core::SessionImportCapabilities::default(),
        };
        let bundle = SyncBundle {
            manifest: agent_sync_bundle::SyncBundleManifest {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                source_snapshot: Uuid::new_v4(),
                selections: vec![],
                redactions: vec![],
                encryption: agent_sync_bundle::BundleEncryptionInfo {
                    required_for_sensitive_payloads: true,
                    method: "none".into(),
                },
            },
            source_snapshot: agent_sync_core::DeviceSnapshot {
                schema_version: "0.2".into(),
                id: Uuid::new_v4(),
                generated_at: Utc::now(),
                platform: agent_sync_core::PlatformInfo {
                    os: "test".into(),
                    arch: "test".into(),
                },
                inputs: agent_sync_core::SnapshotInputs {
                    home: "~".into(),
                    project: source_project.into(),
                    max_depth: 1,
                    max_entries: 10,
                },
                summary: agent_sync_core::SnapshotSummary::default(),
                projects: vec![],
                agents: vec![],
            },
            payloads: vec![],
            session_archives: vec![agent_sync_bundle::SessionArchiveEntry {
                agent_id: "codex".into(),
                agent_name: "Codex".into(),
                session,
                source_project: Some(agent_sync_core::ProjectIdentity {
                    id: Uuid::new_v4(),
                    canonical_path: source_project.into(),
                    physical_path: Some(source_project.into()),
                    git_remote: None,
                    git_root_fingerprint: None,
                    package_name: None,
                    agent_project_keys: vec![],
                }),
                payload_included: true,
                payloads: vec![PayloadEntry {
                    agent_id: "codex".into(),
                    portable_path: "~/.codex/sessions/2026/07/02/rollout.jsonl".into(),
                    sha256: sha,
                    base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
                }],
                import_note: "raw payload included".into(),
            }],
        };

        let journal = stage_session_native_import(
            &bundle,
            &SessionNativeImportStageOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_project: Some(target_project.into()),
                staging_dir: staging.clone(),
                rewrite_project_identity: true,
            },
        )
        .unwrap();

        assert_eq!(journal.staged, 1);
        assert!(journal.records[0].written_payloads[0].project_identity_rewritten);
        let staged_text =
            fs::read_to_string(&journal.records[0].written_payloads[0].staged_path).unwrap();
        assert!(staged_text.contains(target_project));
        assert!(!staged_text.contains(source_project));
        let _ = fs::remove_dir_all(root);
    }
}
