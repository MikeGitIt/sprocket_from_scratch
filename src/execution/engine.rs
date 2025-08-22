use super::task_executor::TaskExecutor;
use super::workflow_executor::WorkflowExecutor;
use crate::error::{Result, SprocketError};
use crate::parser::{Task, WdlDocument, Workflow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: Uuid,
    pub task: Task,
    pub status: TaskStatus,
    pub inputs: HashMap<String, String>,
    pub outputs: HashMap<String, String>,
    pub command_executed: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecution {
    pub id: Uuid,
    pub workflow: Workflow,
    pub status: WorkflowStatus,
    pub inputs: HashMap<String, serde_json::Value>,
    pub outputs: HashMap<String, serde_json::Value>,
    pub task_executions: Vec<Uuid>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

pub struct ExecutionEngine {
    pub task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
    pub workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
    pub task_executor: TaskExecutor,
    pub workflow_executor: WorkflowExecutor,
}

impl ExecutionEngine {
    pub fn new() -> Self {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));

        Self {
            task_executions: task_executions.clone(),
            workflow_executions: workflow_executions.clone(),
            task_executor: TaskExecutor::new(task_executions.clone()),
            workflow_executor: WorkflowExecutor::new(
                task_executions.clone(),
                workflow_executions.clone(),
            ),
        }
    }

    pub async fn execute_task(&self, task: Task, inputs: HashMap<String, String>) -> Result<Uuid> {
        self.task_executor.execute(task, inputs).await
    }

    pub async fn execute_workflow(
        &self,
        wdl_document: WdlDocument,
        workflow_name: String,
        inputs: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        self.workflow_executor
            .execute(wdl_document, workflow_name, inputs)
            .await
    }

    pub async fn get_task_status(&self, task_id: Uuid) -> Result<TaskExecution> {
        let executions = self.task_executions.read().await;
        executions
            .get(&task_id)
            .cloned()
            .ok_or_else(|| SprocketError::TaskNotFound(task_id.to_string()))
    }

    pub async fn get_workflow_status(&self, workflow_id: Uuid) -> Result<WorkflowExecution> {
        let executions = self.workflow_executions.read().await;
        executions
            .get(&workflow_id)
            .cloned()
            .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_id.to_string()))
    }

    pub async fn get_workflow_tasks(&self, workflow_id: Uuid) -> Result<Vec<TaskExecution>> {
        let workflow_executions = self.workflow_executions.read().await;
        let workflow = workflow_executions
            .get(&workflow_id)
            .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_id.to_string()))?;

        let task_executions = self.task_executions.read().await;
        let mut tasks = Vec::new();

        for task_id in &workflow.task_executions {
            if let Some(task) = task_executions.get(task_id) {
                tasks.push(task.clone());
            }
        }

        Ok(tasks)
    }
}
