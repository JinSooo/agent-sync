use agent_sync_core::{AdapterCapabilities, SessionRecord};
use agent_sync_scan::{ScanOptions, scan_device};

pub fn capabilities() -> AdapterCapabilities {
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

pub fn list_sessions(options: ScanOptions) -> agent_sync_core::Result<Vec<SessionRecord>> {
    let mut scoped = options;
    scoped.agents = vec!["codex".to_string()];
    let snapshot = scan_device(scoped)?;
    Ok(snapshot
        .agents
        .into_iter()
        .find(|agent| agent.id == "codex")
        .map(|agent| agent.sessions)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_do_not_claim_db_index_remap() {
        let capabilities = capabilities();
        assert!(capabilities.can_import_sessions);
        assert!(!capabilities.can_remap_session_project);
        assert!(!capabilities.supports_transactional_apply);
        assert!(capabilities.requires_app_stopped_for_session_apply);
    }
}
