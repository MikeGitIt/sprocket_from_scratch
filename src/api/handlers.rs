use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

use crate::api::models::*;
use crate::error::Result;
use crate::execution::ExecutionEngine;
use crate::parser::parse_wdl;
use crate::storage::SqliteStore;

pub struct AppState {
    pub engine: ExecutionEngine,
    pub storage: Arc<SqliteStore>,
    pub start_time: std::time::Instant,
}

impl AppState {
    pub async fn new(database_url: &str) -> Result<Self> {
        let storage = Arc::new(SqliteStore::new(database_url).await?);
        let engine = ExecutionEngine::new();

        Ok(Self {
            engine,
            storage,
            start_time: std::time::Instant::now(),
        })
    }
}

pub async fn submit_workflow(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(payload): Json<SubmitWorkflowRequest>,
) -> std::result::Result<Json<SubmitWorkflowResponse>, StatusCode> {
    info!("Received workflow submission request");

    let state = state.read().await;

    // Parse the WDL source
    let wdl_document = match parse_wdl(&payload.workflow_source) {
        Ok(doc) => doc,
        Err(e) => {
            error!("Failed to parse WDL: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Find the first workflow (for simplicity)
    let workflow_name = match wdl_document.workflows.first() {
        Some(w) => w.name.clone(),
        None => {
            error!("No workflow found in WDL document");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Store the workflow definition
    if let Err(e) = state
        .storage
        .store_workflow(workflow_name.clone(), wdl_document.clone())
        .await
    {
        error!("Failed to store workflow: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Execute the workflow
    let workflow_id = match state
        .engine
        .execute_workflow(wdl_document, workflow_name, payload.inputs)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to execute workflow: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Store the workflow execution
    if let Ok(execution) = state.engine.get_workflow_status(workflow_id).await {
        if let Err(e) = state.storage.store_workflow_execution(&execution).await {
            error!("Failed to store workflow execution: {}", e);
        }
    }

    info!("Workflow {} submitted successfully", workflow_id);

    Ok(Json(SubmitWorkflowResponse {
        workflow_id,
        status: "Running".to_string(),
    }))
}

pub async fn get_workflow_status(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(id): Path<String>,
) -> std::result::Result<Json<WorkflowStatusResponse>, StatusCode> {
    let workflow_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let state = state.read().await;

    // Try to get from execution engine first
    let workflow_execution = match state.engine.get_workflow_status(workflow_id).await {
        Ok(exec) => exec,
        Err(_) => {
            // Fall back to storage
            match state.storage.get_workflow_execution(workflow_id).await {
                Ok(Some(exec)) => exec,
                Ok(None) => return Err(StatusCode::NOT_FOUND),
                Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    };

    // Get task statuses
    let tasks = match state.engine.get_workflow_tasks(workflow_id).await {
        Ok(tasks) => tasks,
        Err(_) => Vec::new(),
    };

    let task_responses: Vec<TaskStatusResponse> = tasks
        .iter()
        .map(|task| TaskStatusResponse {
            task_id: task.id,
            task_name: task.task.name.clone(),
            status: format!("{:?}", task.status),
            start_time: task.start_time.map(|t| t.to_rfc3339()),
            end_time: task.end_time.map(|t| t.to_rfc3339()),
        })
        .collect();

    Ok(Json(WorkflowStatusResponse {
        workflow_id,
        status: format!("{:?}", workflow_execution.status),
        tasks: task_responses,
        start_time: workflow_execution.start_time.map(|t| t.to_rfc3339()),
        end_time: workflow_execution.end_time.map(|t| t.to_rfc3339()),
    }))
}

pub async fn get_workflow_results(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(id): Path<String>,
) -> std::result::Result<Json<WorkflowResultsResponse>, StatusCode> {
    let workflow_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let state = state.read().await;

    // Get workflow execution
    let workflow_execution = match state.engine.get_workflow_status(workflow_id).await {
        Ok(exec) => exec,
        Err(_) => match state.storage.get_workflow_execution(workflow_id).await {
            Ok(Some(exec)) => exec,
            Ok(None) => return Err(StatusCode::NOT_FOUND),
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
    };

    // Get task results
    let tasks = match state.engine.get_workflow_tasks(workflow_id).await {
        Ok(tasks) => tasks,
        Err(_) => Vec::new(),
    };

    let task_results: Vec<TaskResultResponse> = tasks
        .iter()
        .map(|task| TaskResultResponse {
            task_id: task.id,
            task_name: task.task.name.clone(),
            status: format!("{:?}", task.status),
            outputs: task.outputs.clone(),
            stdout: task.stdout.clone(),
            stderr: task.stderr.clone(),
            exit_code: task.exit_code,
        })
        .collect();

    Ok(Json(WorkflowResultsResponse {
        workflow_id,
        status: format!("{:?}", workflow_execution.status),
        outputs: workflow_execution.outputs,
        tasks: task_results,
    }))
}

pub async fn get_task_status(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(id): Path<String>,
) -> std::result::Result<Json<TaskDetailsResponse>, StatusCode> {
    let task_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let state = state.read().await;

    // Get task execution
    let task_execution = match state.engine.get_task_status(task_id).await {
        Ok(exec) => exec,
        Err(_) => match state.storage.get_task_execution(task_id).await {
            Ok(Some(exec)) => exec,
            Ok(None) => return Err(StatusCode::NOT_FOUND),
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
    };

    Ok(Json(TaskDetailsResponse {
        task_id,
        task_name: task_execution.task.name.clone(),
        status: format!("{:?}", task_execution.status),
        inputs: task_execution.inputs,
        outputs: task_execution.outputs,
        command_executed: task_execution.command_executed,
        stdout: task_execution.stdout,
        stderr: task_execution.stderr,
        exit_code: task_execution.exit_code,
        start_time: task_execution.start_time.map(|t| t.to_rfc3339()),
        end_time: task_execution.end_time.map(|t| t.to_rfc3339()),
    }))
}

pub async fn health_check(State(state): State<Arc<RwLock<AppState>>>) -> Json<HealthResponse> {
    let state = state.read().await;
    let uptime = state.start_time.elapsed().as_secs();

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    })
}
