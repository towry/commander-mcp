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
        let daemon = PmDaemon::new()
            .await
            .context("Failed to initialize PMDaemon")?;

        Ok(Self {
            daemon: Arc::new(Mutex::new(daemon)),
            commands: Arc::new(Mutex::new(HashMap::new())),
        })
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

        let config = ProcessConfig::builder()
            .name(&process_id)
            .script(&script)
            .args(args)
            .build()
            .context("Failed to build process config")?;

        let mut daemon = self.daemon.lock().await;

        // Try to start the process, but provide a helpful error if it already exists
        match daemon.start(config).await {
            Ok(_) => {
                // Store the command for later retrieval
                let mut commands = self.commands.lock().await;
                commands.insert(process_id.clone(), command.to_string());
                drop(commands);

                // Wait 1 second to detect early failures
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                // Check if the process has failed
                let processes = daemon.list().await?;
                let process_status = processes.iter().find(|p| p.name == process_id);

                if let Some(status) = process_status {
                    // If process is in errored or stopped state, has no PID (exited),
                    // or has already restarted (indicating it crashed), it failed
                    let has_failed = matches!(
                        status.state,
                        pmdaemon::ProcessState::Errored | pmdaemon::ProcessState::Stopped
                    ) || status.pid.is_none()
                        || status.restarts > 0;

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
        let mut daemon = self.daemon.lock().await;

        // Use delete instead of stop to remove from the list
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
        let mut daemon = self.daemon.lock().await;

        // Use delete_all to remove all processes from the list
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
}
