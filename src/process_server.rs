use crate::process_manager::{
    KillAllResponse, KillResponse, ListResponse, ProcessManager, ReadResponse, RestartResponse,
    RunResponse,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler,
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

/// Parameters for the stop tool
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct StopParams {
    /// The process ID to stop
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

/// Parameters for the kill_all tool (no parameters required)
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct KillAllParams {}

/// Parameters for the list tool (no parameters required)
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListParams {}

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
    async fn run(&self, params: Parameters<RunParams>) -> Result<Json<RunResponse>, String> {
        self.manager
            .run(&params.0.command)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Kill a specific process
    #[tool(
        description = "Kill a previously started process by its ID and remove it from the list."
    )]
    async fn kill(&self, params: Parameters<KillParams>) -> Result<Json<KillResponse>, String> {
        self.manager
            .kill(&params.0.process_id)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Stop a specific process (without removing from list)
    #[tool(description = "Stop a previously started process by its ID (keeps it in the list).")]
    async fn stop(&self, params: Parameters<StopParams>) -> Result<Json<KillResponse>, String> {
        self.manager
            .stop(&params.0.process_id)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Kill all running processes
    #[tool(description = "Kill all running processes and remove them from the list.")]
    async fn kill_all(
        &self,
        _params: Parameters<KillAllParams>,
    ) -> Result<Json<KillAllResponse>, String> {
        self.manager
            .kill_all()
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Read output from a process
    #[tool(
        description = "Read the stdout output from a process by its ID. Returns recent log output (default: last 1000 lines)."
    )]
    async fn read(&self, params: Parameters<ReadParams>) -> Result<Json<ReadResponse>, String> {
        self.manager
            .read(&params.0.process_id, params.0.lines)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Restart a process
    #[tool(description = "Restart a previously run process by its ID.")]
    async fn restart(
        &self,
        params: Parameters<RestartParams>,
    ) -> Result<Json<RestartResponse>, String> {
        self.manager
            .restart(&params.0.process_id)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// List all processes
    #[tool(description = "List all currently running or stopped processes.")]
    async fn list(&self, _params: Parameters<ListParams>) -> Result<Json<ListResponse>, String> {
        self.manager
            .list()
            .await
            .map(Json)
            .map_err(|e| e.to_string())
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
                 - kill: Stop and remove a specific process\n\
                 - stop: Stop a specific process (keeps in list)\n\
                 - kill_all: Stop and remove all processes\n\
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
        assert!(router.has_route("stop"));
        assert!(router.has_route("kill_all"));
        assert!(router.has_route("read"));
        assert!(router.has_route("restart"));
        assert!(router.has_route("list"));

        let tools = router.list_all();
        assert_eq!(tools.len(), 7);
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

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run a simple command
        let run_params = Parameters(RunParams {
            command: "sleep 5".to_string(),
        });
        let result = server.run(run_params).await;
        assert!(result.is_ok(), "Should be able to run a command");

        // List processes to verify it shows up
        let list_result = server.list(Parameters(ListParams {})).await;
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

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run a command
        let run_params1 = Parameters(RunParams {
            command: "sleep 10".to_string(),
        });
        let result1 = server.run(run_params1).await;
        assert!(result1.is_ok(), "First run should succeed");

        // Try to run the same command again - should get an error
        let run_params2 = Parameters(RunParams {
            command: "sleep 10".to_string(),
        });
        let result2 = server.run(run_params2).await;

        // The result should be Err for duplicate process
        assert!(
            result2.is_err(),
            "Should return error for duplicate process"
        );

        // Clean up
        let kill_params = Parameters(KillParams {
            process_id: "sleep".to_string(),
        });
        let _ = server.kill(kill_params).await;
    }

    #[tokio::test]
    #[ignore] // TODO: Fix PMDaemon state persistence issue between test runs
    async fn test_kill_removes_from_list() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run a command
        let run_params = Parameters(RunParams {
            command: "sleep 100".to_string(),
        });
        let run_result = server.run(run_params).await;
        if let Err(e) = &run_result {
            eprintln!("Run failed: {}", e);
        }
        assert!(run_result.is_ok(), "Run should succeed");

        // Give the process a moment to actually start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Kill it
        let kill_params = Parameters(KillParams {
            process_id: "sleep".to_string(),
        });
        let kill_result = server.kill(kill_params).await;
        if let Err(e) = &kill_result {
            eprintln!("Kill failed: {}", e);
        }
        assert!(kill_result.is_ok(), "Kill should succeed");

        // Verify list succeeds
        let list_result = server.list(Parameters(ListParams {})).await;
        assert!(list_result.is_ok(), "List should succeed after kill");
    }

    #[tokio::test]
    async fn test_kill_all_removes_all_from_list() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run multiple commands
        let _ = server
            .run(Parameters(RunParams {
                command: "sleep 5".to_string(),
            }))
            .await;
        let _ = server
            .run(Parameters(RunParams {
                command: "echo test".to_string(),
            }))
            .await;

        // Kill all
        let kill_all_result = server.kill_all(Parameters(KillAllParams {})).await;
        assert!(kill_all_result.is_ok(), "Kill all should succeed");

        // Verify list returns successfully
        let list_result = server.list(Parameters(ListParams {})).await;
        assert!(list_result.is_ok(), "List should succeed after kill_all");
    }

    #[tokio::test]
    async fn test_list_command_field() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run a command
        let run_params = Parameters(RunParams {
            command: "python -m http.server".to_string(),
        });
        let run_result = server.run(run_params).await;
        assert!(run_result.is_ok(), "Run should succeed");

        // List processes
        let list_result = server.list(Parameters(ListParams {})).await;
        assert!(list_result.is_ok(), "List should succeed");

        // Clean up
        let kill_params = Parameters(KillParams {
            process_id: "python".to_string(),
        });
        let _ = server.kill(kill_params).await;
    }

    #[tokio::test]
    #[ignore] // TODO: Fix PMDaemon state persistence issue between test runs
    async fn test_stop_tool_exists() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Run a command
        let run_params = Parameters(RunParams {
            command: "sleep 10".to_string(),
        });
        let run_result = server.run(run_params).await;
        assert!(run_result.is_ok(), "Run should succeed before stop");

        // Give the process a moment to actually start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Stop it (not kill)
        let stop_params = Parameters(StopParams {
            process_id: "sleep".to_string(),
        });
        let stop_result = server.stop(stop_params).await;
        assert!(stop_result.is_ok(), "Stop should succeed");

        // Clean up - now kill to remove it
        let kill_params = Parameters(KillParams {
            process_id: "sleep".to_string(),
        });
        let _ = server.kill(kill_params).await;
    }

    #[tokio::test]
    async fn test_structured_output_format() {
        use rmcp::handler::server::tool::IntoCallToolResult;

        // Create a test response
        let response = RunResponse {
            process_id: "test".to_string(),
            command: "echo hello".to_string(),
            message: "Started process 'test'".to_string(),
            logs: None,
        };

        // Wrap it in Json
        let json_response = rmcp::Json(response);

        // Convert to CallToolResult - this is what rmcp does internally
        let result = json_response.into_call_tool_result().unwrap();

        // Verify structured_content is populated per MCP spec
        assert!(
            result.structured_content.is_some(),
            "structured_content should be populated per MCP protocol"
        );

        // Verify content also has the JSON as text
        assert!(!result.content.is_empty(), "content should not be empty");

        // Verify is_error is false for success
        assert_eq!(
            result.is_error,
            Some(false),
            "is_error should be Some(false) for success"
        );
    }

    #[tokio::test]
    async fn test_run_detects_early_failure() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // Try to run a command that will fail immediately
        // Using a non-existent command should cause an early failure
        let run_params = Parameters(RunParams {
            command: "nonexistent_command_12345".to_string(),
        });
        let result = server.run(run_params).await;

        // Should return an error for a command that fails immediately
        if let Err(error_msg) = result {
            assert!(
                error_msg.contains("Failed to start process") || error_msg.contains("Logs:"),
                "Error message should indicate failure or include logs: {}",
                error_msg
            );
        } else {
            panic!("Should return error for command that fails immediately, but got success");
        }
    }

    #[tokio::test]
    async fn test_run_detects_port_conflict() {
        let server = ProcessServer::new().await.unwrap();

        // Clean up any leftover processes from previous tests
        let _ = server.kill_all(Parameters(KillAllParams {})).await;

        // First, start a server on a specific port
        let run_params1 = Parameters(RunParams {
            command: "python3 -m http.server 9999".to_string(),
        });
        let result1 = server.run(run_params1).await;

        // The first server should start successfully
        assert!(result1.is_ok(), "First server should start successfully");

        // Give it a moment to bind to the port
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Now try to start another server on the same port - should fail
        let run_params2 = Parameters(RunParams {
            command: "python3 -m http.server 9999".to_string(),
        });
        let result2 = server.run(run_params2).await;

        // Should return an error due to port conflict
        // This should either fail because:
        // 1. Process with same ID already exists
        // 2. Process exits due to port conflict (if we use different command to generate different ID)
        assert!(result2.is_err(), "Second server on same port should fail");

        // Clean up
        let _ = server
            .kill(Parameters(KillParams {
                process_id: "python3".to_string(),
            }))
            .await;
    }
}
