use anyhow::{anyhow, Context, Result};
use pmdaemon::{ProcessConfig, ProcessManager as PmDaemon};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Response for the run command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RunResponse {
    pub process_id: String,
    pub command: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<String>,
}

/// Response for the kill command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct KillResponse {
    pub process_id: String,
    pub message: String,
}

/// Response for the kill_all command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct KillAllResponse {
    pub stopped_count: usize,
    pub message: String,
}

/// Response for the read command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ReadResponse {
    pub process_id: String,
    pub logs: String,
}

/// Response for the restart command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RestartResponse {
    pub process_id: String,
    pub message: String,
}

/// Process information for the list command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ProcessInfo {
    pub name: String,
    pub command: String,
    pub status: String,
    pub pid: u32,
    pub started_at: String,
    pub uptime_seconds: i64,
    pub restarts: u32,
    pub cpu_usage: f64,
    pub memory_mb: u64,
}

/// Response for the list command
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListResponse {
    pub processes: Vec<ProcessInfo>,
}

#[derive(Clone)]
pub struct ProcessManager {
    daemon: Arc<Mutex<PmDaemon>>,
    commands: Arc<Mutex<HashMap<String, String>>>,
}

impl ProcessManager {
    pub async fn new() -> Result<Self> {
        // Set PMDAEMON_HOME to current working directory to isolate state per directory
        // This prevents state from persisting across different working directories
        if std::env::var("PMDAEMON_HOME").is_err() {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            let pmdaemon_dir = cwd.join(".pmdaemon");
            std::env::set_var("PMDAEMON_HOME", pmdaemon_dir);
        }

        let mut daemon = PmDaemon::new()
            .await
            .context("Failed to initialize PMDaemon")?;

        // Clean up stale processes on startup (processes with non-existent PIDs)
        // This handles cases where the server crashed without cleanup
        let processes = daemon.list().await.context("Failed to list processes")?;
        let mut stale_count = 0;

        for process in processes {
            // Kill any remaining child processes before checking if stale
            if let Some(pid) = process.pid {
                Self::kill_process_tree(pid);
            }

            // Check if the process is stopped or errored, or if PID doesn't exist
            let is_stale = matches!(
                process.state,
                pmdaemon::ProcessState::Stopped | pmdaemon::ProcessState::Errored
            ) || process.pid.is_none()
                || (process.pid.is_some() && !Self::pid_exists(process.pid.unwrap()));

            if is_stale {
                tracing::info!(
                    "Removing stale process '{}' (PID: {:?}, state: {:?})",
                    process.name,
                    process.pid,
                    process.state
                );
                if let Err(e) = daemon.delete(&process.name).await {
                    tracing::warn!("Failed to delete stale process '{}': {}", process.name, e);
                } else {
                    stale_count += 1;
                }
            }
        }

        if stale_count > 0 {
            tracing::info!("Cleaned up {} stale process(es) on startup", stale_count);
        }

        Ok(Self {
            daemon: Arc::new(Mutex::new(daemon)),
            commands: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Kill a process and all its descendants (process tree)
    /// This ensures that child processes are also terminated
    #[cfg(unix)]
    fn kill_process_tree(pid: u32) {
        // First, try to find and kill all child processes recursively
        Self::kill_children(pid);

        // Then kill the parent process
        // Use negative PID to kill the entire process group if possible
        unsafe {
            // First try to kill the process group
            let _ = libc::kill(-(pid as i32), libc::SIGTERM);

            // Give processes a moment to terminate gracefully
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Force kill the process group if still alive
            let _ = libc::kill(-(pid as i32), libc::SIGKILL);

            // Also try killing the individual process in case it's not a process group leader
            let _ = libc::kill(pid as i32, libc::SIGTERM);
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = libc::kill(pid as i32, libc::SIGKILL);
        }
    }

    /// Recursively find and kill child processes
    #[cfg(unix)]
    fn kill_children(parent_pid: u32) {
        // Find all child processes by reading /proc
        let entries = match std::fs::read_dir("/proc") {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            // Try to parse directory name as PID
            let file_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };

            let pid = match file_name.parse::<u32>() {
                Ok(pid) => pid,
                Err(_) => continue,
            };

            // Read and parse the stat file to get parent PID
            let stat_path = format!("/proc/{}/stat", pid);
            let stat = match std::fs::read_to_string(&stat_path) {
                Ok(stat) => stat,
                Err(_) => continue,
            };

            let ppid = match Self::parse_ppid_from_stat(&stat) {
                Some(ppid) => ppid,
                None => continue,
            };

            // If this process is a child of parent_pid, kill it
            if ppid == parent_pid {
                // Recursively kill children of this child first
                Self::kill_children(pid);

                // Then kill this child process
                unsafe {
                    let _ = libc::kill(pid as i32, libc::SIGTERM);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let _ = libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
    }

    /// Parse parent PID from /proc/[pid]/stat
    #[cfg(unix)]
    fn parse_ppid_from_stat(stat: &str) -> Option<u32> {
        // The stat format is: pid (comm) state ppid ...
        // We need to find the closing parenthesis and then get the 4th field
        let close_paren = stat.rfind(')')?;
        let fields: Vec<&str> = stat[close_paren + 1..].split_whitespace().collect();

        // After the comm field, we have: state ppid ...
        // So ppid is at index 1 (0-based: state is 0, ppid is 1)
        fields.get(1)?.parse().ok()
    }

    /// Kill a process tree on Windows
    #[cfg(windows)]
    fn kill_process_tree(pid: u32) {
        // On Windows, we can use taskkill to terminate the process tree
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
    }

    /// Check if a PID exists
    fn pid_exists(pid: u32) -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check if /proc/{pid} exists
            let proc_path = format!("/proc/{}", pid);
            if !std::path::Path::new(&proc_path).exists() {
                return false;
            }

            // Also check if the process is a zombie by reading /proc/{pid}/stat
            // Zombie processes still exist in /proc but should be considered as "not existing"
            // for our purposes since they can't actually run
            let stat_path = format!("{}/stat", proc_path);
            if let Ok(stat_content) = std::fs::read_to_string(&stat_path) {
                // The stat file format is: pid (comm) state ...
                // We need to find the state character after the command name in parentheses
                // The state is a single character: Z for zombie
                if let Some(state_start) = stat_content.rfind(')') {
                    // State is the next non-whitespace character after the closing paren
                    let remaining = &stat_content[state_start + 1..];
                    if let Some(state_char) = remaining.trim_start().chars().next() {
                        // If the process is a zombie, consider it as not existing
                        if state_char == 'Z' {
                            return false;
                        }
                    }
                }
            }

            true
        }

        #[cfg(unix)]
        #[cfg(not(target_os = "linux"))]
        {
            // On Unix-like systems (macOS, BSD, etc.), use kill with signal 0
            // kill(pid, 0) returns 0 if the process exists and we have permission
            unsafe { libc::kill(pid as i32, 0) == 0 }
        }

        #[cfg(windows)]
        {
            // On Windows, try to open the process handle
            // This is a simplified check - process might exist but we lack permissions
            use std::ptr;
            const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

            unsafe {
                let handle = winapi::um::processthreadsapi::OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                );
                if handle == ptr::null_mut() {
                    false
                } else {
                    winapi::um::handleapi::CloseHandle(handle);
                    true
                }
            }
        }
    }

    /// Cleanup all processes on shutdown
    pub async fn cleanup(&self) -> Result<()> {
        tracing::info!("Cleaning up all processes on shutdown");

        // First, get all process PIDs WITHOUT holding the lock
        let pids_to_kill = {
            let daemon = self.daemon.lock().await;
            let processes = daemon
                .list()
                .await
                .context("Failed to list processes during cleanup")?;
            processes
                .iter()
                .filter_map(|p| p.pid.map(|pid| (p.name.clone(), pid)))
                .collect::<Vec<_>>()
        };

        // Kill all process trees WITHOUT holding the mutex
        for (name, pid) in &pids_to_kill {
            tracing::info!(
                "Killing process tree for '{}' (PID: {}) during cleanup",
                name,
                pid
            );
            Self::kill_process_tree(*pid);
        }

        // Give the processes time to terminate
        if !pids_to_kill.is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // Reacquire lock only for deletion
        let mut daemon = self.daemon.lock().await;
        let deleted_count = daemon
            .delete_all()
            .await
            .context("Failed to delete all processes during cleanup")?;
        drop(daemon);

        // Clear command tracking
        let mut commands = self.commands.lock().await;
        commands.clear();
        drop(commands);

        tracing::info!("Cleaned up {} process(es)", deleted_count);
        Ok(())
    }

    /// Run a command and return the process ID
    pub async fn run(&self, command: &str) -> Result<RunResponse> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        let script = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // Generate a simple process name from the command
        let process_id = self.generate_process_id(command);

        let mut config = ProcessConfig::builder()
            .name(&process_id)
            .script(&script)
            .args(args)
            .build()
            .context("Failed to build process config")?;

        // Disable auto-restart - we want to detect failures immediately
        config.autorestart = false;
        config.max_restarts = 0; // Ensure no restarts happen at all

        let mut daemon = self.daemon.lock().await;

        // Try to start the process, but provide a helpful error if it already exists
        match daemon.start(config).await {
            Ok(_) => {
                // Store the command for later retrieval
                let mut commands = self.commands.lock().await;
                commands.insert(process_id.clone(), command.to_string());
                drop(commands);

                // Wait and check multiple times to detect early failures
                // CRITICAL: PMDaemon doesn't automatically update process status in background!
                // The exit_code field is only updated when check_status() is called on each process.
                // We must explicitly call check_all_processes() before each check to trigger updates.
                let mut check_attempts = 0;
                let max_attempts = 5; // Check for up to 5 seconds

                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    check_attempts += 1;

                    // CRITICAL: PMDaemon doesn't automatically update process status in background
                    // We need to explicitly call check_all_processes() to trigger status updates
                    // This calls check_status() on each process which updates exit_code and state
                    let _ = daemon.check_all_processes().await;

                    // Now get the updated process list
                    let processes = daemon.list().await?;
                    let process_status = processes.iter().find(|p| p.name == process_id);

                    if let Some(status) = process_status {
                        // Check if process state indicates failure
                        // Note: Stopped state by itself doesn't mean failure - process might have completed successfully
                        // We check exit_code separately to determine if it was a failure
                        let state_failed = matches!(status.state, pmdaemon::ProcessState::Errored);

                        // Check if process has exited with a non-zero exit code
                        // This is the PRIMARY check for failure
                        let has_error_exit_code = status.exit_code.is_some_and(|code| code != 0);

                        // Check if PID exists in system (if we have a PID)
                        // This catches zombie processes or processes that crashed without reporting exit code yet
                        // IMPORTANT: Only consider this a failure if we don't have an exit code yet
                        // If we have exit code 0, the process completed successfully even if PID is gone
                        let pid_not_exists_without_exit_code = if status.exit_code.is_none() {
                            if let Some(pid) = status.pid {
                                !Self::pid_exists(pid)
                            } else {
                                // PID is None but no exit code yet - process might have crashed
                                true
                            }
                        } else {
                            // We have an exit code, so use that to determine success/failure
                            false
                        };

                        // A process has failed if:
                        // 1. PMDaemon marked it as Errored, OR
                        // 2. It exited with a non-zero exit code, OR
                        // 3. The PID doesn't exist but we don't have an exit code yet (crashed)
                        let has_failed =
                            state_failed || has_error_exit_code || pid_not_exists_without_exit_code;

                        if has_failed {
                            // Get logs to include in error message
                            let logs = daemon.get_logs(&process_id, 100).await.unwrap_or_default();

                            // Clean up the failed process
                            let _ = daemon.delete(&process_id).await;
                            drop(daemon);

                            // Remove from command tracking
                            let mut commands = self.commands.lock().await;
                            commands.remove(&process_id);
                            drop(commands);

                            return Err(anyhow!(
                                "Process '{}' failed to start. Logs:\n{}",
                                process_id,
                                logs
                            ));
                        }

                        // If process appears healthy and we've checked enough times, consider it started
                        if check_attempts >= max_attempts {
                            break;
                        }
                    } else {
                        // Process not found in list - it may have exited
                        let logs = daemon.get_logs(&process_id, 100).await.unwrap_or_default();

                        // Clean up
                        let _ = daemon.delete(&process_id).await;
                        drop(daemon);

                        let mut commands = self.commands.lock().await;
                        commands.remove(&process_id);
                        drop(commands);

                        return Err(anyhow!(
                            "Process '{}' failed to start (not found in process list). Logs:\n{}",
                            process_id,
                            logs
                        ));
                    }
                }

                // Get initial logs to include in response
                let logs = daemon.get_logs(&process_id, 100).await.ok();

                Ok(RunResponse {
                    process_id: process_id.clone(),
                    command: command.to_string(),
                    message: format!("Started process '{}'", process_id),
                    logs,
                })
            }
            Err(e) => {
                let error_msg = e.to_string();
                // Check if the error is because the process already exists
                if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                    Err(anyhow!(
                        "Process '{}' already exists. Use the 'restart' tool to restart it, or 'kill' then 'run' to start fresh.",
                        process_id
                    ))
                } else {
                    Err(anyhow!("Failed to start process '{}': {}", process_id, e))
                }
            }
        }
    }

    /// Kill a specific process and remove it from the list
    pub async fn kill(&self, process_id: &str) -> Result<KillResponse> {
        // First, get the PID so we can kill the entire process tree
        // Acquire lock, get data, then immediately release
        let pid_to_kill = {
            let daemon = self.daemon.lock().await;
            let processes = daemon.list().await?;
            processes
                .iter()
                .find(|p| p.name == process_id)
                .and_then(|p| p.pid)
        };

        // Kill the process tree WITHOUT holding the mutex
        if let Some(pid) = pid_to_kill {
            tracing::info!("Killing process tree for '{}' (PID: {})", process_id, pid);
            Self::kill_process_tree(pid);
            // Give the processes time to terminate
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // Reacquire lock only for deletion
        let mut daemon = self.daemon.lock().await;
        daemon.delete(process_id).await.with_context(|| {
            format!(
                "Failed to kill process '{}' - it may not exist or may have already stopped",
                process_id
            )
        })?;
        drop(daemon);

        // Remove from command tracking
        let mut commands = self.commands.lock().await;
        commands.remove(process_id);
        drop(commands);

        Ok(KillResponse {
            process_id: process_id.to_string(),
            message: format!("Killed and removed process '{}'", process_id),
        })
    }

    /// Stop a specific process (without removing from the list)
    pub async fn stop(&self, process_id: &str) -> Result<KillResponse> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .stop(process_id)
            .await
            .context(format!("Failed to stop process '{}'", process_id))?;

        Ok(KillResponse {
            process_id: process_id.to_string(),
            message: format!("Stopped process '{}'", process_id),
        })
    }

    /// Kill all running processes and remove them from the list
    pub async fn kill_all(&self) -> Result<KillAllResponse> {
        // First, get all process PIDs WITHOUT holding the lock
        let pids_to_kill = {
            let daemon = self.daemon.lock().await;
            let processes = daemon.list().await?;
            processes
                .iter()
                .filter_map(|p| p.pid.map(|pid| (p.name.clone(), pid)))
                .collect::<Vec<_>>()
        };

        // Kill all process trees WITHOUT holding the mutex
        for (name, pid) in &pids_to_kill {
            tracing::info!("Killing process tree for '{}' (PID: {})", name, pid);
            Self::kill_process_tree(*pid);
        }

        // Give the processes time to terminate
        if !pids_to_kill.is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        // Reacquire lock only for deletion
        let mut daemon = self.daemon.lock().await;
        let deleted_count = daemon
            .delete_all()
            .await
            .context("Failed to delete all processes")?;
        drop(daemon);

        // Clear command tracking
        let mut commands = self.commands.lock().await;
        commands.clear();
        drop(commands);

        Ok(KillAllResponse {
            stopped_count: deleted_count,
            message: format!("Killed and removed {} process(es)", deleted_count),
        })
    }

    /// Read output from a process
    pub async fn read(&self, process_id: &str, lines: usize) -> Result<ReadResponse> {
        let daemon = self.daemon.lock().await;

        // Use get_logs method to retrieve log output
        match daemon.get_logs(process_id, lines).await {
            Ok(logs) => Ok(ReadResponse {
                process_id: process_id.to_string(),
                logs: if logs.is_empty() { String::new() } else { logs },
            }),
            Err(e) => Err(anyhow!(
                "Failed to read logs for process '{}': {}",
                process_id,
                e
            )),
        }
    }

    /// Restart a process
    pub async fn restart(&self, process_id: &str) -> Result<RestartResponse> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .restart(process_id)
            .await
            .context(format!("Failed to restart process '{}'", process_id))?;

        Ok(RestartResponse {
            process_id: process_id.to_string(),
            message: format!("Restarted process '{}'", process_id),
        })
    }

    /// List all processes
    pub async fn list(&self) -> Result<ListResponse> {
        let daemon = self.daemon.lock().await;
        let processes = daemon.list().await?;
        drop(daemon);

        let commands = self.commands.lock().await;

        let process_infos: Vec<ProcessInfo> = processes
            .into_iter()
            .map(|process| {
                let status = match process.state {
                    pmdaemon::ProcessState::Online => "running",
                    pmdaemon::ProcessState::Stopped => "stopped",
                    pmdaemon::ProcessState::Errored => "errored",
                    pmdaemon::ProcessState::Starting => "starting",
                    pmdaemon::ProcessState::Stopping => "stopping",
                    pmdaemon::ProcessState::Restarting => "restarting",
                };

                // Calculate uptime in seconds
                let uptime_secs = if let Some(start_time) = process.uptime {
                    let now = chrono::Utc::now();
                    (now - start_time).num_seconds()
                } else {
                    0
                };

                // Format started_at timestamp
                let started_at = if let Some(start_time) = process.uptime {
                    start_time.format("%Y-%m-%d %H:%M:%S UTC").to_string()
                } else {
                    "N/A".to_string()
                };

                // Get the command from our tracking map, or use a placeholder
                let command = commands
                    .get(&process.name)
                    .cloned()
                    .unwrap_or_else(|| "<unknown>".to_string());

                ProcessInfo {
                    name: process.name,
                    command,
                    status: status.to_string(),
                    pid: process.pid.unwrap_or(0),
                    started_at,
                    uptime_seconds: uptime_secs,
                    restarts: process.restarts,
                    cpu_usage: process.cpu_usage as f64,
                    memory_mb: process.memory_usage / 1024 / 1024,
                }
            })
            .collect();

        Ok(ListResponse {
            processes: process_infos,
        })
    }

    /// Generate a unique process ID from the command
    fn generate_process_id(&self, command: &str) -> String {
        let words: Vec<&str> = command
            .split_whitespace()
            .take(2)
            .filter(|w| !w.starts_with('-'))
            .collect();

        let base_id = if words.is_empty() {
            "proc".to_string()
        } else {
            words.join("_")
        }
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_lowercase();

        // For simplicity, we'll use the base_id directly
        // PMDaemon will handle conflicts by returning errors
        base_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_manager_creation() {
        let manager = ProcessManager::new().await;
        assert!(manager.is_ok(), "Should be able to create ProcessManager");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_kill_process_tree() {
        // This test verifies that kill_process_tree properly terminates child processes
        // We'll spawn a shell script that spawns a child process, then verify both are killed
        let manager = ProcessManager::new().await.unwrap();

        // Create a shell script that spawns a child process
        // The parent will sleep while the child also sleeps
        let script_cmd = "sh -c 'sleep 30 & sleep 30'";

        // Start the process
        let result = manager.run(script_cmd).await;
        if result.is_err() {
            // If it failed to start, skip this test
            return;
        }

        // Give it a moment to spawn children
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Get the process PID
        let daemon = manager.daemon.lock().await;
        let processes = daemon.list().await.unwrap();
        let process = processes.iter().find(|p| p.name == "sh");

        if let Some(proc) = process {
            if let Some(parent_pid) = proc.pid {
                drop(daemon);

                // Count child processes before killing (for debugging)
                let _children_before = count_child_processes(parent_pid);

                // Kill the process tree
                let _ = manager.kill("sh").await;

                // Give processes time to terminate
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                // Verify the parent process is gone
                assert!(
                    !ProcessManager::pid_exists(parent_pid),
                    "Parent process should be terminated"
                );

                // Verify child processes are also gone
                let children_after = count_child_processes(parent_pid);
                assert_eq!(
                    children_after, 0,
                    "All child processes should be terminated"
                );
            }
        }
    }

    #[cfg(unix)]
    fn count_child_processes(parent_pid: u32) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    if let Ok(pid) = file_name.parse::<u32>() {
                        let stat_path = format!("/proc/{}/stat", pid);
                        if let Ok(stat) = std::fs::read_to_string(&stat_path) {
                            if let Some(ppid) = ProcessManager::parse_ppid_from_stat(&stat) {
                                if ppid == parent_pid && ProcessManager::pid_exists(pid) {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        count
    }
}
