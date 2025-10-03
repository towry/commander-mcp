use anyhow::{anyhow, Context, Result};
use pmdaemon::{ProcessConfig, ProcessManager as PmDaemon};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Error response format
#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Response for the run command
#[derive(Serialize, Deserialize)]
struct RunResponse {
    process_id: String,
    command: String,
    message: String,
}

/// Response for the kill command
#[derive(Serialize, Deserialize)]
struct KillResponse {
    process_id: String,
    message: String,
}

/// Response for the kill_all command
#[derive(Serialize, Deserialize)]
struct KillAllResponse {
    stopped_count: usize,
    message: String,
}

/// Response for the read command
#[derive(Serialize, Deserialize)]
struct ReadResponse {
    process_id: String,
    logs: String,
}

/// Response for the restart command
#[derive(Serialize, Deserialize)]
struct RestartResponse {
    process_id: String,
    message: String,
}

/// Process information for the list command
#[derive(Serialize, Deserialize)]
struct ProcessInfo {
    name: String,
    command: String,
    status: String,
    pid: u32,
    started_at: String,
    uptime_seconds: i64,
    restarts: u32,
    cpu_usage: f64,
    memory_mb: u64,
}

/// Response for the list command
#[derive(Serialize, Deserialize)]
struct ListResponse {
    processes: Vec<ProcessInfo>,
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
    pub async fn run(&self, command: &str) -> Result<String> {
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

                let response = RunResponse {
                    process_id: process_id.clone(),
                    command: command.to_string(),
                    message: format!("Started process '{}'", process_id),
                };
                Ok(serde_json::to_string(&response)?)
            }
            Err(e) => {
                let error_msg = e.to_string();
                // Check if the error is because the process already exists
                if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                    let err = ErrorResponse {
                        error: format!(
                            "Process '{}' already exists. Use the 'restart' tool to restart it, or 'kill' then 'run' to start fresh.",
                            process_id
                        ),
                    };
                    Err(anyhow!(serde_json::to_string(&err)?))
                } else {
                    let err = ErrorResponse {
                        error: format!("Failed to start process '{}': {}", process_id, e),
                    };
                    Err(anyhow!(serde_json::to_string(&err)?))
                }
            }
        }
    }

    /// Kill a specific process and remove it from the list
    pub async fn kill(&self, process_id: &str) -> Result<String> {
        let mut daemon = self.daemon.lock().await;

        // Use delete instead of stop to remove from the list
        daemon
            .delete(process_id)
            .await
            .context(format!("Failed to kill process '{}'", process_id))?;

        drop(daemon);

        // Remove from command tracking
        let mut commands = self.commands.lock().await;
        commands.remove(process_id);
        drop(commands);

        let response = KillResponse {
            process_id: process_id.to_string(),
            message: format!("Killed and removed process '{}'", process_id),
        };
        Ok(serde_json::to_string(&response)?)
    }

    /// Stop a specific process (without removing from the list)
    pub async fn stop(&self, process_id: &str) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .stop(process_id)
            .await
            .context(format!("Failed to stop process '{}'", process_id))?;

        let response = KillResponse {
            process_id: process_id.to_string(),
            message: format!("Stopped process '{}'", process_id),
        };
        Ok(serde_json::to_string(&response)?)
    }

    /// Kill all running processes and remove them from the list
    pub async fn kill_all(&self) -> Result<String> {
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

        let response = KillAllResponse {
            stopped_count: deleted_count,
            message: format!("Killed and removed {} process(es)", deleted_count),
        };
        Ok(serde_json::to_string(&response)?)
    }

    /// Read output from a process
    pub async fn read(&self, process_id: &str, lines: usize) -> Result<String> {
        let daemon = self.daemon.lock().await;

        // Use get_logs method to retrieve log output
        match daemon.get_logs(process_id, lines).await {
            Ok(logs) => {
                let response = ReadResponse {
                    process_id: process_id.to_string(),
                    logs: if logs.is_empty() { String::new() } else { logs },
                };
                Ok(serde_json::to_string(&response)?)
            }
            Err(e) => {
                let err = ErrorResponse {
                    error: format!("Failed to read logs for process '{}': {}", process_id, e),
                };
                Err(anyhow!(serde_json::to_string(&err)?))
            }
        }
    }

    /// Restart a process
    pub async fn restart(&self, process_id: &str) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .restart(process_id)
            .await
            .context(format!("Failed to restart process '{}'", process_id))?;

        let response = RestartResponse {
            process_id: process_id.to_string(),
            message: format!("Restarted process '{}'", process_id),
        };
        Ok(serde_json::to_string(&response)?)
    }

    /// List all processes
    pub async fn list(&self) -> Result<String> {
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

        let response = ListResponse {
            processes: process_infos,
        };
        Ok(serde_json::to_string(&response)?)
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
