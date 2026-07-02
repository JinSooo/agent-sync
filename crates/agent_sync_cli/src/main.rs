use agent_sync_apply::{
    NativeSessionProjectRemapApplyOptions, NativeSessionProjectRemapDryRunOptions,
    NativeSessionProjectRemapJournal, NativeSessionProjectRemapPreviewOptions,
    NativeSessionProjectRemapSelection, NativeSessionStoreDiscoveryOptions, OperationJournal,
    SessionNativeFileImportJournal, SessionNativeFileImportOptions,
    SessionNativeImportReadinessOptions, apply_native_session_project_remap, create_journal,
    discover_native_session_stores, dry_run_native_session_project_remap,
    import_session_payloads_to_native_files, preflight, preview_native_session_project_remap,
    rollback_journal, rollback_native_session_project_remap_journal,
    rollback_session_native_file_import_journal, session_native_import_readiness,
};
use agent_sync_bundle::{
    BUNDLE_RECIPIENT_PROFILE_KIND, BundleDeviceKeySummary, BundleExportOptions,
    BundleFileDecryptionOptions, BundleFileEncryptionOptions, BundleRecipientProfile,
    DEFAULT_BUNDLE_KEYRING_ACCOUNT, PayloadSelectionRef, bundle_recipient_from_input,
    bundle_recipient_profile_from_input, delete_bundle_device_key_keyring, export_bundle,
    export_bundle_device_key_keyring_backup, generate_bundle_device_key_file,
    generate_bundle_device_key_keyring, manifest_from_snapshot, read_bundle_device_key_file,
    read_bundle_device_key_keyring, read_bundle_file_with_decryption,
    restore_bundle_device_key_keyring_backup, verify_bundle, write_bundle_file_with_encryption,
    write_bundle_recipient_file,
};
use agent_sync_scan::{ScanOptions, scan_device};
use agent_sync_storage::AgentSyncStore;
use agent_sync_transform::create_transform_plan;
use std::collections::BTreeMap;
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
        "generate-bundle-key" => {
            let output = value_after(&args, "--output")
                .unwrap_or_else(|| "agent-sync-device-key.json".to_string());
            let key = generate_bundle_device_key_file(&output)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&BundleDeviceKeySummary::from(&key))?
            );
        }
        "generate-bundle-keychain" => {
            let account = bundle_keychain_account_or_default(&args);
            let recipient = generate_bundle_device_key_keyring(&account)?;
            println!("{}", serde_json::to_string_pretty(&recipient)?);
        }
        "export-bundle-recipient" => {
            let key_path = bundle_key_path(&args)
                .ok_or_else(|| anyhow::anyhow!("--bundle-key PATH is required"))?;
            let key = read_bundle_device_key_file(key_path)?;
            let recipient = BundleDeviceKeySummary::from(&key);
            if let Some(output) = value_after(&args, "--output") {
                write_bundle_recipient_file(&recipient, output)?;
            }
            println!("{}", serde_json::to_string_pretty(&recipient)?);
        }
        "export-bundle-keychain-recipient" => {
            let account = bundle_keychain_account_or_default(&args);
            let key = read_bundle_device_key_keyring(&account)?;
            let recipient = BundleDeviceKeySummary::from(&key);
            if let Some(output) = value_after(&args, "--output") {
                write_bundle_recipient_file(&recipient, output)?;
            }
            println!("{}", serde_json::to_string_pretty(&recipient)?);
        }
        "forget-bundle-keychain" => {
            let account = bundle_keychain_account_or_default(&args);
            delete_bundle_device_key_keyring(&account)?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "deleted",
                    "account": account
                })
            );
        }
        "export-bundle-keychain-backup" => {
            let account = bundle_keychain_account_or_default(&args);
            let output = value_after(&args, "--output")
                .unwrap_or_else(|| "agent-sync-keychain-backup.age".to_string());
            let passphrase = backup_passphrase(&args)?;
            let summary = export_bundle_device_key_keyring_backup(&account, output, &passphrase)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        "restore-bundle-keychain-backup" => {
            let account = bundle_keychain_account_or_default(&args);
            let input = value_after(&args, "--input")
                .unwrap_or_else(|| "agent-sync-keychain-backup.age".to_string());
            let passphrase = backup_passphrase(&args)?;
            let summary = restore_bundle_device_key_keyring_backup(&account, input, &passphrase)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        "save-bundle-recipient-profile" => {
            let store = AgentSyncStore::open(store_path(&args))?;
            let recipient_input = value_after(&args, "--recipient")
                .or_else(|| value_after(&args, "--bundle-recipient"))
                .ok_or_else(|| anyhow::anyhow!("--recipient AGE_OR_JSON is required"))?;
            let profile = bundle_recipient_profile_from_input(
                &value_after(&args, "--label").unwrap_or_default(),
                value_after(&args, "--device"),
                value_after(&args, "--platform"),
                &recipient_input,
                value_after(&args, "--note"),
                Some("cli".to_string()),
            )?;
            store.save_json(BUNDLE_RECIPIENT_PROFILE_KIND, Some(profile.id), &profile)?;
            println!("{}", serde_json::to_string_pretty(&profile)?);
        }
        "list-bundle-recipient-profiles" => {
            let profiles = load_bundle_recipient_profiles(&args)?;
            println!("{}", serde_json::to_string_pretty(&profiles)?);
        }
        "forget-bundle-recipient-profile" => {
            let id = value_after(&args, "--id")
                .ok_or_else(|| anyhow::anyhow!("--id PROFILE_ID is required"))?;
            let store = AgentSyncStore::open(store_path(&args))?;
            let deleted = store.delete(BUNDLE_RECIPIENT_PROFILE_KIND, &id)?;
            println!(
                "{}",
                serde_json::json!({
                    "status": if deleted { "deleted" } else { "not_found" },
                    "id": id
                })
            );
        }
        "export-bundle" => {
            let output =
                value_after(&args, "--output").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let encryption = bundle_file_encryption_options(&args)?;
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
                    encryption_passphrase: encryption.passphrase.clone(),
                    encryption_recipients: encryption.recipients.clone(),
                },
            )?;
            write_bundle_file_with_encryption(&bundle, &output, &encryption)?;
            println!("{}", serde_json::to_string_pretty(&bundle.manifest)?);
        }
        "verify-bundle" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let decryption = bundle_file_decryption_options(&args)?;
            let bundle = read_bundle_file_with_decryption(input, &decryption)?;
            let errors = verify_bundle(&bundle);
            println!("{}", serde_json::to_string_pretty(&errors)?);
            if !errors.is_empty() {
                std::process::exit(1);
            }
        }
        "check-native-sessions" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let decryption = bundle_file_decryption_options(&args)?;
            let bundle = read_bundle_file_with_decryption(input, &decryption)?;
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
        "dry-run-native-remap" => {
            let options = default_scan_options(&args);
            let snapshot = scan_device(options.clone())?;
            let source_project = value_after(&args, "--source-project").unwrap_or_default();
            let report = dry_run_native_session_project_remap(
                &snapshot,
                &NativeSessionProjectRemapDryRunOptions {
                    target_home: options.home,
                    target_project: options.project,
                    source_project,
                    selections: remap_selection_values(&args),
                    require_agents_stopped: !args
                        .iter()
                        .any(|arg| arg == "--skip-agent-stopped-check"),
                },
            )?;
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
            let decryption = bundle_file_decryption_options(&args)?;
            let target_home = value_after(&args, "--target-home")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            let target_project = value_after(&args, "--target-project");
            let backup_dir = value_after(&args, "--backup-dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("agent-sync-backups"));
            let selected_session_ids = values_after(&args, "--session");
            let target_project_by_session = session_target_project_values(&args)?;
            let bundle = read_bundle_file_with_decryption(input, &decryption)?;
            let journal = import_session_payloads_to_native_files(
                &bundle,
                &SessionNativeFileImportOptions {
                    selected_session_ids,
                    target_home,
                    target_project,
                    target_project_by_session,
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
                "usage: agent-sync-rs [scan|bundle-manifest|generate-bundle-key|generate-bundle-keychain|export-bundle-recipient|export-bundle-keychain-recipient|forget-bundle-keychain|export-bundle-keychain-backup|restore-bundle-keychain-backup|save-bundle-recipient-profile|list-bundle-recipient-profiles|forget-bundle-recipient-profile|export-bundle|verify-bundle|check-native-sessions|discover-native-stores|preview-native-remap|dry-run-native-remap|apply-native-remap|import-native-sessions|rollback-journal|rollback-native-session-journal|rollback-native-remap-journal|self-plan] [--store PATH] [--home PATH] [--project PATH] [--max-depth N] [--max-entries N] [--max-schema-tables N] [--source-project PATH] [--candidate 'AGENT_ID|PORTABLE_PATH|TABLE|COLUMN'] [--output PATH] [--input PATH] [--payload AGENT_ID:PORTABLE_PATH] [--include-session-payloads --session SESSION_ID --bundle-passphrase PASSPHRASE|--bundle-key PATH|--bundle-keychain ACCOUNT|--bundle-recipient AGE_OR_JSON|--bundle-recipient-profile PROFILE_ID --allow-unencrypted-sensitive-payloads] [--backup-passphrase PASSPHRASE] [--target-home PATH --target-project PATH --session-target SESSION_ID=PROJECT_PATH --backup-dir PATH --no-rewrite-project-identity] [--skip-agent-stopped-check] [--no-target-scan]"
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

fn session_target_project_values(args: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for value in values_after(args, "--session-target") {
        let Some((session_id, project)) = value.split_once('=') else {
            anyhow::bail!("--session-target must be SESSION_ID=PROJECT_PATH");
        };
        let session_id = session_id.trim();
        let project = project.trim();
        if session_id.is_empty() || project.is_empty() {
            anyhow::bail!("--session-target must include non-empty session id and project path");
        }
        values.insert(session_id.to_string(), project.to_string());
    }
    Ok(values)
}

fn bundle_passphrase(args: &[String]) -> Option<String> {
    value_after(args, "--bundle-passphrase")
        .or_else(|| env::var("AGENT_SYNC_BUNDLE_PASSPHRASE").ok())
        .filter(|value| !value.is_empty())
}

fn bundle_key_path(args: &[String]) -> Option<String> {
    value_after(args, "--bundle-key")
        .or_else(|| env::var("AGENT_SYNC_BUNDLE_KEY").ok())
        .filter(|value| !value.is_empty())
}

fn bundle_keychain_account(args: &[String]) -> Option<String> {
    value_after(args, "--bundle-keychain")
        .or_else(|| env::var("AGENT_SYNC_BUNDLE_KEYCHAIN").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn bundle_keychain_account_or_default(args: &[String]) -> String {
    bundle_keychain_account(args).unwrap_or_else(|| DEFAULT_BUNDLE_KEYRING_ACCOUNT.to_string())
}

fn backup_passphrase(args: &[String]) -> anyhow::Result<String> {
    value_after(args, "--backup-passphrase")
        .or_else(|| env::var("AGENT_SYNC_BUNDLE_BACKUP_PASSPHRASE").ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "--backup-passphrase or AGENT_SYNC_BUNDLE_BACKUP_PASSPHRASE is required"
            )
        })
}

fn bundle_file_encryption_options(args: &[String]) -> anyhow::Result<BundleFileEncryptionOptions> {
    let passphrase = bundle_passphrase(args);
    let key = bundle_key_path(args)
        .map(read_bundle_device_key_file)
        .transpose()?;
    let keychain_key = bundle_keychain_account(args)
        .map(read_bundle_device_key_keyring)
        .transpose()?;
    let mut recipients = Vec::new();
    if let Some(key) = key {
        recipients.push(key.age_recipient);
    }
    if let Some(key) = keychain_key {
        recipients.push(key.age_recipient);
    }
    for input in values_after(args, "--bundle-recipient") {
        recipients.push(bundle_recipient_from_input(&input)?);
    }
    for profile_id in values_after(args, "--bundle-recipient-profile") {
        let profiles = load_bundle_recipient_profiles(args)?;
        let Some(profile) = profiles
            .iter()
            .find(|profile| profile.id.to_string() == profile_id)
        else {
            anyhow::bail!("bundle recipient profile not found: {profile_id}");
        };
        if profile.revoked {
            anyhow::bail!("bundle recipient profile is revoked: {profile_id}");
        }
        recipients.push(profile.age_recipient.clone());
    }
    if passphrase.is_some() && !recipients.is_empty() {
        anyhow::bail!(
            "--bundle-passphrase is mutually exclusive with --bundle-key/--bundle-keychain/--bundle-recipient/--bundle-recipient-profile"
        );
    }
    Ok(BundleFileEncryptionOptions {
        passphrase,
        recipients,
    })
}

fn bundle_file_decryption_options(args: &[String]) -> anyhow::Result<BundleFileDecryptionOptions> {
    let passphrase = bundle_passphrase(args);
    let key = bundle_key_path(args)
        .map(read_bundle_device_key_file)
        .transpose()?;
    let keychain_key = bundle_keychain_account(args)
        .map(read_bundle_device_key_keyring)
        .transpose()?;
    let identities = key
        .into_iter()
        .chain(keychain_key)
        .map(|key| key.age_identity)
        .collect::<Vec<_>>();
    if passphrase.is_some() && !identities.is_empty() {
        anyhow::bail!(
            "--bundle-passphrase is mutually exclusive with --bundle-key/--bundle-keychain"
        );
    }
    Ok(BundleFileDecryptionOptions {
        passphrase,
        identities,
    })
}

fn store_path(args: &[String]) -> PathBuf {
    value_after(args, "--store")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("agent-sync-studio.sqlite"))
}

fn load_bundle_recipient_profiles(args: &[String]) -> anyhow::Result<Vec<BundleRecipientProfile>> {
    let store = AgentSyncStore::open(store_path(args))?;
    let rows = store.list(BUNDLE_RECIPIENT_PROFILE_KIND)?;
    rows.into_iter()
        .map(|row| serde_json::from_str(&row.json).map_err(Into::into))
        .collect()
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
