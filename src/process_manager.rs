use anyhow::{anyhow, Context, Result};
use pmdaemon::{ProcessConfig, ProcessManager as PmDaemon};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ProcessManager {
    daemon: Arc<Mutex<PmDaemon>>,
}

impl ProcessManager {
    pub async fn new() -> Result<Self> {
        let daemon = PmDaemon::new()
            .await
            .context("Failed to initialize PMDaemon")?;

        Ok(Self {
            daemon: Arc::new(Mutex::new(daemon)),
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
            Ok(_) => Ok(format!(
                "Started process '{}' with command: {}",
                process_id, command
            )),
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

    /// Kill a specific process
    pub async fn kill(&self, process_id: &str) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .stop(process_id)
            .await
            .context(format!("Failed to stop process '{}'", process_id))?;

        Ok(format!("Stopped process '{}'", process_id))
    }

    /// Kill all running processes
    pub async fn kill_all(&self) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        let processes = daemon.list().await?;

        let mut stopped_count = 0;
        for process in processes {
            if let Err(e) = daemon.stop(&process.name).await {
                tracing::warn!("Failed to stop process '{}': {}", process.name, e);
            } else {
                stopped_count += 1;
            }
        }

        Ok(format!("Stopped {} process(es)", stopped_count))
    }

    /// Read output from a process
    pub async fn read(&self, process_id: &str, lines: usize) -> Result<String> {
        let daemon = self.daemon.lock().await;

        // Use get_logs method to retrieve log output
        match daemon.get_logs(process_id, lines).await {
            Ok(logs) => {
                if logs.is_empty() {
                    Ok(format!("No output available for process '{}'", process_id))
                } else {
                    Ok(logs)
                }
            }
            Err(e) => Err(anyhow!(
                "Failed to read logs for process '{}': {}",
                process_id,
                e
            )),
        }
    }

    /// Restart a process
    pub async fn restart(&self, process_id: &str) -> Result<String> {
        let mut daemon = self.daemon.lock().await;
        daemon
            .restart(process_id)
            .await
            .context(format!("Failed to restart process '{}'", process_id))?;

        Ok(format!("Restarted process '{}'", process_id))
    }

    /// List all processes
    pub async fn list(&self) -> Result<String> {
        let daemon = self.daemon.lock().await;
        let processes = daemon.list().await?;

        if processes.is_empty() {
            return Ok("No processes running".to_string());
        }

        let mut result = String::from("Processes:\n");
        for process in processes {
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

            result.push_str(&format!(
                "  - {} [{}]: PID: {}, started_at: {}, uptime: {}s, restarts: {}, CPU: {:.1}%, Mem: {} MB\n",
                process.name,
                status,
                process.pid.unwrap_or(0),
                started_at,
                uptime_secs,
                process.restarts,
                process.cpu_usage,
                process.memory_usage / 1024 / 1024 // Convert to MB
            ));
        }

        Ok(result)
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
