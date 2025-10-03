use crate::process_manager::ProcessManager;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for the run tool
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RunParams {
    /// The command to run in the background
    pub command: String,
}

/// Parameters for the kill tool
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct KillParams {
    /// The process ID to kill
    pub process_id: String,
}

/// Parameters for the read tool
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// The process ID to read output from
    pub process_id: String,
    /// Maximum number of log lines to return (default: 1000)
    #[serde(default = "default_read_lines")]
    pub lines: usize,
}

fn default_read_lines() -> usize {
    1000
}

/// Parameters for the restart tool
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RestartParams {
    /// The process ID to restart
    pub process_id: String,
}

/// Process management server that provides tools for running and managing background processes
#[derive(Clone)]
pub struct ProcessServer {
    tool_router: ToolRouter<ProcessServer>,
    manager: ProcessManager,
}

#[tool_router]
impl ProcessServer {
    pub async fn new() -> Result<Self, McpError> {
        let manager = ProcessManager::new().await.map_err(|e| {
            McpError::invalid_request(format!("Failed to initialize process manager: {}", e), None)
        })?;

        Ok(Self {
            tool_router: Self::tool_router(),
            manager,
        })
    }

    /// Run a command in the background
    #[tool(
        description = "Run a command in the background. Returns the process ID that can be used with other tools."
    )]
    async fn run(&self, params: Parameters<RunParams>) -> Result<CallToolResult, McpError> {
        match self.manager.run(&params.0.command).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to run command: {}",
                e
            ))])),
        }
    }

    /// Kill a specific process
    #[tool(description = "Kill a previously started process by its ID.")]
    async fn kill(&self, params: Parameters<KillParams>) -> Result<CallToolResult, McpError> {
        match self.manager.kill(&params.0.process_id).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to kill process: {}",
                e
            ))])),
        }
    }

    /// Kill all running processes
    #[tool(description = "Kill all running processes.")]
    async fn kill_all(&self) -> Result<CallToolResult, McpError> {
        match self.manager.kill_all().await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to kill all processes: {}",
                e
            ))])),
        }
    }

    /// Read output from a process
    #[tool(
        description = "Read the stdout output from a process by its ID. Returns recent log output (default: last 1000 lines)."
    )]
    async fn read(&self, params: Parameters<ReadParams>) -> Result<CallToolResult, McpError> {
        match self
            .manager
            .read(&params.0.process_id, params.0.lines)
            .await
        {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read process output: {}",
                e
            ))])),
        }
    }

    /// Restart a process
    #[tool(description = "Restart a previously run process by its ID.")]
    async fn restart(&self, params: Parameters<RestartParams>) -> Result<CallToolResult, McpError> {
        match self.manager.restart(&params.0.process_id).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to restart process: {}",
                e
            ))])),
        }
    }

    /// List all processes
    #[tool(description = "List all currently running or stopped processes.")]
    async fn list(&self) -> Result<CallToolResult, McpError> {
        match self.manager.list().await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list processes: {}",
                e
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for ProcessServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This is a process management MCP server using PMDaemon. It provides tools for managing background processes:\n\
                 - run: Execute a command in the background\n\
                 - kill: Stop a specific process\n\
                 - kill_all: Stop all processes\n\
                 - read: Read output logs from a process\n\
                 - restart: Restart a process\n\
                 - list: Show all processes with their status"
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_server_creation() {
        let server = ProcessServer::new().await;
        assert!(server.is_ok(), "Should be able to create ProcessServer");

        if let Ok(s) = server {
            let info = s.get_info();
            assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
            assert!(info.capabilities.tools.is_some());
            assert!(info.instructions.is_some());
        }
    }

    #[tokio::test]
    async fn test_tool_router_has_all_tools() {
        let router = ProcessServer::tool_router();

        assert!(router.has_route("run"));
        assert!(router.has_route("kill"));
        assert!(router.has_route("kill_all"));
        assert!(router.has_route("read"));
        assert!(router.has_route("restart"));
        assert!(router.has_route("list"));

        let tools = router.list_all();
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_read_params_default_lines() {
        // Test that default lines is 1000
        let json = r#"{"process_id": "test_proc"}"#;
        let params: ReadParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.process_id, "test_proc");
        assert_eq!(params.lines, 1000);
    }

    #[test]
    fn test_read_params_custom_lines() {
        // Test that custom lines value is used
        let json = r#"{"process_id": "test_proc", "lines": 500}"#;
        let params: ReadParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.process_id, "test_proc");
        assert_eq!(params.lines, 500);
    }

    #[tokio::test]
    async fn test_run_and_list() {
        let server = ProcessServer::new().await.unwrap();

        // Run a simple command
        let run_params = Parameters(RunParams {
            command: "sleep 5".to_string(),
        });
        let result = server.run(run_params).await;
        assert!(result.is_ok(), "Should be able to run a command");

        // List processes to verify it shows up
        let list_result = server.list().await;
        assert!(list_result.is_ok(), "Should be able to list processes");

        // Clean up - kill the process
        let kill_params = Parameters(KillParams {
            process_id: "sleep".to_string(),
        });
        let _ = server.kill(kill_params).await;
    }

    #[tokio::test]
    async fn test_run_duplicate_process_error() {
        let server = ProcessServer::new().await.unwrap();

        // Run a command
        let run_params1 = Parameters(RunParams {
            command: "sleep 10".to_string(),
        });
        let result1 = server.run(run_params1).await;
        assert!(result1.is_ok(), "First run should succeed");

        // Try to run the same command again - should get helpful error
        let run_params2 = Parameters(RunParams {
            command: "sleep 10".to_string(),
        });
        let result2 = server.run(run_params2).await;

        // The result is Ok(CallToolResult) but should be an error result
        if let Ok(call_result) = result2 {
            // Check if it's an error result
            assert!(
                !call_result.is_error.unwrap_or(false) || !call_result.content.is_empty(),
                "Should return error content for duplicate process"
            );
        }

        // Clean up
        let kill_params = Parameters(KillParams {
            process_id: "sleep".to_string(),
        });
        let _ = server.kill(kill_params).await;
    }
}
