use agent_sync_core::SessionRecord;
use agent_sync_scan::{ScanOptions, scan_device};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterCapabilities {
    pub can_export_config: bool,
    pub can_import_config: bool,
    pub can_export_memory: bool,
    pub can_import_memory: bool,
    pub can_list_sessions: bool,
    pub can_export_sessions: bool,
    pub can_import_sessions: bool,
    pub can_remap_session_project: bool,
    pub requires_app_stopped_for_session_apply: bool,
    pub supports_transactional_apply: bool,
}

pub fn capabilities() -> AdapterCapabilities {
    AdapterCapabilities {
        can_export_config: true,
        can_import_config: true,
        can_export_memory: true,
        can_import_memory: true,
        can_list_sessions: true,
        can_export_sessions: true,
        can_import_sessions: true,
        can_remap_session_project: true,
        requires_app_stopped_for_session_apply: true,
        supports_transactional_apply: true,
    }
}

pub fn list_sessions(options: ScanOptions) -> agent_sync_core::Result<Vec<SessionRecord>> {
    let mut scoped = options;
    scoped.agents = vec!["claude".to_string()];
    let snapshot = scan_device(scoped)?;
    Ok(snapshot
        .agents
        .into_iter()
        .find(|agent| agent.id == "claude")
        .map(|agent| agent.sessions)
        .unwrap_or_default())
}
