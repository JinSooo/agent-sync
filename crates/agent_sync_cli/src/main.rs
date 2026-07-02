use agent_sync_apply::{
    SessionNativeFileImportOptions, create_journal, import_session_payloads_to_native_files,
    preflight,
};
use agent_sync_bundle::{
    BundleExportOptions, PayloadSelectionRef, export_bundle, manifest_from_snapshot,
    read_bundle_file, verify_bundle, write_bundle_file,
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
                },
            )?;
            write_bundle_file(&bundle, &output)?;
            println!("{}", serde_json::to_string_pretty(&bundle.manifest)?);
        }
        "verify-bundle" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let bundle = read_bundle_file(input)?;
            let errors = verify_bundle(&bundle);
            println!("{}", serde_json::to_string_pretty(&errors)?);
            if !errors.is_empty() {
                std::process::exit(1);
            }
        }
        "import-native-sessions" => {
            let input =
                value_after(&args, "--input").unwrap_or_else(|| "agent-sync.asbundle".to_string());
            let target_home = value_after(&args, "--target-home")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            let target_project = value_after(&args, "--target-project");
            let backup_dir = value_after(&args, "--backup-dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("agent-sync-backups"));
            let selected_session_ids = values_after(&args, "--session");
            let bundle = read_bundle_file(input)?;
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
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&journal)?);
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
                "usage: agent-sync-rs [scan|bundle-manifest|export-bundle|verify-bundle|import-native-sessions|self-plan] [--home PATH] [--project PATH] [--max-depth N] [--max-entries N] [--output PATH] [--input PATH] [--payload AGENT_ID:PORTABLE_PATH] [--include-session-payloads --session SESSION_ID] [--target-home PATH --target-project PATH --backup-dir PATH --no-rewrite-project-identity]"
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
