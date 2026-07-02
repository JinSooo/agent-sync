use agent_sync_core::{
    AdapterCapabilities, AgentSnapshot, DeviceSnapshot, FileKind, Finding, PlatformInfo,
    ProjectIdentity, RootRecord, SessionRecord, SnapshotInputs, SnapshotSummary, classify_path,
    portable_path,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOptions {
    pub home: PathBuf,
    pub project: PathBuf,
    pub max_depth: usize,
    pub max_entries: usize,
    pub agents: Vec<String>,
}

impl ScanOptions {
    pub fn new(home: PathBuf, project: PathBuf) -> Self {
        Self {
            home,
            project,
            max_depth: 4,
            max_entries: 2_000,
            agents: vec!["codex".to_string(), "claude".to_string()],
        }
    }
}

#[derive(Debug, Clone)]
struct SurfaceRoot {
    scope: &'static str,
    parts: &'static [&'static str],
}

#[derive(Debug, Clone)]
struct AgentSurface {
    id: &'static str,
    name: &'static str,
    roots: &'static [SurfaceRoot],
    capabilities: AdapterCapabilities,
}

const CODEX_ROOTS: &[SurfaceRoot] = &[
    SurfaceRoot {
        scope: "home",
        parts: &[".codex"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &[".codex"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &["AGENTS.md"],
    },
];

const CLAUDE_ROOTS: &[SurfaceRoot] = &[
    SurfaceRoot {
        scope: "home",
        parts: &[".claude"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &[".claude"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &["CLAUDE.md"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &["CLAUDE.local.md"],
    },
    SurfaceRoot {
        scope: "project",
        parts: &[".mcp.json"],
    },
];

const fn default_supported_capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        can_export_config: true,
        can_import_config: true,
        can_export_memory: true,
        can_import_memory: true,
        can_list_sessions: true,
        can_export_sessions: true,
        can_import_sessions: true,
        can_remap_session_project: false,
        requires_app_stopped_for_session_apply: true,
        supports_transactional_apply: false,
    }
}

const SURFACES: &[AgentSurface] = &[
    AgentSurface {
        id: "codex",
        name: "OpenAI Codex",
        roots: CODEX_ROOTS,
        capabilities: default_supported_capabilities(),
    },
    AgentSurface {
        id: "claude",
        name: "Claude Code",
        roots: CLAUDE_ROOTS,
        capabilities: default_supported_capabilities(),
    },
];

pub fn scan_device(options: ScanOptions) -> agent_sync_core::Result<DeviceSnapshot> {
    let project = project_identity(&options.project);
    let mut agents = Vec::new();
    for surface in SURFACES {
        if !options.agents.is_empty() && !options.agents.iter().any(|id| id == surface.id) {
            continue;
        }
        agents.push(scan_agent(surface, &options, Some(project.id))?);
    }

    let projects = vec![project];
    let summary = summarize(&agents);

    Ok(DeviceSnapshot {
        schema_version: "0.2".to_string(),
        id: Uuid::new_v4(),
        generated_at: Utc::now(),
        platform: PlatformInfo::current(),
        inputs: SnapshotInputs {
            home: "~".to_string(),
            project: options.project.to_string_lossy().to_string(),
            max_depth: options.max_depth,
            max_entries: options.max_entries,
        },
        summary,
        projects,
        agents,
    })
}

fn scan_agent(
    surface: &AgentSurface,
    options: &ScanOptions,
    source_project: Option<Uuid>,
) -> agent_sync_core::Result<AgentSnapshot> {
    let mut agent = AgentSnapshot {
        id: surface.id.to_string(),
        name: surface.name.to_string(),
        detected: false,
        capabilities: surface.capabilities.clone(),
        roots: Vec::new(),
        findings: Vec::new(),
        sessions: Vec::new(),
    };

    for root in surface.roots {
        let root_path = resolve_root(root, options);
        let exists = root_path.exists();
        agent.roots.push(RootRecord {
            path: portable_path(&root_path, &options.home, &options.project),
            scope: root.scope.to_string(),
            exists,
            note: None,
        });
        if exists {
            agent.detected = true;
            collect_path(&root_path, options, surface.id, 0, &mut agent.findings)?;
        }
    }

    agent.sessions = list_session_metadata(&agent.findings, surface.id, source_project);
    Ok(agent)
}

fn resolve_root(root: &SurfaceRoot, options: &ScanOptions) -> PathBuf {
    let base = match root.scope {
        "home" => options.home.clone(),
        "project" => options.project.clone(),
        _ => options.project.clone(),
    };
    root.parts.iter().fold(base, |acc, part| acc.join(part))
}

fn collect_path(
    path: &Path,
    options: &ScanOptions,
    agent_id: &str,
    depth: usize,
    results: &mut Vec<Finding>,
) -> agent_sync_core::Result<()> {
    if results.len() >= options.max_entries {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path)?;
    results.push(finding_record(path, &metadata, options, agent_id, depth));

    if !metadata.is_dir() || metadata.file_type().is_symlink() || depth >= options.max_depth {
        return Ok(());
    }
    if should_prune_directory(path, options) {
        return Ok(());
    }

    let mut entries = match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(error) => {
            results.push(Finding {
                path: portable_path(path, &options.home, &options.project),
                portable_path: portable_path(path, &options.home, &options.project),
                kind: FileKind::Directory,
                depth,
                size: None,
                mtime: None,
                safety_class: agent_sync_core::SafetyClass::Unknown,
                risk: agent_sync_core::RiskLevel::Medium,
                reason: format!("unable to read directory: {}", error),
                recommendation: "manual review required".to_string(),
                truncated: false,
            });
            return Ok(());
        }
    };
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if results.len() >= options.max_entries {
            results.push(Finding {
                path: portable_path(path, &options.home, &options.project),
                portable_path: portable_path(path, &options.home, &options.project),
                kind: FileKind::Directory,
                depth,
                size: None,
                mtime: None,
                safety_class: agent_sync_core::SafetyClass::Unknown,
                risk: agent_sync_core::RiskLevel::Medium,
                reason: format!("entry limit reached ({})", options.max_entries),
                recommendation: "rerun with a larger max_entries value for fuller inventory"
                    .to_string(),
                truncated: true,
            });
            break;
        }
        collect_path(&entry.path(), options, agent_id, depth + 1, results)?;
    }

    Ok(())
}

fn should_prune_directory(path: &Path, options: &ScanOptions) -> bool {
    let portable = portable_path(path, &options.home, &options.project);
    let normalized = portable.replace('\\', "/").to_lowercase();
    let segments = normalized
        .split('/')
        .map(|segment| segment.trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if normalized.contains("/plugins/cache/")
        || normalized.ends_with("/plugins/cache")
        || normalized.contains("/.tmp/")
        || normalized.ends_with("/.tmp")
    {
        return true;
    }

    segments.iter().any(|segment| {
        matches!(
            *segment,
            "node_modules"
                | "dist"
                | "build"
                | "out"
                | "target"
                | "vendor"
                | ".git"
                | "cache"
                | "cached"
                | "tmp"
                | "temp"
                | "logs"
                | "log"
                | "blob_storage"
                | "gpucache"
                | "code cache"
        )
    })
}

fn finding_record(
    path: &Path,
    metadata: &Metadata,
    options: &ScanOptions,
    agent_id: &str,
    depth: usize,
) -> Finding {
    let kind = if metadata.is_dir() {
        FileKind::Directory
    } else if metadata.file_type().is_symlink() {
        FileKind::Symlink
    } else if metadata.is_file() {
        FileKind::File
    } else {
        FileKind::Other
    };
    let mtime = metadata.modified().ok().map(DateTime::<Utc>::from);
    let portable = portable_path(path, &options.home, &options.project);
    let classification = classify_path(Path::new(&portable), &kind, agent_id);
    Finding {
        path: portable.clone(),
        portable_path: portable,
        kind,
        depth,
        size: metadata.is_file().then_some(metadata.len()),
        mtime,
        safety_class: classification.safety_class,
        risk: classification.risk,
        reason: classification.reason,
        recommendation: classification.recommendation,
        truncated: false,
    }
}

fn summarize(agents: &[AgentSnapshot]) -> SnapshotSummary {
    let mut summary = SnapshotSummary::default();
    summary.agents_detected = agents.iter().filter(|agent| agent.detected).count();
    for finding in agents.iter().flat_map(|agent| &agent.findings) {
        summary.findings += 1;
        *summary
            .by_safety_class
            .entry(finding.safety_class.as_str().to_string())
            .or_insert(0) += 1;
        *summary
            .by_risk
            .entry(finding.risk.as_str().to_string())
            .or_insert(0) += 1;
    }
    summary
}

fn project_identity(project: &Path) -> ProjectIdentity {
    ProjectIdentity {
        id: Uuid::new_v4(),
        canonical_path: agent_sync_core::normalize_path(project),
        physical_path: Some(project.to_string_lossy().to_string()),
        git_remote: read_git_remote(project),
        git_root_fingerprint: None,
        package_name: None,
        agent_project_keys: Vec::new(),
    }
}

fn read_git_remote(project: &Path) -> Option<String> {
    let config = fs::read_to_string(project.join(".git").join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_origin = trimmed == "[remote \"origin\"]";
            continue;
        }
        if in_origin && trimmed.starts_with("url") {
            return trimmed
                .split_once('=')
                .map(|(_, value)| value.trim().to_string());
        }
    }
    None
}

fn list_session_metadata(
    findings: &[Finding],
    agent_id: &str,
    source_project: Option<Uuid>,
) -> Vec<SessionRecord> {
    findings
        .iter()
        .filter(|finding| {
            finding.kind == FileKind::File
                && finding.safety_class == agent_sync_core::SafetyClass::RawSession
        })
        .take(200)
        .map(|finding| SessionRecord {
            id: format!("{}:{}", agent_id, finding.portable_path),
            agent_id: agent_id.to_string(),
            title: Some(finding.portable_path.clone()),
            created_at: None,
            updated_at: finding.mtime,
            source_project,
            storage_refs: vec![agent_sync_core::StorageRef {
                kind: "raw_session_surface".to_string(),
                portable_path: finding.portable_path.clone(),
                physical_path: None,
            }],
            visibility: agent_sync_core::SessionVisibility::Unknown,
            content_policy: agent_sync_core::ContentPolicy::ExplicitRawPayloadRequired,
            import_capabilities: agent_sync_core::SessionImportCapabilities {
                import_as_archive: true,
                import_as_new_session: false,
                identity_rewrite: false,
                requires_app_stopped: true,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_codex_and_claude_project_files() {
        let temp = std::env::temp_dir().join(format!("agent-sync-scan-{}", uuid::Uuid::new_v4()));
        let home = temp.join("home");
        let project = temp.join("project");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(home.join(".codex").join("sessions")).unwrap();
        fs::create_dir_all(home.join(".codex").join("memories")).unwrap();
        fs::create_dir_all(project.join(".claude")).unwrap();
        fs::write(home.join(".codex").join("sessions").join("s1.json"), "{}").unwrap();
        fs::write(
            home.join(".codex").join("memories").join("guide.md"),
            "# memory",
        )
        .unwrap();
        fs::write(project.join("AGENTS.md"), "instructions").unwrap();
        fs::write(project.join("CLAUDE.md"), "instructions").unwrap();
        let snapshot = scan_device(ScanOptions::new(home, project)).unwrap();
        assert_eq!(snapshot.summary.agents_detected, 2);
        assert!(
            snapshot
                .agents
                .iter()
                .any(|agent| agent.id == "codex" && agent.detected)
        );
        assert!(
            snapshot
                .agents
                .iter()
                .any(|agent| agent.id == "claude" && agent.detected)
        );
        let codex = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == "codex")
            .unwrap();
        assert_eq!(codex.sessions.len(), 1);
        assert!(codex.capabilities.can_export_sessions);
        assert!(codex.capabilities.can_import_sessions);
        assert!(!codex.capabilities.can_remap_session_project);
        assert!(codex.capabilities.requires_app_stopped_for_session_apply);
        assert_eq!(
            codex.sessions[0].source_project,
            Some(snapshot.projects[0].id)
        );
        assert!(codex.findings.iter().any(|finding| {
            finding.portable_path == "~/.codex/memories/guide.md"
                && finding.safety_class == agent_sync_core::SafetyClass::MemoryKnowledge
        }));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn prunes_generated_cache_dependency_trees_but_keeps_directory_marker() {
        let temp =
            std::env::temp_dir().join(format!("agent-sync-scan-prune-{}", uuid::Uuid::new_v4()));
        let home = temp.join("home");
        let project = temp.join("project");
        let leveldb_dir = home
            .join(".codex")
            .join("plugins")
            .join("cache")
            .join("example-plugin")
            .join("node_modules")
            .join("classic-level")
            .join("deps")
            .join("leveldb");
        fs::create_dir_all(&leveldb_dir).unwrap();
        fs::write(leveldb_dir.join("leveldb.gyp"), "generated dependency").unwrap();
        fs::create_dir_all(&project).unwrap();

        let snapshot = scan_device(ScanOptions {
            home,
            project,
            max_depth: 12,
            max_entries: 200,
            agents: vec!["codex".into()],
        })
        .unwrap();
        let codex = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == "codex")
            .unwrap();

        assert!(codex.findings.iter().any(|finding| {
            finding.portable_path == "~/.codex/plugins/cache"
                && finding.kind == agent_sync_core::FileKind::Directory
        }));
        assert!(
            !codex
                .findings
                .iter()
                .any(|finding| finding.portable_path.contains("node_modules"))
        );
        assert!(
            !codex
                .findings
                .iter()
                .any(|finding| finding.portable_path.contains("leveldb.gyp"))
        );
        let _ = fs::remove_dir_all(temp);
    }
}
