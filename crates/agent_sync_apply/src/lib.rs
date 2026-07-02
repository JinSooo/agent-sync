use agent_sync_bundle::SyncBundle;
use agent_sync_core::SafetyClass;
use agent_sync_platform::{RunningAgentProcess, detect_running_agent_processes};
use agent_sync_storage::AgentSyncStore;
use agent_sync_transform::{ApplyOperationKind, TransformPlan};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
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
pub struct ApplyPayloadOptions {
    pub acknowledge_review_required: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeFileImportOptions {
    pub selected_session_ids: Vec<String>,
    pub target_home: PathBuf,
    pub target_project: Option<String>,
    pub backup_dir: PathBuf,
    pub rewrite_project_identity: bool,
    pub require_agents_stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeFileImportJournal {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub bundle_id: Uuid,
    pub source_snapshot: Uuid,
    pub selected: usize,
    pub imported: usize,
    pub skipped: usize,
    #[serde(default)]
    pub blockers: Vec<String>,
    pub records: Vec<SessionNativeFileImportRecord>,
    pub status: JournalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNativeFileImportRecord {
    pub agent_id: String,
    pub agent_name: String,
    pub session_id: String,
    pub title: Option<String>,
    pub source_project: Option<String>,
    pub target_project: Option<String>,
    pub written_payloads: Vec<NativeFilePayloadRecord>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeFilePayloadRecord {
    pub portable_path: String,
    pub target_path: String,
    pub backup_path: Option<String>,
    pub source_sha256: String,
    pub written_sha256: Option<String>,
    pub project_identity_rewritten: bool,
    pub status: JournalOperationStatus,
    pub message: Option<String>,
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

pub fn import_session_payloads_to_native_files(
    bundle: &SyncBundle,
    options: &SessionNativeFileImportOptions,
) -> std::io::Result<SessionNativeFileImportJournal> {
    let running_processes = if options.require_agents_stopped {
        match detect_running_agent_processes(&selected_agent_ids_with_payloads(bundle, options)) {
            Ok(processes) => processes,
            Err(error) => {
                return Ok(blocked_session_native_file_import_journal(
                    bundle,
                    options,
                    vec![format!(
                        "could not verify Codex/Claude stopped state: {error}; retry after stopping agents or use the explicit skip option"
                    )],
                ));
            }
        }
    } else {
        Vec::new()
    };

    import_session_payloads_to_native_files_with_processes(bundle, options, &running_processes)
}

pub fn import_session_payloads_to_native_files_with_processes(
    bundle: &SyncBundle,
    options: &SessionNativeFileImportOptions,
    running_processes: &[RunningAgentProcess],
) -> std::io::Result<SessionNativeFileImportJournal> {
    fs::create_dir_all(&options.backup_dir)?;
    let blockers = running_agent_blockers(bundle, options, running_processes);
    if !blockers.is_empty() && options.require_agents_stopped {
        return Ok(blocked_session_native_file_import_journal(
            bundle, options, blockers,
        ));
    }

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

        let mut written_payloads = Vec::new();
        for payload in &archive.payloads {
            let Some(target_path_result) = native_session_target_path(
                archive.agent_id.as_str(),
                payload.portable_path.as_str(),
                &options.target_home,
            ) else {
                written_payloads.push(NativeFilePayloadRecord {
                    portable_path: payload.portable_path.clone(),
                    target_path: String::new(),
                    backup_path: None,
                    source_sha256: payload.sha256.clone(),
                    written_sha256: None,
                    project_identity_rewritten: false,
                    status: JournalOperationStatus::Blocked,
                    message: Some(
                        "native file import only allows Codex ~/.codex/** or Claude ~/.claude/** session payload paths"
                            .to_string(),
                    ),
                });
                continue;
            };
            let target_path = target_path_result?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&payload.base64_content)
                .map_err(std::io::Error::other)?;
            let source_sha = format!("{:x}", Sha256::digest(&bytes));
            if source_sha != payload.sha256 {
                written_payloads.push(NativeFilePayloadRecord {
                    portable_path: payload.portable_path.clone(),
                    target_path: target_path.display().to_string(),
                    backup_path: None,
                    source_sha256: payload.sha256.clone(),
                    written_sha256: None,
                    project_identity_rewritten: false,
                    status: JournalOperationStatus::Failed,
                    message: Some("payload checksum mismatch".to_string()),
                });
                continue;
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
            let written_sha = format!("{:x}", Sha256::digest(&next_bytes));

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let backup_path = if target_path.exists() {
                let backup = options.backup_dir.join(format!(
                    "{}-{}",
                    Uuid::new_v4(),
                    sanitize_path(&payload.portable_path)
                ));
                fs::copy(&target_path, &backup)?;
                Some(backup)
            } else {
                None
            };

            fs::write(&target_path, &next_bytes)?;
            let after = fs::read(&target_path)?;
            let after_sha = format!("{:x}", Sha256::digest(&after));
            let (status, message) = if after_sha == written_sha {
                (
                    JournalOperationStatus::Verified,
                    Some("native file written and checksum verified".to_string()),
                )
            } else {
                (
                    JournalOperationStatus::Failed,
                    Some("written checksum mismatch".to_string()),
                )
            };

            written_payloads.push(NativeFilePayloadRecord {
                portable_path: payload.portable_path.clone(),
                target_path: target_path.display().to_string(),
                backup_path: backup_path.map(|path| path.display().to_string()),
                source_sha256: payload.sha256.clone(),
                written_sha256: Some(after_sha),
                project_identity_rewritten: rewritten,
                status,
                message,
            });
        }

        records.push(SessionNativeFileImportRecord {
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
            note: "native file import only; Codex/Claude databases and secondary indexes are not modified"
                .to_string(),
        });
    }

    let imported = records
        .iter()
        .filter(|record| {
            record
                .written_payloads
                .iter()
                .any(|payload| payload.status == JournalOperationStatus::Verified)
        })
        .count();
    let failed = records.iter().any(|record| {
        record
            .written_payloads
            .iter()
            .any(|payload| payload.status == JournalOperationStatus::Failed)
    });

    Ok(SessionNativeFileImportJournal {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        bundle_id: bundle.manifest.id,
        source_snapshot: bundle.source_snapshot.id,
        selected,
        imported,
        skipped: selected.saturating_sub(imported),
        blockers: Vec::new(),
        records,
        status: if failed {
            JournalStatus::Failed
        } else {
            JournalStatus::Completed
        },
    })
}

fn selected_agent_ids_with_payloads(
    bundle: &SyncBundle,
    options: &SessionNativeFileImportOptions,
) -> Vec<String> {
    let mut agent_ids = BTreeSet::new();
    for archive in &bundle.session_archives {
        if archive.payloads.is_empty() {
            continue;
        }
        if options
            .selected_session_ids
            .iter()
            .any(|id| id == &archive.session.id)
        {
            agent_ids.insert(archive.agent_id.clone());
        }
    }
    agent_ids.into_iter().collect()
}

fn running_agent_blockers(
    bundle: &SyncBundle,
    options: &SessionNativeFileImportOptions,
    running_processes: &[RunningAgentProcess],
) -> Vec<String> {
    let selected_agents = selected_agent_ids_with_payloads(bundle, options)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut blockers = Vec::new();
    for process in running_processes {
        if selected_agents.contains(&process.agent_id) {
            blockers.push(format!(
                "{} appears to be running (pid {}, executable {}); stop it before native session import or use the explicit skip option",
                process.agent_id, process.pid, process.executable
            ));
        }
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

fn blocked_session_native_file_import_journal(
    bundle: &SyncBundle,
    options: &SessionNativeFileImportOptions,
    blockers: Vec<String>,
) -> SessionNativeFileImportJournal {
    let mut records = Vec::new();
    for archive in &bundle.session_archives {
        if !options
            .selected_session_ids
            .iter()
            .any(|id| id == &archive.session.id)
            || archive.payloads.is_empty()
        {
            continue;
        }

        let written_payloads = archive
            .payloads
            .iter()
            .map(|payload| {
                let target_path = native_session_target_path(
                    archive.agent_id.as_str(),
                    payload.portable_path.as_str(),
                    &options.target_home,
                )
                .and_then(|result| result.ok())
                .map(|path| path.display().to_string())
                .unwrap_or_default();
                NativeFilePayloadRecord {
                    portable_path: payload.portable_path.clone(),
                    target_path,
                    backup_path: None,
                    source_sha256: payload.sha256.clone(),
                    written_sha256: None,
                    project_identity_rewritten: false,
                    status: JournalOperationStatus::Blocked,
                    message: Some(blockers.join(" / ")),
                }
            })
            .collect();

        records.push(SessionNativeFileImportRecord {
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
            note: "blocked before writing because native session import requires target agents to be stopped"
                .to_string(),
        });
    }

    SessionNativeFileImportJournal {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        bundle_id: bundle.manifest.id,
        source_snapshot: bundle.source_snapshot.id,
        selected: options.selected_session_ids.len(),
        imported: 0,
        skipped: options.selected_session_ids.len(),
        blockers,
        records,
        status: JournalStatus::Failed,
    }
}

fn native_session_target_path(
    agent_id: &str,
    portable_path: &str,
    target_home: &Path,
) -> Option<std::io::Result<PathBuf>> {
    let allowed_prefix = match agent_id {
        "codex" => "~/.codex/",
        "claude" => "~/.claude/",
        _ => return None,
    };
    let rest = portable_path.strip_prefix(allowed_prefix)?;
    if !is_safe_portable_relative_path(rest) {
        return Some(Err(std::io::Error::other(format!(
            "unsafe native session payload path: {}",
            portable_path
        ))));
    }
    Some(Ok(target_home
        .join(allowed_prefix.trim_start_matches("~/"))
        .join(rest)))
}

fn is_safe_portable_relative_path(rest: &str) -> bool {
    if rest.is_empty()
        || rest.starts_with('/')
        || rest.starts_with('\\')
        || rest.contains('\0')
        || rest.contains('\\')
    {
        return false;
    }
    rest.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.contains(':')
            && !segment.contains('\0')
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
    apply_payloads(
        bundle,
        plan,
        context,
        &ApplyPayloadOptions {
            acknowledge_review_required: false,
        },
    )
}

pub fn apply_payloads(
    bundle: &SyncBundle,
    plan: &TransformPlan,
    context: &ApplyContext,
    options: &ApplyPayloadOptions,
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
        if !can_apply_payload_operation(&entry.operation, options) {
            entry.status = JournalOperationStatus::Blocked;
            entry.message = Some(
                "operation requires explicit review approval or adapter-specific apply".to_string(),
            );
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
        } else {
            let backup = context.backup_dir.join(format!(
                "{}-{}-absent",
                Uuid::new_v4(),
                sanitize_path(&entry.operation.path)
            ));
            fs::write(&backup, b"target did not exist before apply")?;
            entry.backup = Some(BackupRef {
                id: Uuid::new_v4(),
                original_path: target.display().to_string(),
                backup_path: backup.display().to_string(),
                checksum: None,
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

pub fn rollback_journal(journal: &OperationJournal) -> std::io::Result<OperationJournal> {
    let mut rolled_back = journal.clone();
    rolled_back.status = JournalStatus::RolledBack;

    for entry in &mut rolled_back.operations {
        let Some(backup) = &entry.backup else {
            entry.status = JournalOperationStatus::Blocked;
            entry.message = Some("no backup reference available for rollback".to_string());
            continue;
        };

        let original_path = PathBuf::from(&backup.original_path);
        match backup.checksum.as_deref() {
            Some(expected_sha) => {
                if let Some(parent) = original_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&backup.backup_path, &original_path)?;
                let restored = fs::read(&original_path)?;
                let restored_sha = format!("{:x}", Sha256::digest(&restored));
                if restored_sha == expected_sha {
                    entry.status = JournalOperationStatus::RolledBack;
                    entry.message = Some("restored from backup and checksum verified".to_string());
                } else {
                    entry.status = JournalOperationStatus::Failed;
                    entry.message = Some("restored checksum mismatch".to_string());
                    rolled_back.status = JournalStatus::Failed;
                }
            }
            None => {
                if original_path.exists() {
                    fs::remove_file(&original_path)?;
                }
                entry.status = JournalOperationStatus::RolledBack;
                entry.message = Some("removed file that did not exist before apply".to_string());
            }
        }
    }

    if rolled_back
        .operations
        .iter()
        .any(|entry| matches!(entry.status, JournalOperationStatus::Failed))
    {
        rolled_back.status = JournalStatus::Failed;
    } else if rolled_back
        .operations
        .iter()
        .any(|entry| matches!(entry.status, JournalOperationStatus::Blocked))
    {
        rolled_back.status = JournalStatus::Failed;
    }

    Ok(rolled_back)
}

pub fn rollback_session_native_file_import_journal(
    journal: &SessionNativeFileImportJournal,
) -> std::io::Result<SessionNativeFileImportJournal> {
    let mut rolled_back = journal.clone();
    rolled_back.status = JournalStatus::RolledBack;

    for record in &mut rolled_back.records {
        record.note = "native file import rollback attempted".to_string();
        for payload in &mut record.written_payloads {
            if payload.status != JournalOperationStatus::Verified {
                continue;
            }
            if payload.target_path.is_empty() {
                payload.status = JournalOperationStatus::Failed;
                payload.message = Some("no target path available for rollback".to_string());
                rolled_back.status = JournalStatus::Failed;
                continue;
            }

            let target_path = PathBuf::from(&payload.target_path);
            match payload.backup_path.as_deref() {
                Some(backup_path) => {
                    let backup_path = PathBuf::from(backup_path);
                    let backup_bytes = fs::read(&backup_path)?;
                    let backup_sha = format!("{:x}", Sha256::digest(&backup_bytes));
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(&backup_path, &target_path)?;
                    let restored = fs::read(&target_path)?;
                    let restored_sha = format!("{:x}", Sha256::digest(&restored));
                    if restored_sha == backup_sha {
                        payload.status = JournalOperationStatus::RolledBack;
                        payload.written_sha256 = Some(restored_sha);
                        payload.message = Some(
                            "restored native session file from backup and checksum verified"
                                .to_string(),
                        );
                    } else {
                        payload.status = JournalOperationStatus::Failed;
                        payload.message =
                            Some("restored native session checksum mismatch".to_string());
                        rolled_back.status = JournalStatus::Failed;
                    }
                }
                None => {
                    if target_path.exists() {
                        let current = fs::read(&target_path)?;
                        let current_sha = format!("{:x}", Sha256::digest(&current));
                        if payload
                            .written_sha256
                            .as_deref()
                            .is_some_and(|expected| expected != current_sha)
                        {
                            payload.status = JournalOperationStatus::Failed;
                            payload.message = Some(
                                "target native session file changed after import; rollback refused to delete it"
                                    .to_string(),
                            );
                            rolled_back.status = JournalStatus::Failed;
                            continue;
                        }
                        fs::remove_file(&target_path)?;
                    }
                    payload.status = JournalOperationStatus::RolledBack;
                    payload.written_sha256 = None;
                    payload.message = Some(
                        "removed native session file that did not exist before import".to_string(),
                    );
                }
            }
        }
    }

    if rolled_back.records.iter().any(|record| {
        record
            .written_payloads
            .iter()
            .any(|payload| payload.status == JournalOperationStatus::Failed)
    }) {
        rolled_back.status = JournalStatus::Failed;
    }

    Ok(rolled_back)
}

fn can_apply_payload_operation(
    operation: &agent_sync_transform::ApplyOperation,
    options: &ApplyPayloadOptions,
) -> bool {
    if !operation.requires_backup {
        return false;
    }
    if !matches!(
        operation.kind,
        ApplyOperationKind::MergeText
            | ApplyOperationKind::CopyFile
            | ApplyOperationKind::ImportMemory
            | ApplyOperationKind::InstallTool
    ) {
        return false;
    }
    if !operation.requires_review {
        return operation.safety_class == SafetyClass::SafeConfig;
    }
    options.acknowledge_review_required
        && matches!(
            operation.safety_class,
            SafetyClass::MemoryKnowledge | SafetyClass::McpConfig
        )
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
    fn rollback_restores_backed_up_file() {
        let root = std::env::temp_dir().join(format!("agent-sync-rollback-{}", Uuid::new_v4()));
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
            warnings: vec![],
            summary: TransformSummary::default(),
        };
        let journal = apply_safe_payloads(
            &bundle,
            &plan,
            &ApplyContext {
                target_home: root.join("home"),
                target_project: project.clone(),
                backup_dir: backup,
            },
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "new"
        );

        let rollback = rollback_journal(&journal).unwrap();
        assert_eq!(rollback.status, JournalStatus::RolledBack);
        assert_eq!(
            rollback.operations[0].status,
            JournalOperationStatus::RolledBack
        );
        assert_eq!(
            fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "old"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_removes_file_created_by_apply() {
        let root = std::env::temp_dir().join(format!("agent-sync-rollback-new-{}", Uuid::new_v4()));
        let project = root.join("project");
        let backup = root.join("backup");
        fs::create_dir_all(&project).unwrap();
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
                portable_path: "<project>/NEW.md".into(),
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
                path: "<project>/NEW.md".into(),
                kind: ApplyOperationKind::CopyFile,
                safety_class: SafetyClass::SafeConfig,
                risk: "low-medium".into(),
                rationale: "test".into(),
                change_type: ChangeType::MissingOnTarget,
                path_warnings: vec![],
                requires_review: false,
                requires_backup: true,
            }],
            project_mappings: vec![],
            blocked: vec![],
            warnings: vec![],
            summary: TransformSummary::default(),
        };
        let journal = apply_safe_payloads(
            &bundle,
            &plan,
            &ApplyContext {
                target_home: root.join("home"),
                target_project: project.clone(),
                backup_dir: backup,
            },
        )
        .unwrap();
        assert!(project.join("NEW.md").exists());
        assert!(
            journal.operations[0]
                .backup
                .as_ref()
                .is_some_and(|backup| backup.checksum.is_none())
        );

        let rollback = rollback_journal(&journal).unwrap();
        assert_eq!(rollback.status, JournalStatus::RolledBack);
        assert!(!project.join("NEW.md").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn applies_review_payload_only_with_explicit_acknowledgement() {
        let root = std::env::temp_dir().join(format!("agent-sync-review-apply-{}", Uuid::new_v4()));
        let home = root.join("home");
        let project = root.join("project");
        let backup = root.join("backup");
        let target = home.join(".codex").join("memories").join("guide.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(&target, "old memory").unwrap();
        let bytes = b"new memory".to_vec();
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
                portable_path: "~/.codex/memories/guide.md".into(),
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
                path: "~/.codex/memories/guide.md".into(),
                kind: ApplyOperationKind::ImportMemory,
                safety_class: SafetyClass::MemoryKnowledge,
                risk: "medium-high".into(),
                rationale: "test".into(),
                change_type: ChangeType::ChangedBetweenSnapshots,
                path_warnings: vec![],
                requires_review: true,
                requires_backup: true,
            }],
            project_mappings: vec![],
            blocked: vec![],
            warnings: vec![],
            summary: TransformSummary::default(),
        };
        let context = ApplyContext {
            target_home: home.clone(),
            target_project: project,
            backup_dir: backup.clone(),
        };

        let blocked = apply_safe_payloads(&bundle, &plan, &context).unwrap();
        assert_eq!(blocked.status, JournalStatus::Completed);
        assert_eq!(
            blocked.operations[0].status,
            JournalOperationStatus::Blocked
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "old memory");

        let applied = apply_payloads(
            &bundle,
            &plan,
            &context,
            &ApplyPayloadOptions {
                acknowledge_review_required: true,
            },
        )
        .unwrap();
        assert_eq!(applied.status, JournalStatus::Completed);
        assert_eq!(
            applied.operations[0].status,
            JournalOperationStatus::Verified
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new memory");
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

    #[test]
    fn imports_session_payloads_to_native_files_with_backup() {
        let root = std::env::temp_dir().join(format!("agent-sync-native-{}", Uuid::new_v4()));
        let target_home = root.join("target-home");
        let backup = root.join("backup");
        let source_project = "/Users/me/source-project";
        let target_project = "C:/Users/me/target-project";
        let target_file = target_home
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("07")
            .join("02")
            .join("rollout.jsonl");
        fs::create_dir_all(target_file.parent().unwrap()).unwrap();
        fs::write(&target_file, "old native session").unwrap();

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

        let journal = import_session_payloads_to_native_files(
            &bundle,
            &SessionNativeFileImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_home: target_home.clone(),
                target_project: Some(target_project.into()),
                backup_dir: backup.clone(),
                rewrite_project_identity: true,
                require_agents_stopped: false,
            },
        )
        .unwrap();

        assert_eq!(journal.status, JournalStatus::Completed);
        assert_eq!(journal.imported, 1);
        assert_eq!(
            journal.records[0].written_payloads[0].status,
            JournalOperationStatus::Verified
        );
        assert!(journal.records[0].written_payloads[0].backup_path.is_some());
        let written_text = fs::read_to_string(&target_file).unwrap();
        assert!(written_text.contains(target_project));
        assert!(!written_text.contains(source_project));
        assert_eq!(
            fs::read_to_string(backup.read_dir().unwrap().next().unwrap().unwrap().path()).unwrap(),
            "old native session"
        );

        let rollback = rollback_session_native_file_import_journal(&journal).unwrap();
        assert_eq!(rollback.status, JournalStatus::RolledBack);
        assert_eq!(
            rollback.records[0].written_payloads[0].status,
            JournalOperationStatus::RolledBack
        );
        assert_eq!(
            fs::read_to_string(target_file).unwrap(),
            "old native session"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn blocks_native_session_import_outside_allowed_agent_root() {
        let root = std::env::temp_dir().join(format!("agent-sync-native-block-{}", Uuid::new_v4()));
        let target_home = root.join("target-home");
        let backup = root.join("backup");
        let bytes = b"hello".to_vec();
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
                payload_included: true,
                payloads: vec![PayloadEntry {
                    agent_id: "codex".into(),
                    portable_path: "<project>/not-a-native-session.jsonl".into(),
                    sha256: sha,
                    base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
                }],
                import_note: "raw payload included".into(),
            }],
        };

        let journal = import_session_payloads_to_native_files(
            &bundle,
            &SessionNativeFileImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_home,
                target_project: None,
                backup_dir: backup,
                rewrite_project_identity: true,
                require_agents_stopped: false,
            },
        )
        .unwrap();

        assert_eq!(journal.status, JournalStatus::Completed);
        assert_eq!(journal.imported, 0);
        assert_eq!(
            journal.records[0].written_payloads[0].status,
            JournalOperationStatus::Blocked
        );
        assert!(!root.join("target-home").join("project").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_native_session_import_removes_created_file() {
        let root = std::env::temp_dir().join(format!(
            "agent-sync-native-created-rollback-{}",
            Uuid::new_v4()
        ));
        let target_home = root.join("target-home");
        let backup = root.join("backup");
        let target_file = target_home
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("07")
            .join("02")
            .join("rollout.jsonl");

        let bundle = codex_session_payload_bundle(b"new native session".to_vec());
        let journal = import_session_payloads_to_native_files(
            &bundle,
            &SessionNativeFileImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_home: target_home.clone(),
                target_project: None,
                backup_dir: backup,
                rewrite_project_identity: true,
                require_agents_stopped: false,
            },
        )
        .unwrap();
        assert_eq!(journal.status, JournalStatus::Completed);
        assert!(target_file.exists());
        assert!(journal.records[0].written_payloads[0].backup_path.is_none());

        let rollback = rollback_session_native_file_import_journal(&journal).unwrap();
        assert_eq!(rollback.status, JournalStatus::RolledBack);
        assert_eq!(
            rollback.records[0].written_payloads[0].status,
            JournalOperationStatus::RolledBack
        );
        assert!(!target_file.exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn blocks_native_session_import_when_agent_process_is_running() {
        let root =
            std::env::temp_dir().join(format!("agent-sync-native-running-{}", Uuid::new_v4()));
        let target_home = root.join("target-home");
        let backup = root.join("backup");
        let target_file = target_home
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("07")
            .join("02")
            .join("rollout.jsonl");
        fs::create_dir_all(target_file.parent().unwrap()).unwrap();
        fs::write(&target_file, "old native session").unwrap();

        let bundle = codex_session_payload_bundle(b"new native session".to_vec());
        let journal = import_session_payloads_to_native_files_with_processes(
            &bundle,
            &SessionNativeFileImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_home: target_home.clone(),
                target_project: None,
                backup_dir: backup,
                rewrite_project_identity: true,
                require_agents_stopped: true,
            },
            &[RunningAgentProcess {
                agent_id: "codex".into(),
                pid: 1234,
                executable: "codex".into(),
                command: "codex".into(),
            }],
        )
        .unwrap();

        assert_eq!(journal.status, JournalStatus::Failed);
        assert_eq!(journal.imported, 0);
        assert_eq!(journal.skipped, 1);
        assert_eq!(journal.blockers.len(), 1);
        assert_eq!(
            journal.records[0].written_payloads[0].status,
            JournalOperationStatus::Blocked
        );
        assert_eq!(
            fs::read_to_string(target_file).unwrap(),
            "old native session"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn allows_native_session_import_when_agent_process_check_is_disabled() {
        let root =
            std::env::temp_dir().join(format!("agent-sync-native-skip-check-{}", Uuid::new_v4()));
        let target_home = root.join("target-home");
        let backup = root.join("backup");
        let target_file = target_home
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("07")
            .join("02")
            .join("rollout.jsonl");

        let bundle = codex_session_payload_bundle(b"new native session".to_vec());
        let journal = import_session_payloads_to_native_files_with_processes(
            &bundle,
            &SessionNativeFileImportOptions {
                selected_session_ids: vec!["codex:session-1".into()],
                target_home: target_home.clone(),
                target_project: None,
                backup_dir: backup,
                rewrite_project_identity: true,
                require_agents_stopped: false,
            },
            &[RunningAgentProcess {
                agent_id: "codex".into(),
                pid: 1234,
                executable: "codex".into(),
                command: "codex".into(),
            }],
        )
        .unwrap();

        assert_eq!(journal.status, JournalStatus::Completed);
        assert_eq!(journal.imported, 1);
        assert_eq!(journal.blockers.len(), 0);
        assert_eq!(
            fs::read_to_string(target_file).unwrap(),
            "new native session"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn codex_session_payload_bundle(bytes: Vec<u8>) -> SyncBundle {
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
        SyncBundle {
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
                payload_included: true,
                payloads: vec![PayloadEntry {
                    agent_id: "codex".into(),
                    portable_path: "~/.codex/sessions/2026/07/02/rollout.jsonl".into(),
                    sha256: sha,
                    base64_content: base64::engine::general_purpose::STANDARD.encode(bytes),
                }],
                import_note: "raw payload included".into(),
            }],
        }
    }
}
