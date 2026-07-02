use agent_sync_apply::{
    NativeSessionProjectRemapApplyOptions, NativeSessionProjectRemapJournal,
    NativeSessionProjectRemapPreviewOptions, NativeSessionProjectRemapSelection,
    NativeSessionStoreDiscoveryOptions, OperationJournal, SessionNativeFileImportJournal,
    SessionNativeFileImportOptions, SessionNativeImportReadinessOptions,
    apply_native_session_project_remap, create_journal, discover_native_session_stores,
    import_session_payloads_to_native_files, preflight, preview_native_session_project_remap,
    rollback_journal, rollback_native_session_project_remap_journal,
    rollback_session_native_file_import_journal, session_native_import_readiness,
};
use agent_sync_bundle::{
    BundleExportOptions, PayloadSelectionRef, export_bundle, manifest_from_snapshot,
    read_bundle_file_with_passphrase, verify_bundle, write_bundle_file_with_passphrase,
};
use agent_sync_scan::{ScanOptions, scan_device};
use agent_sync_transform::create_transform_plan;
use std::env;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let command = if args.is_empty() {
        "scan".to_string()
    } else {
        args.remove(0)
    };

    match command.as_str() {
        "scan" => {
            let snapshot = scan_device(default_scan_options(&args))?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        "bundle-manifest" => {
            let snapshot = scan_device(default_scan_options(&args))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&manifest_from_snapshot(&snapshot))?
            );
        }
        "export-bundle" => {
            let output =
                value_after(&args, "--output").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let passphrase = bundle_passphrase(&args);
            let options = default_scan_options(&args);
            let snapshot = scan_device(options.clone())?;
            let selected_session_ids = values_after(&args, "--session");
            let include_session_payloads = args.iter().any(|arg| {
                arg == "--include-session-payloads" || arg == "--include-all-session-payloads"
            });
            let bundle = export_bundle(
                &snapshot,
                &BundleExportOptions {
                    home: options.home,
                    project: options.project,
                    max_payload_bytes: 1024 * 1024,
                    selected_review_payloads: payload_selection_values(&args, "--payload"),
                    include_session_payloads,
                    selected_session_ids,
                    max_session_payload_bytes: 2 * 1024 * 1024,
                    allow_unencrypted_sensitive_payloads: args
                        .iter()
                        .any(|arg| arg == "--allow-unencrypted-sensitive-payloads"),
                    encryption_passphrase: passphrase.clone(),
                },
            )?;
            write_bundle_file_with_passphrase(&bundle, &output, passphrase.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&bundle.manifest)?);
        }
        "verify-bundle" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let passphrase = bundle_passphrase(&args);
            let bundle = read_bundle_file_with_passphrase(input, passphrase.as_deref())?;
            let errors = verify_bundle(&bundle);
            println!("{}", serde_json::to_string_pretty(&errors)?);
            if !errors.is_empty() {
                std::process::exit(1);
            }
        }
        "check-native-sessions" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let passphrase = bundle_passphrase(&args);
            let bundle = read_bundle_file_with_passphrase(input, passphrase.as_deref())?;
            let selected_session_ids = selected_session_ids_or_all(&bundle, &args);
            let target_snapshot = if args.iter().any(|arg| arg == "--no-target-scan") {
                None
            } else {
                Some(scan_device(default_scan_options(&args))?)
            };
            let report = session_native_import_readiness(
                &bundle,
                target_snapshot.as_ref(),
                &SessionNativeImportReadinessOptions {
                    selected_session_ids,
                    require_agents_stopped: !args
                        .iter()
                        .any(|arg| arg == "--skip-agent-stopped-check"),
                },
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "discover-native-stores" => {
            let options = default_scan_options(&args);
            let snapshot = scan_device(options.clone())?;
            let report = discover_native_session_stores(
                &snapshot,
                &NativeSessionStoreDiscoveryOptions {
                    target_home: options.home,
                    target_project: options.project,
                    max_schema_tables: value_after(&args, "--max-schema-tables")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(20),
                },
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "preview-native-remap" => {
            let options = default_scan_options(&args);
            let snapshot = scan_device(options.clone())?;
            let report = preview_native_session_project_remap(
                &snapshot,
                &NativeSessionProjectRemapPreviewOptions {
                    target_home: options.home,
                    target_project: options.project,
                    source_project: value_after(&args, "--source-project"),
                    max_schema_tables: value_after(&args, "--max-schema-tables")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(20),
                },
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "apply-native-remap" => {
            let options = default_scan_options(&args);
            let snapshot = scan_device(options.clone())?;
            let source_project = value_after(&args, "--source-project").unwrap_or_default();
            let backup_dir = value_after(&args, "--backup-dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("agent-sync-db-remap-backups"));
            let journal = apply_native_session_project_remap(
                &snapshot,
                &NativeSessionProjectRemapApplyOptions {
                    target_home: options.home,
                    target_project: options.project,
                    source_project,
                    backup_dir,
                    selections: remap_selection_values(&args),
                    require_agents_stopped: !args
                        .iter()
                        .any(|arg| arg == "--skip-agent-stopped-check"),
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&journal)?);
        }
        "import-native-sessions" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let passphrase = bundle_passphrase(&args);
            let target_home = value_after(&args, "--target-home")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            let target_project = value_after(&args, "--target-project");
            let backup_dir = value_after(&args, "--backup-dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("agent-sync-backups"));
            let selected_session_ids = values_after(&args, "--session");
            let bundle = read_bundle_file_with_passphrase(input, passphrase.as_deref())?;
            let journal = import_session_payloads_to_native_files(
                &bundle,
                &SessionNativeFileImportOptions {
                    selected_session_ids,
                    target_home,
                    target_project,
                    backup_dir,
                    rewrite_project_identity: !args
                        .iter()
                        .any(|arg| arg == "--no-rewrite-project-identity"),
                    require_agents_stopped: !args
                        .iter()
                        .any(|arg| arg == "--skip-agent-stopped-check"),
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&journal)?);
        }
        "rollback-journal" => {
            let input = value_after(&args, "--input")
                .unwrap_or_else(|| "agent-sync-journal.json".to_string());
            let bytes = std::fs::read(input)?;
            let journal: OperationJournal = serde_json::from_slice(&bytes)?;
            let rolled_back = rollback_journal(&journal)?;
            println!("{}", serde_json::to_string_pretty(&rolled_back)?);
        }
        "rollback-native-session-journal" => {
            let input = value_after(&args, "--input")
                .unwrap_or_else(|| "agent-sync-native-session-journal.json".to_string());
            let bytes = std::fs::read(input)?;
            let journal: SessionNativeFileImportJournal = serde_json::from_slice(&bytes)?;
            let rolled_back = rollback_session_native_file_import_journal(&journal)?;
            println!("{}", serde_json::to_string_pretty(&rolled_back)?);
        }
        "rollback-native-remap-journal" => {
            let input = value_after(&args, "--input")
                .unwrap_or_else(|| "agent-sync-native-remap-journal.json".to_string());
            let bytes = std::fs::read(input)?;
            let journal: NativeSessionProjectRemapJournal = serde_json::from_slice(&bytes)?;
            let rolled_back = rollback_native_session_project_remap_journal(&journal)?;
            println!("{}", serde_json::to_string_pretty(&rolled_back)?);
        }
        "self-plan" => {
            let snapshot = scan_device(default_scan_options(&args))?;
            let plan = create_transform_plan(&snapshot, &snapshot, None);
            let report = preflight(&plan);
            let journal = create_journal(&plan);
            println!(
                "{}",
                serde_json::to_string_pretty(&(plan, report, journal))?
            );
        }
        _ => {
            eprintln!(
                "usage: agent-sync-rs [scan|bundle-manifest|export-bundle|verify-bundle|check-native-sessions|discover-native-stores|preview-native-remap|apply-native-remap|import-native-sessions|rollback-journal|rollback-native-session-journal|rollback-native-remap-journal|self-plan] [--home PATH] [--project PATH] [--max-depth N] [--max-entries N] [--max-schema-tables N] [--source-project PATH] [--candidate 'AGENT_ID|PORTABLE_PATH|TABLE|COLUMN'] [--output PATH] [--input PATH] [--payload AGENT_ID:PORTABLE_PATH] [--include-session-payloads --session SESSION_ID --bundle-passphrase PASSPHRASE --allow-unencrypted-sensitive-payloads] [--target-home PATH --target-project PATH --backup-dir PATH --no-rewrite-project-identity] [--skip-agent-stopped-check] [--no-target-scan]"
            );
            std::process::exit(2);
        }
    }
    Ok(())
}

fn default_scan_options(args: &[String]) -> ScanOptions {
    let mut home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut project = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut max_depth = None;
    let mut max_entries = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--home" if i + 1 < args.len() => {
                home = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--project" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--max-depth" if i + 1 < args.len() => {
                max_depth = args[i + 1].parse::<usize>().ok();
                i += 2;
            }
            "--max-entries" if i + 1 < args.len() => {
                max_entries = args[i + 1].parse::<usize>().ok();
                i += 2;
            }
            _ => i += 1,
        }
    }
    let mut options = ScanOptions::new(home, project);
    if let Some(max_depth) = max_depth {
        options.max_depth = max_depth;
    }
    if let Some(max_entries) = max_entries {
        options.max_entries = max_entries;
    }
    options
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn values_after(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut i = 0;
    while i + 1 < args.len() {
        if args[i] == flag {
            values.push(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    values
}

fn bundle_passphrase(args: &[String]) -> Option<String> {
    value_after(args, "--bundle-passphrase")
        .or_else(|| env::var("AGENT_SYNC_BUNDLE_PASSPHRASE").ok())
        .filter(|value| !value.is_empty())
}

fn selected_session_ids_or_all(
    bundle: &agent_sync_bundle::SyncBundle,
    args: &[String],
) -> Vec<String> {
    let selected = values_after(args, "--session");
    if selected.is_empty() {
        bundle
            .session_archives
            .iter()
            .map(|archive| archive.session.id.clone())
            .collect()
    } else {
        selected
    }
}

fn payload_selection_values(args: &[String], flag: &str) -> Vec<PayloadSelectionRef> {
    values_after(args, flag)
        .into_iter()
        .filter_map(|value| {
            value
                .split_once(':')
                .filter(|(agent_id, portable_path)| {
                    !agent_id.is_empty() && !portable_path.is_empty()
                })
                .map(|(agent_id, portable_path)| PayloadSelectionRef {
                    agent_id: agent_id.to_string(),
                    portable_path: portable_path.to_string(),
                })
        })
        .collect()
}

fn remap_selection_values(args: &[String]) -> Vec<NativeSessionProjectRemapSelection> {
    values_after(args, "--candidate")
        .into_iter()
        .filter_map(|value| {
            let parts = value.split('|').collect::<Vec<_>>();
            if parts.len() != 4 || parts.iter().any(|part| part.is_empty()) {
                return None;
            }
            Some(NativeSessionProjectRemapSelection {
                agent_id: parts[0].to_string(),
                portable_path: parts[1].to_string(),
                table: parts[2].to_string(),
                column: parts[3].to_string(),
            })
        })
        .collect()
}
