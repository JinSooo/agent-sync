import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

type SnapshotSummary = {
  agents_detected: number;
  findings: number;
  by_safety_class: Record<string, number>;
  by_risk: Record<string, number>;
};

type AdapterCapabilities = {
  can_export_config: boolean;
  can_import_config: boolean;
  can_export_memory: boolean;
  can_import_memory: boolean;
  can_list_sessions: boolean;
  can_export_sessions: boolean;
  can_import_sessions: boolean;
  can_remap_session_project: boolean;
  requires_app_stopped_for_session_apply: boolean;
  supports_transactional_apply: boolean;
};

type AgentSnapshot = {
  id: string;
  name: string;
  detected: boolean;
  capabilities: AdapterCapabilities;
  findings: Array<{ portable_path: string; safety_class: string; risk: string; reason: string }>;
  sessions: SessionRecord[];
};

type ProjectIdentity = {
  id: string;
  canonical_path: string;
  physical_path?: string;
  git_remote?: string;
  package_name?: string;
};

type SessionImportCapabilities = {
  import_as_archive: boolean;
  import_as_new_session: boolean;
  identity_rewrite: boolean;
  requires_app_stopped: boolean;
};

type SessionRecord = {
  id: string;
  agent_id: string;
  title?: string;
  source_project?: string;
  visibility: string;
  content_policy: string;
  import_capabilities?: SessionImportCapabilities;
};

type DeviceSnapshot = {
  id: string;
  generated_at: string;
  platform: { os: string; arch: string };
  inputs: { home: string; project: string; max_depth: number; max_entries: number };
  summary: SnapshotSummary;
  projects: ProjectIdentity[];
  agents: AgentSnapshot[];
};

type ApplyOperation = {
  id: string;
  path: string;
  agent_id: string;
  agent_name: string;
  kind: string;
  safety_class: string;
  requires_review: boolean;
  requires_backup: boolean;
  rationale: string;
};

type ProjectMapping = {
  source_project_id: string;
  target_project_id?: string;
  source_canonical_path: string;
  target_canonical_path?: string;
  source_git_remote?: string;
  target_git_remote?: string;
  strategy: string;
  status: string;
  confidence: number;
  reason: string;
};

type TransformPlan = {
  id: string;
  source_snapshot: string;
  target_snapshot: string;
  target_platform: string;
  summary: {
    safe_candidates: number;
    review_required: number;
    blocked: number;
    changed: number;
    missing_on_target: number;
  };
  operations: ApplyOperation[];
  project_mappings: ProjectMapping[];
  blocked: Array<{ path: string; agent_id: string; safety_class: string; reason: string }>;
  warnings: Array<{ code: string; message: string; path?: string }>;
};

type PreflightReport = {
  passed: boolean;
  blockers: string[];
  warnings: string[];
  operations_requiring_review: number;
  operations_requiring_backup: number;
};

type SyncBundleManifest = {
  id: string;
  created_at: string;
  selections: unknown[];
  redactions: unknown[];
};

type PayloadEntry = {
  agent_id: string;
  portable_path: string;
  sha256: string;
  base64_content: string;
};

type PayloadSelectionRef = {
  agent_id: string;
  portable_path: string;
};

type SessionArchiveEntry = {
  agent_id: string;
  agent_name: string;
  session: SessionRecord;
  source_project?: ProjectIdentity;
  payload_included: boolean;
  payloads: PayloadEntry[];
  import_note: string;
};

type SyncBundle = {
  manifest: SyncBundleManifest;
  source_snapshot: DeviceSnapshot;
  payloads: PayloadEntry[];
  session_archives: SessionArchiveEntry[];
};

type OperationJournal = {
  id: string;
  status: string;
  operations: Array<{ status: string; message?: string; operation: ApplyOperation; backup?: { backup_path: string } }>;
};

type StoredRecord = {
  id: string;
  kind: string;
  created_at: string;
  updated_at: string;
  json: string;
};

type SessionArchiveImportJournal = {
  id: string;
  status: string;
  selected: number;
  imported: number;
  skipped: number;
  records: Array<{
    record_id: string;
    agent_id: string;
    agent_name: string;
    session_id: string;
    title?: string;
    source_project?: string;
    target_project?: string;
    payload_included: boolean;
    note: string;
  }>;
};

type SessionNativeImportStageJournal = {
  id: string;
  status: string;
  selected: number;
  staged: number;
  skipped: number;
  records: Array<{
    agent_id: string;
    agent_name: string;
    session_id: string;
    title?: string;
    source_project?: string;
    target_project?: string;
    note: string;
    written_payloads: Array<{
      portable_path: string;
      staged_path: string;
      source_sha256: string;
      staged_sha256: string;
      project_identity_rewritten: boolean;
    }>;
  }>;
};

type SessionNativeFileImportJournal = {
  id: string;
  status: string;
  selected: number;
  imported: number;
  skipped: number;
  blockers: string[];
  records: Array<{
    agent_id: string;
    agent_name: string;
    session_id: string;
    title?: string;
    source_project?: string;
    target_project?: string;
    note: string;
    written_payloads: Array<{
      portable_path: string;
      target_path: string;
      backup_path?: string;
      source_sha256: string;
      written_sha256?: string;
      project_identity_rewritten: boolean;
      status: string;
      message?: string;
    }>;
  }>;
};

type SessionNativeImportReadinessReport = {
  selected: number;
  ready: number;
  blocked: number;
  warnings: string[];
  blockers: string[];
  entries: Array<{
    agent_id: string;
    agent_name: string;
    session_id: string;
    title?: string;
    payloads: number;
    can_import_native_files: boolean;
    can_rewrite_project_identity: boolean;
    can_remap_session_project: boolean;
    requires_app_stopped: boolean;
    ready: boolean;
    warnings: string[];
    blockers: string[];
    note: string;
  }>;
};

type NativeSessionStoreDiscoveryReport = {
  warnings: string[];
  stores: Array<{
    agent_id: string;
    agent_name: string;
    portable_path: string;
    store_kind: string;
    size?: number;
    schema_status: string;
    tables: Array<{ name: string; columns: string[] }>;
    note: string;
  }>;
};

const safetyOrder = ['safe_config', 'memory_knowledge', 'mcp_config', 'raw_session', 'executable', 'database', 'secret_bearing', 'binary_or_cache', 'unknown'];
const autoApplyKinds = new Set(['merge_text', 'copy_file']);
const reviewPayloadClasses = new Set(['memory_knowledge', 'mcp_config']);

function isAutoApplicable(operation: ApplyOperation) {
  return !operation.requires_review && operation.safety_class === 'safe_config' && autoApplyKinds.has(operation.kind);
}

function isReviewPayloadApplicable(operation: ApplyOperation) {
  return operation.requires_review && reviewPayloadClasses.has(operation.safety_class) && ['import_memory', 'install_tool', 'merge_text', 'copy_file'].includes(operation.kind);
}

function isSelectableOperation(operation: ApplyOperation) {
  return isAutoApplicable(operation) || isReviewPayloadApplicable(operation);
}

function emptyCapabilities(): AdapterCapabilities {
  return {
    can_export_config: false,
    can_import_config: false,
    can_export_memory: false,
    can_import_memory: false,
    can_list_sessions: false,
    can_export_sessions: false,
    can_import_sessions: false,
    can_remap_session_project: false,
    requires_app_stopped_for_session_apply: false,
    supports_transactional_apply: false
  };
}

function hasDeclaredCapabilities(capabilities: AdapterCapabilities) {
  return Object.values(capabilities).some(Boolean);
}

export function App() {
  const [snapshot, setSnapshot] = useState<DeviceSnapshot | null>(null);
  const [remoteSnapshot, setRemoteSnapshot] = useState<DeviceSnapshot | null>(null);
  const [importedBundle, setImportedBundle] = useState<SyncBundle | null>(null);
  const [plan, setPlan] = useState<TransformPlan | null>(null);
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [journal, setJournal] = useState<OperationJournal | null>(null);
  const [sessionArchiveJournal, setSessionArchiveJournal] = useState<SessionArchiveImportJournal | null>(null);
  const [sessionStageJournal, setSessionStageJournal] = useState<SessionNativeImportStageJournal | null>(null);
  const [sessionNativeFileJournal, setSessionNativeFileJournal] = useState<SessionNativeFileImportJournal | null>(null);
  const [sessionReadinessReport, setSessionReadinessReport] = useState<SessionNativeImportReadinessReport | null>(null);
  const [nativeStoreReport, setNativeStoreReport] = useState<NativeSessionStoreDiscoveryReport | null>(null);
  const [journalHistory, setJournalHistory] = useState<StoredRecord[]>([]);
  const [sessionNativeFileJournalHistory, setSessionNativeFileJournalHistory] = useState<StoredRecord[]>([]);
  const [bundleManifest, setBundleManifest] = useState<SyncBundleManifest | null>(null);
  const [verifyErrors, setVerifyErrors] = useState<string[]>([]);
  const [selectedOperationIds, setSelectedOperationIds] = useState<string[]>([]);
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([]);
  const [selectedLocalSessionIds, setSelectedLocalSessionIds] = useState<string[]>([]);
  const [selectedLocalReviewPayloadKeys, setSelectedLocalReviewPayloadKeys] = useState<string[]>([]);
  const [reviewApplyAcknowledged, setReviewApplyAcknowledged] = useState(false);
  const [allowUnencryptedSensitiveExport, setAllowUnencryptedSensitiveExport] = useState(false);
  const [storeMessage, setStoreMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [bundlePath, setBundlePath] = useState('agent-sync-local.asbundle');
  const [exportPath, setExportPath] = useState('agent-sync-local.asbundle');
  const [targetProjectPath, setTargetProjectPath] = useState('');
  const [targetHomePath, setTargetHomePath] = useState('');
  const [backupDir, setBackupDir] = useState('agent-sync-backups');
  const [archiveStorePath, setArchiveStorePath] = useState('agent-sync-studio.sqlite');
  const [sessionStageDir, setSessionStageDir] = useState('agent-sync-session-staging');
  const [requireAgentsStopped, setRequireAgentsStopped] = useState(true);

  async function scan() {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<DeviceSnapshot>('scan_device', { maxDepth: 8, maxEntries: 5000 });
      setSnapshot(next);
      setTargetProjectPath(next.inputs.project);
      setPlan(null);
      setPreflight(null);
      setJournal(null);
      setSessionArchiveJournal(null);
      setSessionStageJournal(null);
      setSessionNativeFileJournal(null);
      setSessionReadinessReport(null);
      setNativeStoreReport(null);
      setBundleManifest(null);
      setStoreMessage(null);
      setSelectedLocalSessionIds([]);
      setSelectedLocalReviewPayloadKeys([]);
      setAllowUnencryptedSensitiveExport(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function createSelfPlan() {
    if (!snapshot) return;
    setBusy(true);
    setError(null);
    try {
      const nextPlan = await invoke<TransformPlan>('create_transform_plan_command', { from: snapshot, to: snapshot });
      await updatePlanState(nextPlan);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function exportBundle() {
    if (!snapshot) return;
    setBusy(true);
    setError(null);
    try {
      const manifest = await invoke<SyncBundleManifest>('export_bundle_file', {
        snapshot,
        output: exportPath,
        maxPayloadBytes: 1024 * 1024,
        selectedReviewPayloads: selectedLocalReviewPayloadKeys.map(payloadKeyToSelection),
        includeSessionPayloads: selectedLocalSessionIds.length > 0,
        selectedSessionIds: selectedLocalSessionIds,
        maxSessionPayloadBytes: 2 * 1024 * 1024,
        allowUnencryptedSensitivePayloads: allowUnencryptedSensitiveExport
      });
      setBundleManifest(manifest);
      setBundlePath(exportPath);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function importBundle() {
    setBusy(true);
    setError(null);
    setVerifyErrors([]);
    try {
      const bundle = await invoke<SyncBundle>('read_bundle', { path: bundlePath });
      const errors = await invoke<string[]>('verify_bundle_command', { bundle });
      setImportedBundle(bundle);
      setRemoteSnapshot(bundle.source_snapshot);
      setVerifyErrors(errors);
      setSelectedSessionIds(bundle.session_archives.map((archive) => archive.session.id));
      setPlan(null);
      setPreflight(null);
      setJournal(null);
      setSessionArchiveJournal(null);
      setSessionStageJournal(null);
      setSessionNativeFileJournal(null);
      setSessionReadinessReport(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function createImportPlan() {
    if (!snapshot || !remoteSnapshot) return;
    setBusy(true);
    setError(null);
    try {
      const nextPlan = await invoke<TransformPlan>('create_transform_plan_command', {
        from: remoteSnapshot,
        to: snapshot,
        targetPlatform: snapshot.platform.os
      });
      await updatePlanState(nextPlan);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function applySelectedSafePayloads() {
    if (!importedBundle || !plan) return;
    setBusy(true);
    setError(null);
    try {
      const selectedPlan = withSelectedOperations(plan, selectedOperationIds);
      const nextPreflight = await invoke<PreflightReport>('preflight_plan', { plan: selectedPlan });
      setPreflight(nextPreflight);
      const nextJournal = await invoke<OperationJournal>('apply_safe_payloads_command', {
        bundle: importedBundle,
        plan: selectedPlan,
        targetProject: targetProjectPath || undefined,
        backupDir: backupDir || 'agent-sync-backups',
        acknowledgeReviewRequired: reviewApplyAcknowledged
      });
      setJournal(nextJournal);
      const id = await invoke<string>('save_operation_journal', {
        dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
        journal: nextJournal
      });
      setStoreMessage(`apply journal saved: ${id}`);
      await refreshJournalHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function rollbackLastJournal() {
    if (!journal) return;
    setBusy(true);
    setError(null);
    try {
      const nextJournal = await invoke<OperationJournal>('rollback_journal_command', { journal });
      setJournal(nextJournal);
      const id = await invoke<string>('save_operation_journal', {
        dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
        journal: nextJournal
      });
      setStoreMessage(`rollback journal saved: ${id}`);
      await refreshJournalHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function importSelectedSessionArchives() {
    if (!importedBundle) return;
    setBusy(true);
    setError(null);
    try {
      const nextJournal = await invoke<SessionArchiveImportJournal>('import_session_archives_command', {
        bundle: importedBundle,
        dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
        selectedSessionIds,
        targetProject: targetProjectPath || undefined
      });
      setSessionArchiveJournal(nextJournal);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function stageSelectedSessionPayloads() {
    if (!importedBundle) return;
    setBusy(true);
    setError(null);
    try {
      const nextJournal = await invoke<SessionNativeImportStageJournal>('stage_session_native_import_command', {
        bundle: importedBundle,
        selectedSessionIds,
        targetProject: targetProjectPath || undefined,
        stagingDir: sessionStageDir || 'agent-sync-session-staging',
        rewriteProjectIdentity: true
      });
      setSessionStageJournal(nextJournal);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function checkSessionNativeImportReadiness() {
    if (!importedBundle) return;
    setBusy(true);
    setError(null);
    try {
      const report = await invoke<SessionNativeImportReadinessReport>('session_native_import_readiness_command', {
        bundle: importedBundle,
        targetSnapshot: snapshot ?? undefined,
        selectedSessionIds,
        requireAgentsStopped
      });
      setSessionReadinessReport(report);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function importSelectedSessionPayloadsToNativeFiles() {
    if (!importedBundle) return;
    setBusy(true);
    setError(null);
    try {
      const nextJournal = await invoke<SessionNativeFileImportJournal>('import_session_payloads_to_native_files_command', {
        bundle: importedBundle,
        selectedSessionIds,
        targetHome: targetHomePath || undefined,
        targetProject: targetProjectPath || undefined,
        backupDir: backupDir || 'agent-sync-backups',
        rewriteProjectIdentity: true,
        requireAgentsStopped
      });
      setSessionNativeFileJournal(nextJournal);
      const id = await invoke<string>('save_session_native_file_import_journal', {
        dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
        journal: nextJournal
      });
      setStoreMessage(`native file import journal saved: ${id}`);
      await refreshSessionNativeFileJournalHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function rollbackSessionNativeFileImport() {
    if (!sessionNativeFileJournal) return;
    setBusy(true);
    setError(null);
    try {
      const nextJournal = await invoke<SessionNativeFileImportJournal>('rollback_session_native_file_import_journal_command', {
        journal: sessionNativeFileJournal
      });
      setSessionNativeFileJournal(nextJournal);
      const id = await invoke<string>('save_session_native_file_import_journal', {
        dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
        journal: nextJournal
      });
      setStoreMessage(`native file import rollback journal saved: ${id}`);
      await refreshSessionNativeFileJournalHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function saveSnapshot() {
    if (!snapshot) return;
    setBusy(true);
    setError(null);
    try {
      const id = await invoke<string>('save_snapshot_to_store', {
        dbPath: 'agent-sync-studio.sqlite',
        snapshot
      });
      setStoreMessage(`snapshot saved: ${id}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function discoverNativeSessionStores() {
    if (!snapshot) return;
    setBusy(true);
    setError(null);
    try {
      const report = await invoke<NativeSessionStoreDiscoveryReport>('discover_native_session_stores_command', {
        snapshot,
        targetHome: targetHomePath || undefined,
        targetProject: targetProjectPath || undefined,
        maxSchemaTables: 20
      });
      setNativeStoreReport(report);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function refreshJournalHistory() {
    const rows = await invoke<StoredRecord[]>('list_store_records', {
      dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
      kind: 'apply_journal'
    });
    setJournalHistory(rows);
  }

  async function refreshSessionNativeFileJournalHistory() {
    const rows = await invoke<StoredRecord[]>('list_store_records', {
      dbPath: archiveStorePath || 'agent-sync-studio.sqlite',
      kind: 'session_native_file_import_journal'
    });
    setSessionNativeFileJournalHistory(rows);
  }

  async function loadJournalFromHistory(record: StoredRecord) {
    setError(null);
    try {
      const parsed = JSON.parse(record.json) as OperationJournal;
      setJournal(parsed);
      setStoreMessage(`apply journal loaded: ${record.id}`);
    } catch (err) {
      setError(`Failed to parse stored journal ${record.id}: ${String(err)}`);
    }
  }

  async function loadSessionNativeFileJournalFromHistory(record: StoredRecord) {
    setError(null);
    try {
      const parsed = JSON.parse(record.json) as SessionNativeFileImportJournal;
      setSessionNativeFileJournal(parsed);
      setStoreMessage(`native file import journal loaded: ${record.id}`);
    } catch (err) {
      setError(`Failed to parse stored native file import journal ${record.id}: ${String(err)}`);
    }
  }

  async function chooseExportPath() {
    const selected = await save({
      defaultPath: exportPath,
      filters: [{ name: 'Agent Sync Bundle', extensions: ['asbundle', 'json'] }]
    });
    if (selected) setExportPath(selected);
  }

  async function chooseBundlePath() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Agent Sync Bundle', extensions: ['asbundle', 'json'] }]
    });
    const path = singlePath(selected);
    if (path) setBundlePath(path);
  }

  async function chooseTargetProject() {
    const selected = await open({ directory: true, multiple: false });
    const path = singlePath(selected);
    if (path) setTargetProjectPath(path);
  }

  async function chooseTargetHome() {
    const selected = await open({ directory: true, multiple: false });
    const path = singlePath(selected);
    if (path) setTargetHomePath(path);
  }

  async function chooseBackupDir() {
    const selected = await open({ directory: true, multiple: false });
    const path = singlePath(selected);
    if (path) setBackupDir(path);
  }

  async function chooseSessionStageDir() {
    const selected = await open({ directory: true, multiple: false });
    const path = singlePath(selected);
    if (path) setSessionStageDir(path);
  }

  async function updatePlanState(nextPlan: TransformPlan) {
    const autoIds = nextPlan.operations.filter(isAutoApplicable).map((operation) => operation.id);
    const nextPreflight = await invoke<PreflightReport>('preflight_plan', { plan: { ...nextPlan, operations: nextPlan.operations.filter((operation) => autoIds.includes(operation.id)) } });
    setPlan(nextPlan);
    setSelectedOperationIds(autoIds);
    setPreflight(nextPreflight);
    setJournal(null);
  }

  function toggleOperation(id: string) {
    setSelectedOperationIds((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]);
  }

  function toggleSession(id: string) {
    setSelectedSessionIds((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]);
    setSessionReadinessReport(null);
  }

  function toggleLocalSession(id: string) {
    setSelectedLocalSessionIds((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]);
  }

  function toggleLocalReviewPayload(key: string) {
    setSelectedLocalReviewPayloadKeys((current) => current.includes(key) ? current.filter((item) => item !== key) : [...current, key]);
  }

  const agents = snapshot?.agents ?? [];
  const detected = agents.filter((agent) => agent.detected);
  const totalSessions = useMemo(() => agents.reduce((count, agent) => count + agent.sessions.length, 0), [agents]);
  const localSessions = useMemo(() => agents.flatMap((agent) => agent.sessions.map((session) => ({ agent, session }))), [agents]);
  const exportableLocalSessions = useMemo(() => localSessions.filter(({ agent }) => agent.capabilities.can_export_sessions), [localSessions]);
  const localReviewPayloads = useMemo(
    () => agents.flatMap((agent) => agent.findings
      .filter((finding) => reviewPayloadClasses.has(finding.safety_class))
      .map((finding) => ({ agent, finding, key: payloadSelectionKey(agent.id, finding.portable_path) }))),
    [agents]
  );
  const sensitiveLocalPayloadSelected = selectedLocalReviewPayloadKeys.length > 0 || selectedLocalSessionIds.length > 0;
  const selectedOperations = useMemo(() => (plan ? plan.operations.filter((operation) => selectedOperationIds.includes(operation.id)) : []), [plan, selectedOperationIds]);
  const selectedReviewOperations = useMemo(() => selectedOperations.filter(isReviewPayloadApplicable), [selectedOperations]);
  const autoApplicableCount = plan?.operations.filter(isAutoApplicable).length ?? 0;
  const reviewApplicableCount = plan?.operations.filter(isReviewPayloadApplicable).length ?? 0;
  const importedSessionArchives = importedBundle?.session_archives ?? [];
  const selectedRemoteArchives = useMemo(
    () => importedSessionArchives.filter((archive) => selectedSessionIds.includes(archive.session.id)),
    [importedSessionArchives, selectedSessionIds]
  );
  const selectedRemotePayloadCount = selectedRemoteArchives.reduce((count, archive) => count + archive.payloads.length, 0);
  const targetAgentCapabilities = (agentId: string) => (snapshot?.agents.find((agent) => agent.id === agentId)?.capabilities ?? importedBundle?.source_snapshot.agents.find((agent) => agent.id === agentId)?.capabilities ?? emptyCapabilities());
  const selectedSessionsCanNativeImport = selectedRemoteArchives.every((archive) => {
    const capabilities = targetAgentCapabilities(archive.agent_id);
    return hasDeclaredCapabilities(capabilities)
      ? capabilities.can_import_sessions
      : (archive.session.import_capabilities?.import_as_archive ?? false);
  });
  const selectedSessionsRequireStopped = selectedRemoteArchives.some((archive) => {
    const capabilities = targetAgentCapabilities(archive.agent_id);
    return hasDeclaredCapabilities(capabilities)
      ? capabilities.requires_app_stopped_for_session_apply
      : (archive.session.import_capabilities?.requires_app_stopped ?? true);
  });
  const selectedSessionsSupportDbRemap = selectedRemoteArchives.every((archive) => {
    const capabilities = targetAgentCapabilities(archive.agent_id);
    return hasDeclaredCapabilities(capabilities) && capabilities.can_remap_session_project;
  });

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="logo">AS</span>
          <div>
            <strong>Agent Sync Studio</strong>
            <small>Tauri + Rust control plane</small>
          </div>
        </div>
        <nav>
          <a className="active">Dashboard</a>
          <a>Scan Center</a>
          <a>Bundle Import</a>
          <a>Project Mapping</a>
          <a>Session Library</a>
          <a>Memory & Tools</a>
          <a>Apply Center</a>
        </nav>
      </aside>

      <section className="content">
        <header className="hero">
          <div>
            <p className="eyebrow">Cross-platform agent state migration</p>
            <h1>把 agent 上下文作为产品状态来同步，而不是裸拷贝目录。</h1>
            <p className="subtitle">Scan → Import Bundle → Map Project → Select Payloads → Apply → Verify/Rollback</p>
          </div>
          <div className="actions vertical">
            <button onClick={scan} disabled={busy}>{busy ? 'Working…' : 'Scan this device'}</button>
            <button className="secondary" onClick={saveSnapshot} disabled={!snapshot || busy}>Save snapshot</button>
            <button className="secondary" onClick={createSelfPlan} disabled={!snapshot || busy}>Self preview</button>
          </div>
        </header>

        {error && <section className="alert">{error}</section>}
        {storeMessage && <section className="notice">{storeMessage}</section>}
        {bundleManifest && (
          <section className="notice">
            bundle exported: {bundleManifest.id} · selections {bundleManifest.selections.length} · redactions {bundleManifest.redactions.length}
          </section>
        )}
        {importedBundle && (
          <section className={verifyErrors.length === 0 ? 'notice' : 'alert'}>
            bundle imported: {importedBundle.manifest.id} · payloads {importedBundle.payloads.length} · session archives {importedBundle.session_archives.length} · verify {verifyErrors.length === 0 ? 'passed' : `${verifyErrors.length} errors`}
          </section>
        )}

        <section className="grid metrics">
          <Metric label="Detected agents" value={snapshot ? detected.length : '—'} />
          <Metric label="Findings" value={snapshot?.summary.findings ?? '—'} />
          <Metric label="Session surfaces" value={snapshot ? totalSessions : '—'} />
          <Metric label="Platform" value={snapshot ? `${snapshot.platform.os}/${snapshot.platform.arch}` : '—'} />
        </section>

        <section className="grid two">
          <section className="panel">
            <div className="panelTitle">
              <h2>Local target device</h2>
              <span>{snapshot?.generated_at ?? 'not scanned'}</span>
            </div>
            <div className="formGrid">
              <label>
                Target project path
                <input value={targetProjectPath} onChange={(event) => setTargetProjectPath(event.target.value)} placeholder="Scan first or paste target project path" />
              </label>
              <button className="secondary" onClick={chooseTargetProject} disabled={busy}>Choose target project…</button>
              <label>
                Target home path
                <input value={targetHomePath} onChange={(event) => setTargetHomePath(event.target.value)} placeholder="Blank = current OS HOME" />
              </label>
              <button className="secondary" onClick={chooseTargetHome} disabled={busy}>Choose target home…</button>
              <label>
                Backup directory
                <input value={backupDir} onChange={(event) => setBackupDir(event.target.value)} placeholder="agent-sync-backups" />
              </label>
              <button className="secondary" onClick={chooseBackupDir} disabled={busy}>Choose backup dir…</button>
              <label>
                Archive store
                <input value={archiveStorePath} onChange={(event) => setArchiveStorePath(event.target.value)} placeholder="agent-sync-studio.sqlite" />
              </label>
              <button className="secondary" onClick={refreshJournalHistory} disabled={busy}>Refresh apply journals</button>
              <button className="secondary" onClick={refreshSessionNativeFileJournalHistory} disabled={busy}>Refresh native import journals</button>
              <button className="secondary" onClick={discoverNativeSessionStores} disabled={!snapshot || busy}>Discover native stores</button>
              <label>
                Session staging directory
                <input value={sessionStageDir} onChange={(event) => setSessionStageDir(event.target.value)} placeholder="agent-sync-session-staging" />
              </label>
              <button className="secondary" onClick={chooseSessionStageDir} disabled={busy}>Choose staging dir…</button>
            </div>
            {snapshot && (
              <div className="chips spaced">
                {safetyOrder.map((key) => (
                  <span key={key} className={`chip ${key}`}>{key}: {snapshot.summary.by_safety_class[key] ?? 0}</span>
                ))}
              </div>
            )}
          </section>

          <section className="panel">
            <div className="panelTitle">
              <h2>Bundle exchange</h2>
              <span>{remoteSnapshot ? `${remoteSnapshot.platform.os}/${remoteSnapshot.platform.arch}` : 'no source bundle'}</span>
            </div>
            <div className="formGrid">
              <label>
                Export path
                <input value={exportPath} onChange={(event) => setExportPath(event.target.value)} />
              </label>
              <button className="secondary" onClick={chooseExportPath} disabled={busy}>Choose export path…</button>
              <button className="secondary" onClick={exportBundle} disabled={!snapshot || busy || (sensitiveLocalPayloadSelected && !allowUnencryptedSensitiveExport)}>Export local bundle</button>
              <label>
                Import bundle path
                <input value={bundlePath} onChange={(event) => setBundlePath(event.target.value)} />
              </label>
              <button className="secondary" onClick={chooseBundlePath} disabled={busy}>Choose bundle…</button>
              <button className="secondary" onClick={importBundle} disabled={busy || !bundlePath}>Import + verify bundle</button>
              <button onClick={createImportPlan} disabled={!snapshot || !remoteSnapshot || busy}>Plan remote → local</button>
            </div>
            {sensitiveLocalPayloadSelected && (
              <label className="ackBox">
                <input type="checkbox" checked={allowUnencryptedSensitiveExport} onChange={(event) => setAllowUnencryptedSensitiveExport(event.target.checked)} />
                I understand selected memory/MCP or raw session payloads are currently exported as unencrypted bundle payloads. Use only trusted storage/transport until real bundle encryption is implemented.
              </label>
            )}
          </section>
        </section>

        <section className="panel">
          <div className="panelTitle">
            <h2>Local sessions for next bundle</h2>
            <span>{selectedLocalSessionIds.length}/{localSessions.length} selected for raw payload export</span>
          </div>
          {localSessions.length ? (
            <div className="stack">
              <div className="chips">
                <button className="secondary smallButton" onClick={() => setSelectedLocalSessionIds(exportableLocalSessions.map(({ session }) => session.id))} disabled={busy}>Select exportable sessions</button>
                <button className="secondary smallButton" onClick={() => setSelectedLocalSessionIds([])} disabled={busy}>Clear</button>
              </div>
              <div className="operationTable compact">
                {localSessions.slice(0, 120).map(({ agent, session }) => (
                  <label key={session.id} className="operationItem">
                    <input type="checkbox" checked={selectedLocalSessionIds.includes(session.id)} disabled={!agent.capabilities.can_export_sessions} onChange={() => toggleLocalSession(session.id)} />
                    <span>
                      <strong>{agent.name}</strong> · {session.title ?? session.id}
                      <small>{session.visibility} · {session.content_policy} · {agent.capabilities.can_export_sessions ? 'payload is included only if checked before Export local bundle' : 'adapter cannot export sessions yet'}</small>
                    </span>
                  </label>
                ))}
              </div>
            </div>
          ) : (
            <p className="empty">Scan with session depth to discover local Codex/Claude session files. Raw payload export is opt-in per session.</p>
          )}
        </section>

        <section className="panel">
          <div className="panelTitle">
            <h2>Memory / MCP payloads for next bundle</h2>
            <span>{selectedLocalReviewPayloadKeys.length}/{localReviewPayloads.length} selected for explicit review export</span>
          </div>
          {localReviewPayloads.length ? (
            <div className="stack">
              <div className="chips">
                <button className="secondary smallButton" onClick={() => setSelectedLocalReviewPayloadKeys(localReviewPayloads.map((item) => item.key))} disabled={busy}>Select all review payloads</button>
                <button className="secondary smallButton" onClick={() => setSelectedLocalReviewPayloadKeys([])} disabled={busy}>Clear</button>
              </div>
              <div className="operationTable compact">
                {localReviewPayloads.slice(0, 160).map(({ agent, finding, key }) => (
                  <label key={key} className="operationItem">
                    <input type="checkbox" checked={selectedLocalReviewPayloadKeys.includes(key)} onChange={() => toggleLocalReviewPayload(key)} />
                    <span>
                      <strong>{agent.name}</strong> · {finding.portable_path}
                      <small>{finding.safety_class} · {finding.risk} · payload is included only if checked before Export local bundle</small>
                      <small>{finding.reason}</small>
                    </span>
                  </label>
                ))}
              </div>
            </div>
          ) : (
            <p className="empty">Scan to discover memory/rules/prompts and MCP config. These review-required payloads are never exported unless explicitly checked.</p>
          )}
        </section>

        <section className="panel">
          <div className="panelTitle">
            <h2>Session Library</h2>
            <span>{selectedSessionIds.length}/{importedSessionArchives.length} selected · {selectedRemotePayloadCount} payloads</span>
          </div>
          {importedSessionArchives.length ? (
            <div className="stack">
              <div className="operationTable">
                {importedSessionArchives.slice(0, 100).map((archive) => (
                  <label key={archive.session.id} className="operationItem">
                    <input type="checkbox" checked={selectedSessionIds.includes(archive.session.id)} onChange={() => toggleSession(archive.session.id)} />
                    <span>
                      <strong>{archive.agent_name}</strong> · {archive.session.title ?? archive.session.id}
                      <small>
                        {archive.session.visibility} · {archive.session.content_policy} · source project {archive.source_project?.canonical_path ?? 'unknown'} · {archive.payload_included ? `${archive.payloads.length} payload(s) included` : 'metadata-only'}
                      </small>
                      <small>{archive.import_note}</small>
                    </span>
                  </label>
                ))}
              </div>
              <button onClick={importSelectedSessionArchives} disabled={!importedBundle || selectedSessionIds.length === 0 || busy}>
                Import selected session archives
              </button>
              {!selectedSessionsCanNativeImport && (
                <div className="preflight fail">
                  Selected agent adapter cannot import native sessions on this target yet.
                </div>
              )}
              {!selectedSessionsSupportDbRemap && (
                <div className="notice">
                  Current adapter capability is native-file import only: project text paths can be rewritten, but Codex/Claude DB/index project remap is not claimed.
                </div>
              )}
              <button className="secondary" onClick={checkSessionNativeImportReadiness} disabled={!importedBundle || selectedSessionIds.length === 0 || busy}>
                Check native import readiness
              </button>
              {sessionReadinessReport && (
                <div className={sessionReadinessReport.blocked === 0 && sessionReadinessReport.blockers.length === 0 ? 'preflight pass' : 'preflight fail'}>
                  Native import readiness: ready {sessionReadinessReport.ready}/{sessionReadinessReport.selected} · blocked {sessionReadinessReport.blocked}
                  {sessionReadinessReport.blockers.length > 0 && <small>{sessionReadinessReport.blockers.join(' / ')}</small>}
                  {sessionReadinessReport.warnings.length > 0 && <small>{sessionReadinessReport.warnings.join(' / ')}</small>}
                  <ul>
                    {sessionReadinessReport.entries.map((entry) => (
                      <li key={entry.session_id}>
                        {entry.ready ? 'ready' : 'blocked'} · {entry.agent_name} · {entry.title ?? entry.session_id} · payloads {entry.payloads} · {entry.can_remap_session_project ? 'DB/index remap supported' : 'file rewrite only'}
                        <small>{entry.note}</small>
                        {entry.blockers.length > 0 && <small>blockers: {entry.blockers.join(' / ')}</small>}
                        {entry.warnings.length > 0 && <small>warnings: {entry.warnings.join(' / ')}</small>}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              <button className="secondary" onClick={stageSelectedSessionPayloads} disabled={!importedBundle || selectedSessionIds.length === 0 || busy || !selectedSessionsCanNativeImport}>
                Stage selected native session payloads
              </button>
              {selectedSessionsRequireStopped && (
                <label className="ackBox">
                  <input type="checkbox" checked={requireAgentsStopped} onChange={(event) => {
                    setRequireAgentsStopped(event.target.checked);
                    setSessionReadinessReport(null);
                  }} />
                  Require Codex/Claude to be stopped before writing native session files. Uncheck only for an explicit manual override.
                </label>
              )}
              <button onClick={importSelectedSessionPayloadsToNativeFiles} disabled={!importedBundle || selectedSessionIds.length === 0 || selectedRemotePayloadCount === 0 || busy || !selectedSessionsCanNativeImport}>
                Import selected payloads to native files
              </button>
            </div>
          ) : (
            <p className="empty">Import a bundle to choose Codex/Claude session archives. Current archive import is metadata-only unless an adapter later includes raw payloads.</p>
          )}
        </section>

        <section className="grid two">
          <section className="panel">
            <h2>Agents</h2>
            <div className="list">
              {agents.map((agent) => (
                <article key={agent.id} className="row">
                  <div>
                    <strong>{agent.name}</strong>
                    <small>{agent.id} · {agent.findings.length} findings · {agent.sessions.length} session surfaces</small>
                    <small>
                      {agent.capabilities.can_export_config ? 'config export' : 'no config export'} · {agent.capabilities.can_import_config ? 'config import' : 'no config import'} · {agent.capabilities.can_import_sessions ? 'session file import' : 'no session import'} · {agent.capabilities.can_remap_session_project ? 'DB/index remap' : 'no DB/index remap'}
                    </small>
                  </div>
                  <span className={agent.detected ? 'status ok' : 'status'}>{agent.detected ? 'detected' : 'absent'}</span>
                </article>
              ))}
              {!snapshot && <p className="empty">Run a scan to populate the Rust-backed inventory.</p>}
            </div>
          </section>

          <section className="panel">
            <h2>Project mapping</h2>
            {plan?.project_mappings.length ? (
              <div className="list">
                {plan.project_mappings.map((mapping) => (
                  <article key={mapping.source_project_id} className="mappingCard">
                    <div className="mappingHeader">
                      <strong>{mapping.status}</strong>
                      <span>{mapping.strategy} · {mapping.confidence}%</span>
                    </div>
                    <small>from: {mapping.source_canonical_path}</small>
                    <small>to: {mapping.target_canonical_path ?? 'manual target required'}</small>
                    <small>{mapping.reason}</small>
                  </article>
                ))}
              </div>
            ) : (
              <p className="empty">Import a bundle and create a remote → local plan to see project identity mapping.</p>
            )}
          </section>
        </section>

        {nativeStoreReport && (
          <section className="panel">
            <div className="panelTitle">
              <h2>Native session store discovery</h2>
              <span>{nativeStoreReport.stores.length} DB/index candidates · schema only</span>
            </div>
            {nativeStoreReport.warnings.length > 0 && (
              <div className="notice">{nativeStoreReport.warnings.join(' / ')}</div>
            )}
            {nativeStoreReport.stores.length ? (
              <ul className="operationList">
                {nativeStoreReport.stores.slice(0, 60).map((store) => (
                  <li key={`${store.agent_id}:${store.portable_path}`}>
                    {store.agent_name} · {store.store_kind} · {store.portable_path} · {store.schema_status}
                    <small>{store.note}</small>
                    {store.tables.length > 0 && (
                      <ul>
                        {store.tables.map((table) => (
                          <li key={table.name}>{table.name}: {table.columns.join(', ') || 'no columns'}</li>
                        ))}
                      </ul>
                    )}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="empty">No Codex/Claude DB/index candidates were found in the current scan. Increase scan depth/entries if expected stores are missing.</p>
            )}
          </section>
        )}

        <section className="panel">
          <div className="panelTitle">
            <h2>Transform preview & apply</h2>
              <span>{plan ? `${selectedOperations.length}/${plan.operations.length} selected · ${autoApplicableCount} auto-safe · ${reviewApplicableCount} review-applicable` : 'no plan'}</span>
          </div>
          {plan ? (
            <div className="stack">
              <div className="chips">
                <span className="chip safe_config">safe {plan.summary.safe_candidates}</span>
                <span className="chip memory_knowledge">review {plan.summary.review_required}</span>
                <span className="chip secret_bearing">blocked {plan.summary.blocked}</span>
                <span className="chip">changed {plan.summary.changed}</span>
                <span className="chip">missing {plan.summary.missing_on_target}</span>
              </div>
              {preflight && (
                <div className={preflight.passed ? 'preflight pass' : 'preflight fail'}>
                  Preflight selected ops: {preflight.passed ? 'passed' : 'blocked'} · review {preflight.operations_requiring_review} · backups {preflight.operations_requiring_backup}
                  {preflight.blockers.length > 0 && <small>{preflight.blockers.join(' / ')}</small>}
                </div>
              )}
              <div className="operationTable">
                {plan.operations.slice(0, 50).map((operation) => {
                  const auto = isAutoApplicable(operation);
                  const review = isReviewPayloadApplicable(operation);
                  return (
                    <label key={operation.id} className={auto || review ? 'operationItem' : 'operationItem disabled'}>
                      <input type="checkbox" checked={selectedOperationIds.includes(operation.id)} disabled={!isSelectableOperation(operation)} onChange={() => toggleOperation(operation.id)} />
                      <span>
                        <strong>{operation.kind}</strong> · {operation.agent_id} · {operation.path}
                        <small>{operation.safety_class} · {auto ? 'auto-applicable with backup' : review ? 'explicit review payload apply' : 'adapter-specific required'} · {operation.rationale}</small>
                      </span>
                    </label>
                  );
                })}
                {plan.operations.length === 0 && <p className="empty">No operations for this preview.</p>}
              </div>
              {selectedReviewOperations.length > 0 && (
                <label className="ackBox">
                  <input type="checkbox" checked={reviewApplyAcknowledged} onChange={(event) => setReviewApplyAcknowledged(event.target.checked)} />
                  I reviewed selected memory/MCP payloads and accept applying them with backup/checksum verification.
                </label>
              )}
              <button onClick={applySelectedSafePayloads} disabled={!importedBundle || selectedOperations.length === 0 || busy || (selectedReviewOperations.length > 0 && !reviewApplyAcknowledged)}>
                Apply selected payloads
              </button>
            </div>
          ) : (
            <p className="empty">Scan this device, import a bundle, then create a remote → local transform plan.</p>
          )}
        </section>

        {journal && (
          <section className="panel">
            <div className="panelTitle">
              <h2>Apply journal</h2>
              <span>{journal.id} · {journal.status}</span>
            </div>
            <button className="secondary" onClick={rollbackLastJournal} disabled={busy || journal.status === 'rolled_back'}>
              Rollback this journal
            </button>
            <ul className="operationList">
              {journal.operations.map((entry) => (
                <li key={entry.operation.id}>{entry.status} · {entry.operation.path} · {entry.message ?? 'no message'}</li>
              ))}
            </ul>
          </section>
        )}
        <section className="panel">
          <div className="panelTitle">
            <h2>Apply journal history</h2>
            <span>{journalHistory.length} stored in {archiveStorePath || 'agent-sync-studio.sqlite'}</span>
          </div>
          {journalHistory.length ? (
            <ul className="operationList">
              {journalHistory.slice(0, 20).map((record) => {
                const summary = storedJournalSummary(record);
                return (
                  <li key={record.id}>
                    <button className="secondary smallButton" onClick={() => loadJournalFromHistory(record)} disabled={busy}>Load</button>{' '}
                    {record.id} · {summary.status} · ops {summary.operations} · updated {record.updated_at}
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="empty">Apply journals are saved automatically after apply/rollback. Refresh to load rollback points from the local SQLite store.</p>
          )}
        </section>
        {sessionArchiveJournal && (
          <section className="panel">
            <div className="panelTitle">
              <h2>Session archive journal</h2>
              <span>{sessionArchiveJournal.id} · {sessionArchiveJournal.status}</span>
            </div>
            <div className="chips">
              <span className="chip safe_config">imported {sessionArchiveJournal.imported}</span>
              <span className="chip">selected {sessionArchiveJournal.selected}</span>
              <span className="chip">skipped {sessionArchiveJournal.skipped}</span>
            </div>
            <ul className="operationList">
              {sessionArchiveJournal.records.map((record) => (
                <li key={record.record_id}>{record.agent_name} · {record.title ?? record.session_id} · target {record.target_project ?? 'none'} · {record.note}</li>
              ))}
            </ul>
          </section>
        )}
        {sessionStageJournal && (
          <section className="panel">
            <div className="panelTitle">
              <h2>Native import staging journal</h2>
              <span>{sessionStageJournal.id} · {sessionStageJournal.status}</span>
            </div>
            <div className="chips">
              <span className="chip safe_config">staged {sessionStageJournal.staged}</span>
              <span className="chip">selected {sessionStageJournal.selected}</span>
              <span className="chip">skipped {sessionStageJournal.skipped}</span>
            </div>
            <ul className="operationList">
              {sessionStageJournal.records.map((record) => (
                <li key={record.session_id}>
                  {record.agent_name} · {record.title ?? record.session_id} · payloads {record.written_payloads.length} · target {record.target_project ?? 'none'} · {record.note}
                  <ul>
                    {record.written_payloads.map((payload) => (
                      <li key={payload.staged_path}>{payload.project_identity_rewritten ? 'rewritten' : 'copied'} · {payload.portable_path} → {payload.staged_path}</li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        )}
        {sessionNativeFileJournal && (
          <section className="panel">
            <div className="panelTitle">
              <h2>Native file import journal</h2>
              <span>{sessionNativeFileJournal.id} · {sessionNativeFileJournal.status}</span>
            </div>
            <button className="secondary" onClick={rollbackSessionNativeFileImport} disabled={busy || sessionNativeFileJournal.status === 'rolled_back'}>
              Rollback native file import
            </button>
            <div className="chips">
              <span className="chip safe_config">imported {sessionNativeFileJournal.imported}</span>
              <span className="chip">selected {sessionNativeFileJournal.selected}</span>
              <span className="chip">skipped {sessionNativeFileJournal.skipped}</span>
            </div>
            {sessionNativeFileJournal.blockers.length > 0 && (
              <div className="preflight fail">
                Native import blocked
                <small>{sessionNativeFileJournal.blockers.join(' / ')}</small>
              </div>
            )}
            <ul className="operationList">
              {sessionNativeFileJournal.records.map((record) => (
                <li key={record.session_id}>
                  {record.agent_name} · {record.title ?? record.session_id} · payloads {record.written_payloads.length} · target {record.target_project ?? 'none'} · {record.note}
                  <ul>
                    {record.written_payloads.map((payload) => (
                      <li key={`${record.session_id}:${payload.portable_path}`}>
                        {payload.status} · {payload.project_identity_rewritten ? 'rewritten' : 'copied'} · {payload.portable_path} → {payload.target_path || 'blocked'}{payload.backup_path ? ` · backup ${payload.backup_path}` : ''}{payload.message ? ` · ${payload.message}` : ''}
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        )}
        <section className="panel">
          <div className="panelTitle">
            <h2>Native file import journal history</h2>
            <span>{sessionNativeFileJournalHistory.length} stored in {archiveStorePath || 'agent-sync-studio.sqlite'}</span>
          </div>
          {sessionNativeFileJournalHistory.length ? (
            <ul className="operationList">
              {sessionNativeFileJournalHistory.slice(0, 20).map((record) => {
                const summary = storedSessionNativeFileJournalSummary(record);
                return (
                  <li key={record.id}>
                    <button className="secondary smallButton" onClick={() => loadSessionNativeFileJournalFromHistory(record)} disabled={busy}>Load</button>{' '}
                    {record.id} · {summary.status} · imported {summary.imported} · records {summary.records} · updated {record.updated_at}
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="empty">Native file import journals are saved automatically after import/rollback. Refresh to load rollback points from the local SQLite store.</p>
          )}
        </section>
      </section>
    </main>
  );
}

function withSelectedOperations(plan: TransformPlan, selectedIds: string[]): TransformPlan {
  const selected = plan.operations.filter((operation) => selectedIds.includes(operation.id));
  return {
    ...plan,
    operations: selected,
    summary: {
      ...plan.summary,
      safe_candidates: selected.filter((operation) => !operation.requires_review).length,
      review_required: selected.filter((operation) => operation.requires_review).length
    }
  };
}

function singlePath(path: string | string[] | null): string | null {
  if (Array.isArray(path)) return path[0] ?? null;
  return path;
}

function payloadSelectionKey(agentId: string, portablePath: string): string {
  return `${encodeURIComponent(agentId)}:${encodeURIComponent(portablePath)}`;
}

function payloadKeyToSelection(key: string): PayloadSelectionRef {
  const [agentId = '', portablePath = ''] = key.split(':');
  return {
    agent_id: decodeURIComponent(agentId),
    portable_path: decodeURIComponent(portablePath)
  };
}

function storedJournalSummary(record: StoredRecord): { status: string; operations: number } {
  try {
    const journal = JSON.parse(record.json) as OperationJournal;
    return { status: journal.status, operations: journal.operations.length };
  } catch {
    return { status: 'invalid_json', operations: 0 };
  }
}

function storedSessionNativeFileJournalSummary(record: StoredRecord): { status: string; imported: number; records: number } {
  try {
    const journal = JSON.parse(record.json) as SessionNativeFileImportJournal;
    return { status: journal.status, imported: journal.imported, records: journal.records.length };
  } catch {
    return { status: 'invalid_json', imported: 0, records: 0 };
  }
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <section className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </section>
  );
}
