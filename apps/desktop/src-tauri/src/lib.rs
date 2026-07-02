use agent_sync_apply::{
    ApplyContext, OperationJournal, PreflightReport, SessionArchiveImportJournal,
    SessionArchiveImportOptions, SessionNativeFileImportJournal, SessionNativeFileImportOptions,
    SessionNativeImportStageJournal, SessionNativeImportStageOptions, apply_safe_payloads,
    create_journal, import_session_archives, import_session_payloads_to_native_files, preflight,
    stage_session_native_import,
};
use agent_sync_bundle::{
    BundleExportOptions, SyncBundle, SyncBundleManifest, export_bundle, manifest_from_snapshot,
    read_bundle_file, verify_bundle, write_bundle_file,
};
use agent_sync_core::DeviceSnapshot;
use agent_sync_scan::{ScanOptions, scan_device as scan_device_core};
use agent_sync_storage::{AgentSyncStore, StoredRecord};
use agent_sync_transform::{SnapshotDiff, TransformPlan, create_transform_plan, diff_snapshots};
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
fn export_bundle_file(
    snapshot: DeviceSnapshot,
    home: Option<String>,
    project: Option<String>,
    output: String,
    max_payload_bytes: Option<u64>,
    include_session_payloads: Option<bool>,
    selected_session_ids: Option<Vec<String>>,
    max_session_payload_bytes: Option<u64>,
) -> Result<SyncBundleManifest, String> {
    let home = home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let project = match project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    let bundle = export_bundle(
        &snapshot,
        &BundleExportOptions {
            home,
            project,
            max_payload_bytes: max_payload_bytes.unwrap_or(1024 * 1024),
            include_session_payloads: include_session_payloads.unwrap_or(false),
            selected_session_ids: selected_session_ids.unwrap_or_default(),
            max_session_payload_bytes: max_session_payload_bytes.unwrap_or(2 * 1024 * 1024),
        },
    )
    .map_err(|error| error.to_string())?;
    write_bundle_file(&bundle, output).map_err(|error| error.to_string())?;
    Ok(bundle.manifest)
}

#[tauri::command]
fn read_bundle(path: String) -> Result<SyncBundle, String> {
    read_bundle_file(path).map_err(|error| error.to_string())
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
) -> Result<OperationJournal, String> {
    let target_home = target_home
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let target_project = match target_project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    apply_safe_payloads(
        &bundle,
        &plan,
        &ApplyContext {
            target_home,
            target_project,
            backup_dir: PathBuf::from(backup_dir),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_session_archives_command(
    bundle: SyncBundle,
    db_path: String,
    selected_session_ids: Vec<String>,
    target_project: Option<String>,
) -> Result<SessionArchiveImportJournal, String> {
    let store = AgentSyncStore::open(db_path).map_err(|error| error.to_string())?;
    import_session_archives(
        &store,
        &bundle,
        &SessionArchiveImportOptions {
            selected_session_ids,
            target_project,
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
    staging_dir: String,
    rewrite_project_identity: Option<bool>,
) -> Result<SessionNativeImportStageJournal, String> {
    stage_session_native_import(
        &bundle,
        &SessionNativeImportStageOptions {
            selected_session_ids,
            target_project,
            staging_dir: PathBuf::from(staging_dir),
            rewrite_project_identity: rewrite_project_identity.unwrap_or(true),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_session_payloads_to_native_files_command(
    bundle: SyncBundle,
    selected_session_ids: Vec<String>,
    target_home: Option<String>,
    target_project: Option<String>,
    backup_dir: String,
    rewrite_project_identity: Option<bool>,
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
            backup_dir: PathBuf::from(backup_dir),
            rewrite_project_identity: rewrite_project_identity.unwrap_or(true),
        },
    )
    .map_err(|error| error.to_string())
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_device,
            diff_snapshots_command,
            create_transform_plan_command,
            create_bundle_manifest,
            export_bundle_file,
            read_bundle,
            verify_bundle_command,
            preflight_plan,
            create_operation_journal,
            apply_safe_payloads_command,
            import_session_archives_command,
            stage_session_native_import_command,
            import_session_payloads_to_native_files_command,
            save_snapshot_to_store,
            list_store_records
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agent Sync Studio");
}
