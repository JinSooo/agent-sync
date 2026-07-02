use agent_sync_apply::{
    ApplyContext, ApplyPayloadOptions, NativeSessionProjectRemapApplyOptions,
    NativeSessionProjectRemapJournal, NativeSessionProjectRemapPreviewOptions,
    NativeSessionProjectRemapPreviewReport, NativeSessionProjectRemapSelection,
    NativeSessionStoreDiscoveryOptions, NativeSessionStoreDiscoveryReport, OperationJournal,
    PreflightReport, SessionArchiveImportJournal, SessionArchiveImportOptions,
    SessionNativeFileImportJournal, SessionNativeFileImportOptions,
    SessionNativeImportReadinessOptions, SessionNativeImportReadinessReport,
    SessionNativeImportStageJournal, SessionNativeImportStageOptions,
    apply_native_session_project_remap, apply_payloads, create_journal,
    discover_native_session_stores, import_session_archives,
    import_session_payloads_to_native_files, preflight, preview_native_session_project_remap,
    rollback_journal, rollback_native_session_project_remap_journal,
    rollback_session_native_file_import_journal, session_native_import_readiness,
    stage_session_native_import,
};
use agent_sync_bundle::{
    BundleDeviceKeySummary, BundleExportOptions, BundleFileDecryptionOptions,
    BundleFileEncryptionOptions, PayloadSelectionRef, SyncBundle, SyncBundleManifest,
    bundle_recipient_from_input, export_bundle, generate_bundle_device_key_file,
    manifest_from_snapshot, read_bundle_device_key_file, read_bundle_file_with_decryption,
    verify_bundle, write_bundle_file_with_encryption, write_bundle_recipient_file,
};
use agent_sync_core::DeviceSnapshot;
use agent_sync_scan::{ScanOptions, scan_device as scan_device_core};
use agent_sync_storage::{AgentSyncStore, StoredRecord};
use agent_sync_transform::{SnapshotDiff, TransformPlan, create_transform_plan, diff_snapshots};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[tauri::command]
fn scan_device(
    home: Option<String>,
    project: Option<String>,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
) -> Result<DeviceSnapshot, String> {
    let home = home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let project = match project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    let mut options = ScanOptions::new(home, project);
    if let Some(max_depth) = max_depth {
        options.max_depth = max_depth;
    }
    if let Some(max_entries) = max_entries {
        options.max_entries = max_entries;
    }
    scan_device_core(options).map_err(|error| error.to_string())
}

#[tauri::command]
fn diff_snapshots_command(
    from: DeviceSnapshot,
    to: DeviceSnapshot,
) -> Result<SnapshotDiff, String> {
    Ok(diff_snapshots(&from, &to))
}

#[tauri::command]
fn create_transform_plan_command(
    from: DeviceSnapshot,
    to: DeviceSnapshot,
    target_platform: Option<String>,
) -> Result<TransformPlan, String> {
    Ok(create_transform_plan(&from, &to, target_platform))
}

#[tauri::command]
fn create_bundle_manifest(snapshot: DeviceSnapshot) -> Result<SyncBundleManifest, String> {
    Ok(manifest_from_snapshot(&snapshot))
}

#[tauri::command]
fn generate_bundle_key_file(output: String) -> Result<BundleDeviceKeySummary, String> {
    generate_bundle_device_key_file(output)
        .map(|key| BundleDeviceKeySummary::from(&key))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_bundle_recipient_file(
    key_path: String,
    output: String,
) -> Result<BundleDeviceKeySummary, String> {
    let key = read_bundle_device_key_file(key_path).map_err(|error| error.to_string())?;
    let recipient = BundleDeviceKeySummary::from(&key);
    write_bundle_recipient_file(&recipient, output).map_err(|error| error.to_string())?;
    Ok(recipient)
}

#[tauri::command]
fn export_bundle_file(
    snapshot: DeviceSnapshot,
    home: Option<String>,
    project: Option<String>,
    output: String,
    max_payload_bytes: Option<u64>,
    selected_review_payloads: Option<Vec<PayloadSelectionRef>>,
    include_session_payloads: Option<bool>,
    selected_session_ids: Option<Vec<String>>,
    max_session_payload_bytes: Option<u64>,
    allow_unencrypted_sensitive_payloads: Option<bool>,
    encryption_passphrase: Option<String>,
    encryption_key_path: Option<String>,
    encryption_recipient_inputs: Option<Vec<String>>,
) -> Result<SyncBundleManifest, String> {
    let home = home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let project = match project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    let encryption = bundle_file_encryption_options(
        encryption_passphrase,
        encryption_key_path,
        encryption_recipient_inputs.unwrap_or_default(),
    )?;
    let bundle = export_bundle(
        &snapshot,
        &BundleExportOptions {
            home,
            project,
            max_payload_bytes: max_payload_bytes.unwrap_or(1024 * 1024),
            selected_review_payloads: selected_review_payloads.unwrap_or_default(),
            include_session_payloads: include_session_payloads.unwrap_or(false),
            selected_session_ids: selected_session_ids.unwrap_or_default(),
            max_session_payload_bytes: max_session_payload_bytes.unwrap_or(2 * 1024 * 1024),
            allow_unencrypted_sensitive_payloads: allow_unencrypted_sensitive_payloads
                .unwrap_or(false),
            encryption_passphrase: encryption.passphrase.clone(),
            encryption_recipients: encryption.recipients.clone(),
        },
    )
    .map_err(|error| error.to_string())?;
    write_bundle_file_with_encryption(&bundle, output, &encryption)
        .map_err(|error| error.to_string())?;
    Ok(bundle.manifest)
}

#[tauri::command]
fn read_bundle(
    path: String,
    encryption_passphrase: Option<String>,
    encryption_key_path: Option<String>,
) -> Result<SyncBundle, String> {
    let decryption = bundle_file_decryption_options(encryption_passphrase, encryption_key_path)?;
    read_bundle_file_with_decryption(path, &decryption).map_err(|error| error.to_string())
}

#[tauri::command]
fn verify_bundle_command(bundle: SyncBundle) -> Result<Vec<String>, String> {
    Ok(verify_bundle(&bundle))
}

#[tauri::command]
fn preflight_plan(plan: TransformPlan) -> Result<PreflightReport, String> {
    Ok(preflight(&plan))
}

#[tauri::command]
fn create_operation_journal(plan: TransformPlan) -> Result<OperationJournal, String> {
    Ok(create_journal(&plan))
}

#[tauri::command]
fn apply_safe_payloads_command(
    bundle: SyncBundle,
    plan: TransformPlan,
    target_home: Option<String>,
    target_project: Option<String>,
    backup_dir: String,
    acknowledge_review_required: Option<bool>,
) -> Result<OperationJournal, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let target_project = match target_project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    apply_payloads(
        &bundle,
        &plan,
        &ApplyContext {
            target_home,
            target_project,
            backup_dir: PathBuf::from(backup_dir),
        },
        &ApplyPayloadOptions {
            acknowledge_review_required: acknowledge_review_required.unwrap_or(false),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn rollback_journal_command(journal: OperationJournal) -> Result<OperationJournal, String> {
    rollback_journal(&journal).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_operation_journal(db_path: String, journal: OperationJournal) -> Result<String, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    store
        .save_json("apply_journal", Some(journal.id), &journal)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_session_native_file_import_journal(
    db_path: String,
    journal: SessionNativeFileImportJournal,
) -> Result<String, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    store
        .save_json(
            "session_native_file_import_journal",
            Some(journal.id),
            &journal,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_native_session_project_remap_journal(
    db_path: String,
    journal: NativeSessionProjectRemapJournal,
) -> Result<String, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    store
        .save_json(
            "native_session_project_remap_journal",
            Some(journal.id),
            &journal,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_session_archives_command(
    bundle: SyncBundle,
    db_path: String,
    selected_session_ids: Vec<String>,
    target_project: Option<String>,
    target_project_by_session: Option<BTreeMap<String, String>>,
) -> Result<SessionArchiveImportJournal, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    import_session_archives(
        &store,
        &bundle,
        &SessionArchiveImportOptions {
            selected_session_ids,
            target_project,
            target_project_by_session: target_project_by_session.unwrap_or_default(),
            target_project_id: None,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn stage_session_native_import_command(
    bundle: SyncBundle,
    selected_session_ids: Vec<String>,
    target_project: Option<String>,
    target_project_by_session: Option<BTreeMap<String, String>>,
    staging_dir: String,
    rewrite_project_identity: Option<bool>,
) -> Result<SessionNativeImportStageJournal, String> {
    stage_session_native_import(
        &bundle,
        &SessionNativeImportStageOptions {
            selected_session_ids,
            target_project,
            target_project_by_session: target_project_by_session.unwrap_or_default(),
            staging_dir: PathBuf::from(staging_dir),
            rewrite_project_identity: rewrite_project_identity.unwrap_or(true),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn session_native_import_readiness_command(
    bundle: SyncBundle,
    target_snapshot: Option<DeviceSnapshot>,
    selected_session_ids: Vec<String>,
    require_agents_stopped: Option<bool>,
) -> Result<SessionNativeImportReadinessReport, String> {
    Ok(session_native_import_readiness(
        &bundle,
        target_snapshot.as_ref(),
        &SessionNativeImportReadinessOptions {
            selected_session_ids,
            require_agents_stopped: require_agents_stopped.unwrap_or(true),
        },
    ))
}

#[tauri::command]
fn discover_native_session_stores_command(
    snapshot: DeviceSnapshot,
    target_home: Option<String>,
    target_project: Option<String>,
    max_schema_tables: Option<usize>,
) -> Result<NativeSessionStoreDiscoveryReport, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let target_project = match target_project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    Ok(discover_native_session_stores(
        &snapshot,
        &NativeSessionStoreDiscoveryOptions {
            target_home,
            target_project,
            max_schema_tables: max_schema_tables.unwrap_or(20),
        },
    ))
}

#[tauri::command]
fn preview_native_session_project_remap_command(
    snapshot: DeviceSnapshot,
    target_home: Option<String>,
    target_project: Option<String>,
    source_project: Option<String>,
    max_schema_tables: Option<usize>,
) -> Result<NativeSessionProjectRemapPreviewReport, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let target_project = match target_project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    Ok(preview_native_session_project_remap(
        &snapshot,
        &NativeSessionProjectRemapPreviewOptions {
            target_home,
            target_project,
            source_project,
            max_schema_tables: max_schema_tables.unwrap_or(20),
        },
    ))
}

#[tauri::command]
fn apply_native_session_project_remap_command(
    snapshot: DeviceSnapshot,
    target_home: Option<String>,
    target_project: Option<String>,
    source_project: String,
    backup_dir: String,
    selections: Vec<NativeSessionProjectRemapSelection>,
    require_agents_stopped: Option<bool>,
) -> Result<NativeSessionProjectRemapJournal, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let target_project = match target_project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    apply_native_session_project_remap(
        &snapshot,
        &NativeSessionProjectRemapApplyOptions {
            target_home,
            target_project,
            source_project,
            backup_dir: PathBuf::from(backup_dir),
            selections,
            require_agents_stopped: require_agents_stopped.unwrap_or(true),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn rollback_native_session_project_remap_journal_command(
    journal: NativeSessionProjectRemapJournal,
) -> Result<NativeSessionProjectRemapJournal, String> {
    rollback_native_session_project_remap_journal(&journal).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_session_payloads_to_native_files_command(
    bundle: SyncBundle,
    selected_session_ids: Vec<String>,
    target_home: Option<String>,
    target_project: Option<String>,
    target_project_by_session: Option<BTreeMap<String, String>>,
    backup_dir: String,
    rewrite_project_identity: Option<bool>,
    require_agents_stopped: Option<bool>,
) -> Result<SessionNativeFileImportJournal, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    import_session_payloads_to_native_files(
        &bundle,
        &SessionNativeFileImportOptions {
            selected_session_ids,
            target_home,
            target_project,
            target_project_by_session: target_project_by_session.unwrap_or_default(),
            backup_dir: PathBuf::from(backup_dir),
            rewrite_project_identity: rewrite_project_identity.unwrap_or(true),
            require_agents_stopped: require_agents_stopped.unwrap_or(true),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn rollback_session_native_file_import_journal_command(
    journal: SessionNativeFileImportJournal,
) -> Result<SessionNativeFileImportJournal, String> {
    rollback_session_native_file_import_journal(&journal).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_snapshot_to_store(db_path: String, snapshot: DeviceSnapshot) -> Result<String, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    store
        .save_json("snapshot", Some(snapshot.id), &snapshot)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_store_records(db_path: String, kind: String) -> Result<Vec<StoredRecord>, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    store.list(&kind).map_err(|error| error.to_string())
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn bundle_file_encryption_options(
    passphrase: Option<String>,
    key_path: Option<String>,
    recipient_inputs: Vec<String>,
) -> Result<BundleFileEncryptionOptions, String> {
    let passphrase = nonempty(passphrase);
    let key = nonempty(key_path)
        .map(read_bundle_device_key_file)
        .transpose()
        .map_err(|error| error.to_string())?;
    let mut recipients = Vec::new();
    if let Some(key) = key {
        recipients.push(key.age_recipient);
    }
    for input in recipient_inputs {
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        recipients.push(bundle_recipient_from_input(input).map_err(|error| error.to_string())?);
    }
    if passphrase.is_some() && !recipients.is_empty() {
        return Err(
            "Bundle passphrase is mutually exclusive with bundle key files or recipients"
                .to_string(),
        );
    }
    Ok(BundleFileEncryptionOptions {
        passphrase,
        recipients,
    })
}

fn bundle_file_decryption_options(
    passphrase: Option<String>,
    key_path: Option<String>,
) -> Result<BundleFileDecryptionOptions, String> {
    let passphrase = nonempty(passphrase);
    let key = nonempty(key_path)
        .map(read_bundle_device_key_file)
        .transpose()
        .map_err(|error| error.to_string())?;
    if passphrase.is_some() && key.is_some() {
        return Err("Bundle passphrase and bundle key file are mutually exclusive".to_string());
    }
    Ok(BundleFileDecryptionOptions {
        passphrase,
        identities: key.map(|key| vec![key.age_identity]).unwrap_or_default(),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_device,
            diff_snapshots_command,
            create_transform_plan_command,
            create_bundle_manifest,
            generate_bundle_key_file,
            export_bundle_recipient_file,
            export_bundle_file,
            read_bundle,
            verify_bundle_command,
            preflight_plan,
            create_operation_journal,
            apply_safe_payloads_command,
            rollback_journal_command,
            save_operation_journal,
            save_session_native_file_import_journal,
            save_native_session_project_remap_journal,
            import_session_archives_command,
            stage_session_native_import_command,
            session_native_import_readiness_command,
            discover_native_session_stores_command,
            preview_native_session_project_remap_command,
            apply_native_session_project_remap_command,
            rollback_native_session_project_remap_journal_command,
            import_session_payloads_to_native_files_command,
            rollback_session_native_file_import_journal_command,
            save_snapshot_to_store,
            list_store_records
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agent Sync Studio");
}
