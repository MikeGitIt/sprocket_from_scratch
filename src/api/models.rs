use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitWorkflowRequest {
    pub workflow_source: String,
    pub inputs: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitWorkflowResponse {
    pub workflow_id: Uuid,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowStatusResponse {
    pub workflow_id: Uuid,
    pub status: String,
    pub tasks: Vec<TaskStatusResponse>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: Uuid,
    pub task_name: String,
    pub status: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowResultsResponse {
    pub workflow_id: Uuid,
    pub status: String,
    pub outputs: HashMap<String, serde_json::Value>,
    pub tasks: Vec<TaskResultResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResultResponse {
    pub task_id: Uuid,
    pub task_name: String,
    pub status: String,
    pub outputs: HashMap<String, String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskDetailsResponse {
    pub task_id: Uuid,
    pub task_name: String,
    pub status: String,
    pub inputs: HashMap<String, String>,
    pub outputs: HashMap<String, String>,
    pub command_executed: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
