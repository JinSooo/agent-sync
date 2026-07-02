use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformPaths {
    pub home: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
}

pub fn current_platform_paths() -> PlatformPaths {
    PlatformPaths {
        home: std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        config_dir: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        data_dir: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunningAgentProcess {
    pub agent_id: String,
    pub pid: u32,
    pub executable: String,
    pub command: String,
}

pub fn detect_running_agent_processes(
    agent_ids: &[String],
) -> std::io::Result<Vec<RunningAgentProcess>> {
    #[cfg(windows)]
    {
        detect_running_agent_processes_windows(agent_ids)
    }
    #[cfg(not(windows))]
    {
        detect_running_agent_processes_unix(agent_ids)
    }
}

#[cfg(not(windows))]
fn detect_running_agent_processes_unix(
    agent_ids: &[String],
) -> std::io::Result<Vec<RunningAgentProcess>> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,args="])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("ps process listing failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let current_pid = std::process::id();
    let mut processes = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((pid_text, command)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid_text.trim().parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        let command = command.trim().to_string();
        let executable = command
            .split_whitespace()
            .next()
            .map(executable_basename)
            .unwrap_or_default();
        if let Some(agent_id) = matching_agent_process(&executable, &command, agent_ids) {
            processes.push(RunningAgentProcess {
                agent_id,
                pid,
                executable,
                command,
            });
        }
    }
    Ok(processes)
}

#[cfg(windows)]
fn detect_running_agent_processes_windows(
    agent_ids: &[String],
) -> std::io::Result<Vec<RunningAgentProcess>> {
    let output = std::process::Command::new("tasklist")
        .args(["/fo", "csv", "/nh"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("tasklist process listing failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let current_pid = std::process::id();
    let mut processes = Vec::new();
    for line in text.lines() {
        let fields = parse_tasklist_csv_line(line);
        if fields.len() < 2 {
            continue;
        }
        let executable = fields[0].trim_end_matches(".exe").to_string();
        let Ok(pid) = fields[1].parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        if let Some(agent_id) = matching_agent_process(&executable, &fields[0], agent_ids) {
            processes.push(RunningAgentProcess {
                agent_id,
                pid,
                executable: executable.clone(),
                command: fields[0].clone(),
            });
        }
    }
    Ok(processes)
}

fn executable_basename(value: &str) -> String {
    let normalized = value.trim_matches('"');
    std::path::Path::new(normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(normalized)
        .trim_end_matches(".exe")
        .to_string()
}

fn matching_agent_process(executable: &str, command: &str, agent_ids: &[String]) -> Option<String> {
    let executable_names = process_name_candidates(executable);
    let command_names = command_name_candidates(command);
    for agent_id in agent_ids {
        let aliases = agent_process_aliases(agent_id)
            .iter()
            .map(|alias| normalize_process_name(alias))
            .collect::<Vec<_>>();
        if executable_names
            .iter()
            .chain(command_names.iter())
            .any(|candidate| aliases.iter().any(|alias| alias == candidate))
        {
            return Some(agent_id.clone());
        }
    }
    None
}

fn agent_process_aliases(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        "codex" => &["codex", "codex-cli"],
        "claude" => &["claude", "claude-code", "claude code"],
        _ => &[],
    }
}

fn process_name_candidates(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let basename = executable_basename(value);
    push_process_candidate(&mut candidates, &basename);
    for component in value.trim_matches('"').split(['/', '\\']) {
        push_process_candidate(&mut candidates, component);
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn command_name_candidates(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .flat_map(process_name_candidates)
        .collect::<Vec<_>>()
}

fn push_process_candidate(candidates: &mut Vec<String>, value: &str) {
    let trimmed = value.trim().trim_matches('"').trim_end_matches(".exe");
    if trimmed.is_empty() {
        return;
    }
    candidates.push(normalize_process_name(trimmed));
    if let Some(stripped) = trimmed.strip_suffix(".js") {
        candidates.push(normalize_process_name(stripped));
    }
}

fn normalize_process_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn parse_tasklist_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_agent_process_aliases() {
        let agents = vec!["codex".to_string(), "claude".to_string()];
        assert_eq!(
            matching_agent_process("codex", "codex", &agents),
            Some("codex".into())
        );
        assert_eq!(
            matching_agent_process("codex-cli", "codex-cli", &agents),
            Some("codex".into())
        );
        assert_eq!(
            matching_agent_process("Claude Code", "Claude Code", &agents),
            Some("claude".into())
        );
        assert_eq!(
            matching_agent_process("agent-sync-studio", "agent-sync-studio", &agents),
            None
        );
    }
}
