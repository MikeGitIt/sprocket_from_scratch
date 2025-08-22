use crate::error::Result;
use crate::execution::{TaskExecution, WorkflowExecution};
use crate::parser::WdlDocument;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct MemoryStore {
    pub workflows: Arc<RwLock<HashMap<String, WdlDocument>>>,
    pub task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
    pub workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            task_executions: Arc::new(RwLock::new(HashMap::new())),
            workflow_executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store_workflow(&self, name: String, document: WdlDocument) -> Result<()> {
        let mut workflows = self.workflows.write().await;
        workflows.insert(name, document);
        Ok(())
    }

    pub async fn get_workflow(&self, name: &str) -> Result<Option<WdlDocument>> {
        let workflows = self.workflows.read().await;
        Ok(workflows.get(name).cloned())
    }

    pub async fn list_workflows(&self) -> Result<Vec<String>> {
        let workflows = self.workflows.read().await;
        Ok(workflows.keys().cloned().collect())
    }

    pub async fn store_task_execution(&self, execution: TaskExecution) -> Result<()> {
        let mut executions = self.task_executions.write().await;
        executions.insert(execution.id, execution);
        Ok(())
    }

    pub async fn get_task_execution(&self, id: Uuid) -> Result<Option<TaskExecution>> {
        let executions = self.task_executions.read().await;
        Ok(executions.get(&id).cloned())
    }

    pub async fn store_workflow_execution(&self, execution: WorkflowExecution) -> Result<()> {
        let mut executions = self.workflow_executions.write().await;
        executions.insert(execution.id, execution);
        Ok(())
    }

    pub async fn get_workflow_execution(&self, id: Uuid) -> Result<Option<WorkflowExecution>> {
        let executions = self.workflow_executions.read().await;
        Ok(executions.get(&id).cloned())
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::Storage for MemoryStore {
    async fn store_workflow(&self, name: String, document: WdlDocument) -> Result<()> {
        self.store_workflow(name, document).await
    }

    async fn get_workflow(&self, name: &str) -> Result<Option<WdlDocument>> {
        self.get_workflow(name).await
    }

    async fn list_workflows(&self) -> Result<Vec<String>> {
        self.list_workflows().await
    }

    async fn store_workflow_execution(&self, execution: &WorkflowExecution) -> Result<()> {
        self.store_workflow_execution(execution.clone()).await
    }

    async fn get_workflow_execution(&self, id: Uuid) -> Result<Option<WorkflowExecution>> {
        self.get_workflow_execution(id).await
    }

    async fn store_task_execution(
        &self,
        execution: &TaskExecution,
        _workflow_id: Option<Uuid>,
    ) -> Result<()> {
        self.store_task_execution(execution.clone()).await
    }

    async fn get_task_execution(&self, id: Uuid) -> Result<Option<TaskExecution>> {
        self.get_task_execution(id).await
    }
}
