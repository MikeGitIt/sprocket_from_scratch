pub mod memory_store;
pub mod sqlite_store;

pub use memory_store::MemoryStore;
pub use sqlite_store::SqliteStore;

use crate::error::Result;
use crate::execution::{TaskExecution, WorkflowExecution};
use crate::parser::WdlDocument;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn store_workflow(&self, name: String, document: WdlDocument) -> Result<()>;
    async fn get_workflow(&self, name: &str) -> Result<Option<WdlDocument>>;
    async fn list_workflows(&self) -> Result<Vec<String>>;

    async fn store_workflow_execution(&self, execution: &WorkflowExecution) -> Result<()>;
    async fn get_workflow_execution(&self, id: Uuid) -> Result<Option<WorkflowExecution>>;

    async fn store_task_execution(
        &self,
        execution: &TaskExecution,
        workflow_id: Option<Uuid>,
    ) -> Result<()>;
    async fn get_task_execution(&self, id: Uuid) -> Result<Option<TaskExecution>>;
}
