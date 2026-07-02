import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

type SnapshotSummary = {
  agents_detected: number;
  findings: number;
  by_safety_class: Record<string, number>;
  by_risk: Record<string, number>;
};

type AgentSnapshot = {
  id: string;
  name: string;
  detected: boolean;
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

type SessionRecord = {
  id: string;
  agent_id: string;
  title?: string;
  source_project?: string;
  visibility: string;
  content_policy: string;
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
  const [bundleManifest, setBundleManifest] = useState<SyncBundleManifest | null>(null);
  const [verifyErrors, setVerifyErrors] = useState<string[]>([]);
  const [selectedOperationIds, setSelectedOperationIds] = useState<string[]>([]);
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([]);
  const [selectedLocalSessionIds, setSelectedLocalSessionIds] = useState<string[]>([]);
  const [selectedLocalReviewPayloadKeys, setSelectedLocalReviewPayloadKeys] = useState<string[]>([]);
  const [reviewApplyAcknowledged, setReviewApplyAcknowledged] = useState(false);
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
      setBundleManifest(null);
      setStoreMessage(null);
      setSelectedLocalSessionIds([]);
      setSelectedLocalReviewPayloadKeys([]);
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
        maxSessionPayloadBytes: 2 * 1024 * 1024
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
        rewriteProjectIdentity: true
      });
      setSessionNativeFileJournal(nextJournal);
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
  const localReviewPayloads = useMemo(
    () => agents.flatMap((agent) => agent.findings
      .filter((finding) => reviewPayloadClasses.has(finding.safety_class))
      .map((finding) => ({ agent, finding, key: payloadSelectionKey(agent.id, finding.portable_path) }))),
    [agents]
  );
  const selectedOperations = useMemo(() => (plan ? plan.operations.filter((operation) => selectedOperationIds.includes(operation.id)) : []), [plan, selectedOperationIds]);
  const selectedReviewOperations = useMemo(() => selectedOperations.filter(isReviewPayloadApplicable), [selectedOperations]);
  const autoApplicableCount = plan?.operations.filter(isAutoApplicable).length ?? 0;
  const reviewApplicableCount = plan?.operations.filter(isReviewPayloadApplicable).length ?? 0;
  const importedSessionArchives = importedBundle?.session_archives ?? [];
  const selectedRemotePayloadCount = importedSessionArchives
    .filter((archive) => selectedSessionIds.includes(archive.session.id))
    .reduce((count, archive) => count + archive.payloads.length, 0);

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
              <button className="secondary" onClick={exportBundle} disabled={!snapshot || busy}>Export local bundle</button>
              <label>
                Import bundle path
                <input value={bundlePath} onChange={(event) => setBundlePath(event.target.value)} />
              </label>
              <button className="secondary" onClick={chooseBundlePath} disabled={busy}>Choose bundle…</button>
              <button className="secondary" onClick={importBundle} disabled={busy || !bundlePath}>Import + verify bundle</button>
              <button onClick={createImportPlan} disabled={!snapshot || !remoteSnapshot || busy}>Plan remote → local</button>
            </div>
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
                <button className="secondary smallButton" onClick={() => setSelectedLocalSessionIds(localSessions.map(({ session }) => session.id))} disabled={busy}>Select all local sessions</button>
                <button className="secondary smallButton" onClick={() => setSelectedLocalSessionIds([])} disabled={busy}>Clear</button>
              </div>
              <div className="operationTable compact">
                {localSessions.slice(0, 120).map(({ agent, session }) => (
                  <label key={session.id} className="operationItem">
                    <input type="checkbox" checked={selectedLocalSessionIds.includes(session.id)} onChange={() => toggleLocalSession(session.id)} />
                    <span>
                      <strong>{agent.name}</strong> · {session.title ?? session.id}
                      <small>{session.visibility} · {session.content_policy} · payload is included only if checked before Export local bundle</small>
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
              <button className="secondary" onClick={stageSelectedSessionPayloads} disabled={!importedBundle || selectedSessionIds.length === 0 || busy}>
                Stage selected native session payloads
              </button>
              <button onClick={importSelectedSessionPayloadsToNativeFiles} disabled={!importedBundle || selectedSessionIds.length === 0 || selectedRemotePayloadCount === 0 || busy}>
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
            <div className="chips">
              <span className="chip safe_config">imported {sessionNativeFileJournal.imported}</span>
              <span className="chip">selected {sessionNativeFileJournal.selected}</span>
              <span className="chip">skipped {sessionNativeFileJournal.skipped}</span>
            </div>
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

function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <section className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </section>
  );
}
