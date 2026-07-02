use agent_sync_apply::{create_journal, preflight};
use agent_sync_bundle::{
    BundleExportOptions, export_bundle, manifest_from_snapshot, read_bundle_file, verify_bundle,
    write_bundle_file,
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
            let bundle = export_bundle(
                &snapshot,
                &BundleExportOptions {
                    home: options.home,
                    project: options.project,
                    max_payload_bytes: 1024 * 1024,
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
                "usage: agent-sync-rs [scan|bundle-manifest|export-bundle|verify-bundle|self-plan] [--home PATH] [--project PATH] [--output PATH] [--input PATH]"
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
            _ => i += 1,
        }
    }
    ScanOptions::new(home, project)
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}
