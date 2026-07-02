use agent_sync_core::{DeviceSnapshot, Finding, ProjectIdentity, SafetyClass};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotDiff {
    pub generated_at: DateTime<Utc>,
    pub from_snapshot: Uuid,
    pub to_snapshot: Uuid,
    pub summary: DiffSummary,
    pub agents: Vec<AgentDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiffSummary {
    pub agents_both: usize,
    pub agents_only_from: usize,
    pub agents_only_to: usize,
    pub findings_only_from: usize,
    pub findings_only_to: usize,
    pub findings_changed: usize,
    pub findings_unchanged: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDiff {
    pub id: String,
    pub name: String,
    pub status: AgentDiffStatus,
    pub only_from: Vec<Finding>,
    pub only_to: Vec<Finding>,
    pub changed: Vec<ChangedFinding>,
    pub unchanged: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDiffStatus {
    Both,
    OnlyFrom,
    OnlyTo,
    Absent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedFinding {
    pub path: String,
    pub from: Finding,
    pub to: Finding,
    pub changed_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransformPlan {
    pub id: Uuid,
    pub generated_at: DateTime<Utc>,
    pub source_snapshot: Uuid,
    pub target_snapshot: Uuid,
    pub target_platform: String,
    pub operations: Vec<ApplyOperation>,
    pub project_mappings: Vec<ProjectMapping>,
    pub blocked: Vec<BlockedOperation>,
    pub warnings: Vec<TransformWarning>,
    pub summary: TransformSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMapping {
    pub source_project_id: Uuid,
    pub target_project_id: Option<Uuid>,
    pub source_canonical_path: String,
    pub target_canonical_path: Option<String>,
    pub source_git_remote: Option<String>,
    pub target_git_remote: Option<String>,
    pub strategy: ProjectMappingStrategy,
    pub status: ProjectMappingStatus,
    pub confidence: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMappingStrategy {
    GitRemoteExact,
    CanonicalBasename,
    SingleProjectFallback,
    NoCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMappingStatus {
    Matched,
    ManualReview,
    Unmapped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TransformSummary {
    pub safe_candidates: usize,
    pub review_required: usize,
    pub blocked: usize,
    pub changed: usize,
    pub missing_on_target: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyOperation {
    pub id: Uuid,
    pub agent_id: String,
    pub agent_name: String,
    pub path: String,
    pub kind: ApplyOperationKind,
    pub safety_class: SafetyClass,
    pub risk: String,
    pub rationale: String,
    pub change_type: ChangeType,
    pub path_warnings: Vec<String>,
    pub requires_review: bool,
    pub requires_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyOperationKind {
    CopyFile,
    MergeText,
    ImportMemory,
    InstallTool,
    ImportSessionAsArchive,
    ImportSessionAsNewNativeSession,
    RewriteSessionProjectIdentity,
    UpdateAgentIndex,
    UpdateProjectVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockedOperation {
    pub agent_id: String,
    pub path: String,
    pub safety_class: SafetyClass,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransformWarning {
    pub path: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    MissingOnTarget,
    ChangedBetweenSnapshots,
}

pub fn diff_snapshots(from: &DeviceSnapshot, to: &DeviceSnapshot) -> SnapshotDiff {
    let mut summary = DiffSummary::default();
    let mut agents = Vec::new();
    let from_agents = from
        .agents
        .iter()
        .map(|agent| (&agent.id, agent))
        .collect::<BTreeMap<_, _>>();
    let to_agents = to
        .agents
        .iter()
        .map(|agent| (&agent.id, agent))
        .collect::<BTreeMap<_, _>>();
    let ids = from_agents
        .keys()
        .chain(to_agents.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    for id in ids {
        let from_agent = from_agents.get(id);
        let to_agent = to_agents.get(id);
        let status = match (from_agent.map(|a| a.detected), to_agent.map(|a| a.detected)) {
            (Some(true), Some(true)) => AgentDiffStatus::Both,
            (Some(true), _) => AgentDiffStatus::OnlyFrom,
            (_, Some(true)) => AgentDiffStatus::OnlyTo,
            _ => AgentDiffStatus::Absent,
        };
        match status {
            AgentDiffStatus::Both => summary.agents_both += 1,
            AgentDiffStatus::OnlyFrom => summary.agents_only_from += 1,
            AgentDiffStatus::OnlyTo => summary.agents_only_to += 1,
            AgentDiffStatus::Absent => {}
        }

        let name = from_agent
            .or(to_agent)
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| id.to_string());
        let from_index = from_agent
            .map(|a| index_findings(&a.findings))
            .unwrap_or_default();
        let to_index = to_agent
            .map(|a| index_findings(&a.findings))
            .unwrap_or_default();
        let keys = from_index
            .keys()
            .chain(to_index.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut agent_diff = AgentDiff {
            id: id.to_string(),
            name,
            status,
            only_from: Vec::new(),
            only_to: Vec::new(),
            changed: Vec::new(),
            unchanged: 0,
        };

        for key in keys {
            match (from_index.get(&key), to_index.get(&key)) {
                (Some(from_finding), Some(to_finding)) => {
                    let changed = changed_fields(from_finding, to_finding);
                    if changed.is_empty() {
                        agent_diff.unchanged += 1;
                        summary.findings_unchanged += 1;
                    } else {
                        agent_diff.changed.push(ChangedFinding {
                            path: key,
                            from: (*from_finding).clone(),
                            to: (*to_finding).clone(),
                            changed_fields: changed,
                        });
                        summary.findings_changed += 1;
                    }
                }
                (Some(finding), None) => {
                    agent_diff.only_from.push((*finding).clone());
                    summary.findings_only_from += 1;
                }
                (None, Some(finding)) => {
                    agent_diff.only_to.push((*finding).clone());
                    summary.findings_only_to += 1;
                }
                (None, None) => {}
            }
        }
        agents.push(agent_diff);
    }

    SnapshotDiff {
        generated_at: Utc::now(),
        from_snapshot: from.id,
        to_snapshot: to.id,
        summary,
        agents,
    }
}

pub fn create_transform_plan(
    from: &DeviceSnapshot,
    to: &DeviceSnapshot,
    target_platform: Option<String>,
) -> TransformPlan {
    let diff = diff_snapshots(from, to);
    let target_platform = target_platform.unwrap_or_else(|| to.platform.os.clone());
    let mut plan = TransformPlan {
        id: Uuid::new_v4(),
        generated_at: Utc::now(),
        source_snapshot: from.id,
        target_snapshot: to.id,
        target_platform: target_platform.clone(),
        operations: Vec::new(),
        project_mappings: suggest_project_mappings(from, to),
        blocked: Vec::new(),
        warnings: Vec::new(),
        summary: TransformSummary::default(),
    };

    for mapping in &plan.project_mappings {
        if mapping.status != ProjectMappingStatus::Matched {
            plan.warnings.push(TransformWarning {
                path: Some(mapping.source_canonical_path.clone()),
                code: "project_mapping_review".to_string(),
                message: format!(
                    "source project requires manual mapping before native session identity rewrite: {}",
                    mapping.reason
                ),
            });
        }
    }

    for agent in diff.agents {
        for finding in agent.only_from {
            plan.summary.missing_on_target += 1;
            add_finding_operation(
                &mut plan,
                &agent.id,
                &agent.name,
                finding,
                ChangeType::MissingOnTarget,
                &target_platform,
            );
        }
        for changed in agent.changed {
            plan.summary.changed += 1;
            add_finding_operation(
                &mut plan,
                &agent.id,
                &agent.name,
                changed.from,
                ChangeType::ChangedBetweenSnapshots,
                &target_platform,
            );
        }
    }

    plan.summary.blocked = plan.blocked.len();
    plan.summary.safe_candidates = plan
        .operations
        .iter()
        .filter(|op| !op.requires_review)
        .count();
    plan.summary.review_required = plan
        .operations
        .iter()
        .filter(|op| op.requires_review)
        .count();
    plan
}

pub fn suggest_project_mappings(from: &DeviceSnapshot, to: &DeviceSnapshot) -> Vec<ProjectMapping> {
    from.projects
        .iter()
        .map(|source| suggest_project_mapping(source, &to.projects, from.projects.len()))
        .collect()
}

fn suggest_project_mapping(
    source: &ProjectIdentity,
    targets: &[ProjectIdentity],
    source_count: usize,
) -> ProjectMapping {
    if let Some(source_remote) = source.git_remote.as_deref().and_then(normalize_git_remote) {
        if let Some(target) = targets.iter().find(|target| {
            target
                .git_remote
                .as_deref()
                .and_then(normalize_git_remote)
                .as_deref()
                == Some(source_remote.as_str())
        }) {
            return project_mapping(
                source,
                Some(target),
                ProjectMappingStrategy::GitRemoteExact,
                ProjectMappingStatus::Matched,
                100,
                "same normalized git remote".to_string(),
            );
        }
    }

    if let Some(source_basename) = basename_key(&source.canonical_path) {
        let candidates = targets
            .iter()
            .filter(|target| {
                basename_key(&target.canonical_path).as_deref() == Some(&source_basename)
            })
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            return project_mapping(
                source,
                candidates.first().copied(),
                ProjectMappingStrategy::CanonicalBasename,
                ProjectMappingStatus::ManualReview,
                65,
                "same project directory name; confirm because platform paths differ".to_string(),
            );
        }
    }

    if source_count == 1 && targets.len() == 1 {
        return project_mapping(
            source,
            targets.first(),
            ProjectMappingStrategy::SingleProjectFallback,
            ProjectMappingStatus::ManualReview,
            40,
            "only one source and one target project; confirm before rewriting identities"
                .to_string(),
        );
    }

    project_mapping(
        source,
        None,
        ProjectMappingStrategy::NoCandidate,
        ProjectMappingStatus::Unmapped,
        0,
        "no stable git remote or path match found".to_string(),
    )
}

fn project_mapping(
    source: &ProjectIdentity,
    target: Option<&ProjectIdentity>,
    strategy: ProjectMappingStrategy,
    status: ProjectMappingStatus,
    confidence: u8,
    reason: String,
) -> ProjectMapping {
    ProjectMapping {
        source_project_id: source.id,
        target_project_id: target.map(|target| target.id),
        source_canonical_path: source.canonical_path.clone(),
        target_canonical_path: target.map(|target| target.canonical_path.clone()),
        source_git_remote: source.git_remote.clone(),
        target_git_remote: target.and_then(|target| target.git_remote.clone()),
        strategy,
        status,
        confidence,
        reason,
    }
}

fn normalize_git_remote(remote: &str) -> Option<String> {
    let mut value = remote.trim().trim_end_matches(".git").to_lowercase();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix("git@") {
        value = rest.replacen(':', "/", 1);
    }
    for prefix in ["https://", "http://", "ssh://", "git://"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_string();
            break;
        }
    }
    Some(value.trim_end_matches('/').to_string())
}

fn basename_key(path: &str) -> Option<String> {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_lowercase())
}

fn index_findings(findings: &[Finding]) -> BTreeMap<String, &Finding> {
    findings
        .iter()
        .map(|finding| (finding.portable_path.clone(), finding))
        .collect()
}

fn changed_fields(from: &Finding, to: &Finding) -> Vec<String> {
    let mut fields = Vec::new();
    if from.kind != to.kind {
        fields.push("kind".to_string());
    }
    if from.size != to.size {
        fields.push("size".to_string());
    }
    if from.safety_class != to.safety_class {
        fields.push("safety_class".to_string());
    }
    if from.risk != to.risk {
        fields.push("risk".to_string());
    }
    if from.recommendation != to.recommendation {
        fields.push("recommendation".to_string());
    }
    fields
}

fn add_finding_operation(
    plan: &mut TransformPlan,
    agent_id: &str,
    agent_name: &str,
    finding: Finding,
    change_type: ChangeType,
    target_platform: &str,
) {
    let warnings = path_warnings(&finding.portable_path, target_platform);
    for warning in &warnings {
        plan.warnings.push(TransformWarning {
            path: Some(finding.portable_path.clone()),
            code: warning.clone(),
            message: format!(
                "{} needs target-platform path review",
                finding.portable_path
            ),
        });
    }

    if matches!(
        finding.safety_class,
        SafetyClass::SecretBearing | SafetyClass::Database | SafetyClass::BinaryOrCache
    ) {
        plan.blocked.push(BlockedOperation {
            agent_id: agent_id.to_string(),
            path: finding.portable_path,
            safety_class: finding.safety_class,
            reason: "blocked by product safety policy; use adapter-specific import or recreate on target".to_string(),
        });
        return;
    }

    let (kind, requires_review, rationale) = match finding.safety_class {
        SafetyClass::SafeConfig => (
            ApplyOperationKind::MergeText,
            false,
            "safe text config can be copied or merged after preview",
        ),
        SafetyClass::McpConfig => (
            ApplyOperationKind::InstallTool,
            true,
            "MCP changes can execute tools and must be reviewed per target",
        ),
        SafetyClass::MemoryKnowledge => (
            ApplyOperationKind::ImportMemory,
            true,
            "memory/rules can change agent behavior and may contain private context",
        ),
        SafetyClass::RawSession => (
            ApplyOperationKind::ImportSessionAsArchive,
            true,
            "raw sessions require explicit adapter import, backup, and verification",
        ),
        SafetyClass::Executable => (
            ApplyOperationKind::InstallTool,
            true,
            "hooks/scripts are executable and require trust review",
        ),
        SafetyClass::Unknown => (
            ApplyOperationKind::MergeText,
            true,
            "unknown surface requires manual review",
        ),
        SafetyClass::SecretBearing | SafetyClass::Database | SafetyClass::BinaryOrCache => {
            unreachable!()
        }
    };

    plan.operations.push(ApplyOperation {
        id: Uuid::new_v4(),
        agent_id: agent_id.to_string(),
        agent_name: agent_name.to_string(),
        path: finding.portable_path,
        kind,
        safety_class: finding.safety_class,
        risk: finding.risk.as_str().to_string(),
        rationale: rationale.to_string(),
        change_type,
        path_warnings: warnings,
        requires_review,
        requires_backup: true,
    });
}

fn path_warnings(path: &str, target_platform: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if path.contains(":\\") || path.chars().nth(1) == Some(':') {
        warnings.push("windows_absolute_path".to_string());
    }
    if path.starts_with("/Users/") || path.starts_with("/home/") {
        warnings.push("posix_absolute_path".to_string());
    }
    if target_platform == "windows" || target_platform == "win32" {
        if path.contains('/') && !path.starts_with("~") && !path.starts_with("<project>") {
            warnings.push("check_windows_path_mapping".to_string());
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_sync_core::{
        AgentProjectKey, AgentSnapshot, FileKind, Finding, PlatformInfo, ProjectIdentity,
        RiskLevel, RootRecord, SafetyClass, SnapshotInputs, SnapshotSummary,
    };

    fn snapshot(id_suffix: &str, findings: Vec<Finding>) -> DeviceSnapshot {
        DeviceSnapshot {
            schema_version: "0.2".to_string(),
            id: uuid::Uuid::new_v4(),
            generated_at: chrono::Utc::now(),
            platform: PlatformInfo {
                os: id_suffix.to_string(),
                arch: "test".to_string(),
            },
            inputs: SnapshotInputs {
                home: "~".into(),
                project: "/tmp/project".into(),
                max_depth: 1,
                max_entries: 10,
            },
            summary: SnapshotSummary::default(),
            projects: Vec::new(),
            agents: vec![AgentSnapshot {
                id: "codex".into(),
                name: "Codex".into(),
                detected: true,
                roots: vec![RootRecord {
                    path: "~/.codex".into(),
                    scope: "home".into(),
                    exists: true,
                    note: None,
                }],
                findings,
                sessions: Vec::new(),
            }],
        }
    }

    fn finding(path: &str, class: SafetyClass) -> Finding {
        Finding {
            path: path.into(),
            portable_path: path.into(),
            kind: FileKind::File,
            depth: 0,
            size: Some(1),
            mtime: None,
            safety_class: class,
            risk: RiskLevel::LowMedium,
            reason: "r".into(),
            recommendation: "x".into(),
            truncated: false,
        }
    }

    fn project(path: &str, remote: Option<&str>) -> ProjectIdentity {
        ProjectIdentity {
            id: uuid::Uuid::new_v4(),
            canonical_path: path.into(),
            physical_path: Some(path.into()),
            git_remote: remote.map(str::to_string),
            git_root_fingerprint: None,
            package_name: None,
            agent_project_keys: Vec::<AgentProjectKey>::new(),
        }
    }

    #[test]
    fn transform_blocks_secret_and_allows_config() {
        let from = snapshot(
            "darwin",
            vec![
                finding("~/.codex/config.toml", SafetyClass::SafeConfig),
                finding("~/.codex/auth.json", SafetyClass::SecretBearing),
            ],
        );
        let to = snapshot("win32", vec![]);
        let plan = create_transform_plan(&from, &to, Some("windows".into()));
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.blocked.len(), 1);
        assert_eq!(plan.operations[0].kind, ApplyOperationKind::MergeText);
    }

    #[test]
    fn maps_projects_by_normalized_git_remote() {
        let mut from = snapshot("darwin", vec![]);
        let mut to = snapshot("windows", vec![]);
        from.projects = vec![project(
            "/Users/me/work/agent-sync",
            Some("git@github.com:JinSooo/agent-sync.git"),
        )];
        to.projects = vec![project(
            "C:/Users/me/source/agent-sync",
            Some("https://github.com/jinsooo/agent-sync"),
        )];

        let plan = create_transform_plan(&from, &to, Some("windows".into()));

        assert_eq!(plan.project_mappings.len(), 1);
        assert_eq!(
            plan.project_mappings[0].strategy,
            ProjectMappingStrategy::GitRemoteExact
        );
        assert_eq!(
            plan.project_mappings[0].status,
            ProjectMappingStatus::Matched
        );
        assert_eq!(plan.project_mappings[0].confidence, 100);
        assert!(
            !plan
                .warnings
                .iter()
                .any(|warning| warning.code == "project_mapping_review")
        );
    }

    #[test]
    fn basename_mapping_requires_manual_review() {
        let mut from = snapshot("darwin", vec![]);
        let mut to = snapshot("windows", vec![]);
        from.projects = vec![project("/Users/me/work/sync-tools", None)];
        to.projects = vec![project("C:/Users/me/source/sync-tools", None)];

        let plan = create_transform_plan(&from, &to, Some("windows".into()));

        assert_eq!(
            plan.project_mappings[0].strategy,
            ProjectMappingStrategy::CanonicalBasename
        );
        assert_eq!(
            plan.project_mappings[0].status,
            ProjectMappingStatus::ManualReview
        );
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.code == "project_mapping_review")
        );
    }
}
