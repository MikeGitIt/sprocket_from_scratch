use crate::error::{Result, SprocketError};
use crate::execution::{TaskExecution, TaskStatus, WorkflowExecution, WorkflowStatus};
use crate::parser::WdlDocument;
use chrono::{DateTime, Utc};
use serde_json;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use uuid::Uuid;

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| {
                SprocketError::ExecutionError(format!("Failed to connect to database: {}", e))
            })?;

        let store = Self { pool };
        store.initialize_schema().await?;
        Ok(store)
    }

    async fn initialize_schema(&self) -> Result<()> {
        // Create workflows table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                document JSON NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!("Failed to create workflows table: {}", e))
        })?;

        // Create workflow_executions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_executions (
                id TEXT PRIMARY KEY,
                workflow_name TEXT NOT NULL,
                status TEXT NOT NULL,
                inputs JSON NOT NULL,
                outputs JSON,
                start_time TIMESTAMP,
                end_time TIMESTAMP,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!(
                "Failed to create workflow_executions table: {}",
                e
            ))
        })?;

        // Create task_executions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS task_executions (
                id TEXT PRIMARY KEY,
                workflow_execution_id TEXT,
                task_name TEXT NOT NULL,
                status TEXT NOT NULL,
                inputs JSON NOT NULL,
                outputs JSON,
                command_executed TEXT,
                stdout TEXT,
                stderr TEXT,
                exit_code INTEGER,
                start_time TIMESTAMP,
                end_time TIMESTAMP,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (workflow_execution_id) REFERENCES workflow_executions(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!("Failed to create task_executions table: {}", e))
        })?;

        // Create index for better query performance
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_task_executions_workflow 
            ON task_executions(workflow_execution_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to create index: {}", e)))?;

        Ok(())
    }

    pub async fn store_workflow(&self, name: String, document: WdlDocument) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let document_json =
            serde_json::to_string(&document).map_err(|e| SprocketError::SerializationError(e))?;

        sqlx::query(
            r#"
            INSERT INTO workflows (id, name, document)
            VALUES (?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                document = excluded.document
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(document_json)
        .execute(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to store workflow: {}", e)))?;

        Ok(())
    }

    pub async fn get_workflow(&self, name: &str) -> Result<Option<WdlDocument>> {
        let row = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT document FROM workflows WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to get workflow: {}", e)))?;

        match row {
            Some((document_json,)) => {
                let document = serde_json::from_str(&document_json)
                    .map_err(|e| SprocketError::SerializationError(e))?;
                Ok(Some(document))
            }
            None => Ok(None),
        }
    }

    pub async fn list_workflows(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT name FROM workflows ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to list workflows: {}", e)))?;

        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    pub async fn store_workflow_execution(&self, execution: &WorkflowExecution) -> Result<()> {
        let inputs_json = serde_json::to_string(&execution.inputs)
            .map_err(|e| SprocketError::SerializationError(e))?;
        let outputs_json = serde_json::to_string(&execution.outputs)
            .map_err(|e| SprocketError::SerializationError(e))?;

        sqlx::query(
            r#"
            INSERT INTO workflow_executions (id, workflow_name, status, inputs, outputs, start_time, end_time)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                outputs = excluded.outputs,
                end_time = excluded.end_time
            "#
        )
        .bind(execution.id.to_string())
        .bind(&execution.workflow.name)
        .bind(format!("{:?}", execution.status))
        .bind(inputs_json)
        .bind(outputs_json)
        .bind(execution.start_time)
        .bind(execution.end_time)
        .execute(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to store workflow execution: {}", e)))?;

        Ok(())
    }

    pub async fn store_task_execution(
        &self,
        execution: &TaskExecution,
        workflow_id: Option<Uuid>,
    ) -> Result<()> {
        let inputs_json = serde_json::to_string(&execution.inputs)
            .map_err(|e| SprocketError::SerializationError(e))?;
        let outputs_json = serde_json::to_string(&execution.outputs)
            .map_err(|e| SprocketError::SerializationError(e))?;

        sqlx::query(
            r#"
            INSERT INTO task_executions (
                id, workflow_execution_id, task_name, status, inputs, outputs,
                command_executed, stdout, stderr, exit_code, start_time, end_time
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                outputs = excluded.outputs,
                stdout = excluded.stdout,
                stderr = excluded.stderr,
                exit_code = excluded.exit_code,
                end_time = excluded.end_time
            "#,
        )
        .bind(execution.id.to_string())
        .bind(workflow_id.map(|id| id.to_string()))
        .bind(&execution.task.name)
        .bind(format!("{:?}", execution.status))
        .bind(inputs_json)
        .bind(outputs_json)
        .bind(&execution.command_executed)
        .bind(&execution.stdout)
        .bind(&execution.stderr)
        .bind(execution.exit_code)
        .bind(execution.start_time)
        .bind(execution.end_time)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!("Failed to store task execution: {}", e))
        })?;

        Ok(())
    }

    pub async fn get_workflow_execution(&self, id: Uuid) -> Result<Option<WorkflowExecution>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r#"
            SELECT workflow_name, status, inputs, outputs, start_time, end_time
            FROM workflow_executions WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!("Failed to get workflow execution: {}", e))
        })?;

        match row {
            Some((workflow_name, status_str, inputs_json, outputs_json, start_time, end_time)) => {
                // Get the workflow document
                let workflow_doc = self
                    .get_workflow(&workflow_name)
                    .await?
                    .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_name.clone()))?;

                let workflow = workflow_doc
                    .workflows
                    .into_iter()
                    .find(|w| w.name == workflow_name)
                    .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_name))?;

                // Get associated task executions
                let task_ids = sqlx::query_as::<_, (String,)>(
                    r#"
                    SELECT id FROM task_executions WHERE workflow_execution_id = ?
                    "#,
                )
                .bind(id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    SprocketError::ExecutionError(format!("Failed to get task executions: {}", e))
                })?;

                let task_execution_ids: Vec<Uuid> = task_ids
                    .into_iter()
                    .filter_map(|(id_str,)| Uuid::parse_str(&id_str).ok())
                    .collect();

                let status = match status_str.as_str() {
                    "Queued" => WorkflowStatus::Queued,
                    "Running" => WorkflowStatus::Running,
                    "Completed" => WorkflowStatus::Completed,
                    "Failed" => WorkflowStatus::Failed,
                    "Canceled" => WorkflowStatus::Canceled,
                    _ => WorkflowStatus::Failed,
                };

                let inputs: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(&inputs_json)
                        .map_err(|e| SprocketError::SerializationError(e))?;

                let outputs: std::collections::HashMap<String, serde_json::Value> = outputs_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();

                Ok(Some(WorkflowExecution {
                    id,
                    workflow,
                    status,
                    inputs,
                    outputs,
                    task_executions: task_execution_ids,
                    start_time,
                    end_time,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn get_task_execution(&self, id: Uuid) -> Result<Option<TaskExecution>> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<i32>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
            r#"
            SELECT task_name, status, inputs, outputs, command_executed, stdout, stderr, exit_code, start_time, end_time
            FROM task_executions WHERE id = ?
            "#
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SprocketError::ExecutionError(format!("Failed to get task execution: {}", e)))?;

        match row {
            Some((
                task_name,
                status_str,
                inputs_json,
                outputs_json,
                command_executed,
                stdout,
                stderr,
                exit_code,
                start_time,
                end_time,
            )) => {
                // For simplicity, create a basic task structure
                // In a real implementation, you might want to store the full task definition
                let task = crate::parser::Task {
                    name: task_name,
                    inputs: vec![],
                    command: command_executed.clone().unwrap_or_default(),
                    outputs: vec![],
                };

                let status = match status_str.as_str() {
                    "Queued" => TaskStatus::Queued,
                    "Running" => TaskStatus::Running,
                    "Completed" => TaskStatus::Completed,
                    "Failed" => TaskStatus::Failed,
                    "Canceled" => TaskStatus::Canceled,
                    _ => TaskStatus::Failed,
                };

                let inputs: std::collections::HashMap<String, String> =
                    serde_json::from_str(&inputs_json)
                        .map_err(|e| SprocketError::SerializationError(e))?;

                let outputs: std::collections::HashMap<String, String> = outputs_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();

                Ok(Some(TaskExecution {
                    id,
                    task,
                    status,
                    inputs,
                    outputs,
                    command_executed,
                    stdout,
                    stderr,
                    start_time,
                    end_time,
                    exit_code,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn list_workflow_executions(
        &self,
        limit: i64,
    ) -> Result<Vec<(Uuid, String, String)>> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, workflow_name, status 
            FROM workflow_executions 
            ORDER BY created_at DESC 
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            SprocketError::ExecutionError(format!("Failed to list workflow executions: {}", e))
        })?;

        let mut results = Vec::new();
        for (id_str, name, status) in rows {
            if let Ok(id) = Uuid::parse_str(&id_str) {
                results.push((id, name, status));
            }
        }

        Ok(results)
    }
}
