use super::engine::{TaskExecution, WorkflowExecution, WorkflowStatus};
use super::task_executor::TaskExecutor;
use crate::cache::WorkflowCache;
use crate::error::{Result, SprocketError};
use crate::parser::{TaskCall, WdlDocument};
use chrono::Utc;
use futures::future;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct WorkflowExecutor {
    task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
    workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
    cache: Option<Arc<WorkflowCache>>,
}

impl WorkflowExecutor {
    pub fn new(
        task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
        workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
    ) -> Self {
        Self {
            task_executions,
            workflow_executions,
            cache: None,
        }
    }

    pub fn new_with_cache(
        task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
        workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
        ttl_hours: i64,
    ) -> Self {
        Self {
            task_executions,
            workflow_executions,
            cache: Some(Arc::new(WorkflowCache::new(ttl_hours))),
        }
    }

    pub fn with_cache(
        task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
        workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
        cache: Arc<WorkflowCache>,
    ) -> Self {
        Self {
            task_executions,
            workflow_executions,
            cache: Some(cache),
        }
    }

    pub async fn execute(
        &self,
        wdl_document: WdlDocument,
        workflow_name: String,
        mut inputs: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        let workflow = wdl_document
            .workflows
            .iter()
            .find(|w| w.name == workflow_name)
            .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_name.clone()))?
            .clone();

        for workflow_input in &workflow.inputs {
            if !inputs.contains_key(&workflow_input.name) {
                if let Some(default_value) = &workflow_input.default {
                    inputs.insert(workflow_input.name.clone(), serde_json::Value::String(default_value.clone()));
                }
            }
        }

        if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(&workflow_name, &inputs).await {
                let workflow_id = cached_result.workflow_id;
                let workflow_execution = WorkflowExecution {
                    id: workflow_id,
                    workflow: workflow.clone(),
                    status: WorkflowStatus::Completed,
                    inputs: inputs.clone(),
                    outputs: cached_result.outputs.clone(),
                    task_executions: Vec::new(),
                    start_time: Some(cached_result.created_at),
                    end_time: Some(Utc::now()),
                };

                let mut execs = self.workflow_executions.write().await;
                execs.insert(workflow_id, workflow_execution);
                
                return Ok(workflow_id);
            }
        }

        let workflow_id = Uuid::new_v4();
        let mut workflow_execution = WorkflowExecution {
            id: workflow_id,
            workflow: workflow.clone(),
            status: WorkflowStatus::Queued,
            inputs: inputs.clone(),
            outputs: HashMap::new(),
            task_executions: Vec::new(),
            start_time: Some(Utc::now()),
            end_time: None,
        };

        {
            let mut execs = self.workflow_executions.write().await;
            execs.insert(workflow_id, workflow_execution.clone());
        }

        let result = self
            .run_workflow(&mut workflow_execution, &wdl_document)
            .await;

        // Update final workflow state
        match result {
            Ok(_) => {
                let task_executions = self.task_executions.read().await;
                let any_task_failed = workflow_execution.task_executions.iter()
                    .any(|task_id| {
                        task_executions.get(task_id)
                            .map(|exec| exec.status == super::engine::TaskStatus::Failed)
                            .unwrap_or(false)
                    });
                
                if any_task_failed {
                    workflow_execution.status = WorkflowStatus::Failed;
                } else {
                    workflow_execution.status = WorkflowStatus::Completed;
                }
            }
            Err(_) => {
                workflow_execution.status = WorkflowStatus::Failed;
            }
        }
        workflow_execution.end_time = Some(Utc::now());

        if workflow_execution.status == WorkflowStatus::Completed {
            if let Some(cache) = &self.cache {
                if let Err(e) = cache.put(
                    workflow_id,
                    workflow_execution.workflow.name.clone(),
                    &workflow_execution.inputs,
                    workflow_execution.outputs.clone(),
                    "Completed".to_string(),
                ).await {
                    tracing::warn!("Failed to cache workflow result: {}", e);
                }
            }
        }

        {
            let mut execs = self.workflow_executions.write().await;
            execs.insert(workflow_id, workflow_execution);
        }

        result?;
        Ok(workflow_id)
    }

    async fn run_workflow(
        &self,
        workflow_execution: &mut WorkflowExecution,
        wdl_document: &WdlDocument,
    ) -> Result<()> {
        workflow_execution.status = WorkflowStatus::Running;

        let mut completed_tasks = Vec::new();
        let mut remaining_calls = workflow_execution.workflow.calls.clone();

        while !remaining_calls.is_empty() {
            let mut executable_calls = Vec::new();
            let mut deferred_calls = Vec::new();

            for call in remaining_calls {
                if self.can_execute_task(&call, &completed_tasks) {
                    debug!("Task {} is executable", call.task_name);
                    executable_calls.push(call);
                } else {
                    debug!("Task {} is deferred (waiting for dependencies)", call.task_name);
                    deferred_calls.push(call);
                }
            }

            if executable_calls.is_empty() && !deferred_calls.is_empty() {
                let mut dep_info = String::from("Circular dependency detected. Deferred calls:\n");
                for call in &deferred_calls {
                    dep_info.push_str(&format!("  - {} with inputs: {:?}\n", call.task_name, call.inputs));
                }
                return Err(SprocketError::WorkflowExecutionError(dep_info));
            }

            let task_futures: Vec<_> = executable_calls
                .iter()
                .map(|call| {
                    self.execute_task_call(
                        call,
                        wdl_document,
                        &workflow_execution.inputs,
                        &completed_tasks,
                    )
                })
                .collect();

            let results = future::join_all(task_futures).await;
            
            for result in results {
                let task_id = result?;
                completed_tasks.push(task_id);
                workflow_execution.task_executions.push(task_id);
            }

            remaining_calls = deferred_calls;
        }

        self.extract_workflow_outputs(workflow_execution).await?;

        Ok(())
    }

    fn can_execute_task(&self, call: &TaskCall, completed_task_ids: &[Uuid]) -> bool {
        for (_, input_ref) in &call.inputs {
            if input_ref.contains('.') {
                if let Some(last_dot) = input_ref.rfind('.') {
                    let task_ref = &input_ref[..last_dot];
                    
                    let mut found = false;
                    for task_id in completed_task_ids {
                        if let Ok(executions) = self.task_executions.try_read() {
                            if let Some(exec) = executions.get(task_id) {
                                if exec.call_name == task_ref {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !found {
                        return false;
                    }
                }
            }
        }
        true
    }

    async fn execute_task_call(
        &self,
        call: &TaskCall,
        wdl_document: &WdlDocument,
        workflow_inputs: &HashMap<String, serde_json::Value>,
        previous_task_ids: &[Uuid],
    ) -> Result<Uuid> {
        // Find the task definition
        debug!("Looking for task: {} in document with tasks: {:?}", 
            call.task_name, 
            wdl_document.tasks.iter().map(|t| &t.name).collect::<Vec<_>>());
        
        let task = wdl_document
            .tasks
            .iter()
            .find(|t| t.name == call.task_name)
            .ok_or_else(|| SprocketError::TaskNotFound(call.task_name.clone()))?
            .clone();

        // Prepare task inputs
        let mut task_inputs = HashMap::new();

        for (input_name, input_ref) in &call.inputs {
            // Resolve the input value
            let value = self
                .resolve_input_value(input_ref, workflow_inputs, previous_task_ids)
                .await?;
            task_inputs.insert(input_name.clone(), value);
        }

        // Execute the task
        let task_executor = TaskExecutor::new(self.task_executions.clone());
        let call_name = call.task_name.clone();
        task_executor.execute(task, call_name, task_inputs).await
    }

    async fn resolve_input_value(
        &self,
        input_ref: &str,
        workflow_inputs: &HashMap<String, serde_json::Value>,
        previous_task_ids: &[Uuid],
    ) -> Result<String> {
        // Check if it's a workflow input
        if let Some(value) = workflow_inputs.get(input_ref) {
            return Ok(self.json_value_to_string(value));
        }

        // Check if it's a reference to a previous task output (e.g., "task_name.output_name")
        if input_ref.contains('.') {
            let parts: Vec<&str> = input_ref.split('.').collect();
            if parts.len() >= 2 {
                let task_name = if parts.len() == 2 {
                    parts[0].to_string()
                } else {
                    parts[..parts.len()-1].join(".")
                };
                let output_name = parts[parts.len()-1];

                // Find the task execution by name
                let task_executions = self.task_executions.read().await;
                for task_id in previous_task_ids {
                    if let Some(task_exec) = task_executions.get(task_id) {
                        debug!("Checking task {} with call_name {} against {}", task_exec.task.name, task_exec.call_name, task_name);
                        if task_exec.call_name == task_name {
                            debug!("Found matching task, outputs: {:?}", task_exec.outputs);
                            if let Some(output_value) = task_exec.outputs.get(output_name) {
                                return Ok(output_value.clone());
                            } else {
                                debug!("Output {} not found in task outputs", output_name);
                            }
                        }
                    }
                }
            }
        }

        Err(SprocketError::VariableNotFound(input_ref.to_string()))
    }

    async fn extract_workflow_outputs(
        &self,
        workflow_execution: &mut WorkflowExecution,
    ) -> Result<()> {
        let task_executions = self.task_executions.read().await;
        let all_tasks_succeeded = workflow_execution.task_executions.iter()
            .all(|task_id| {
                task_executions.get(task_id)
                    .map(|exec| exec.status == super::engine::TaskStatus::Completed)
                    .unwrap_or(false)
            });

        if all_tasks_succeeded {
            for output in &workflow_execution.workflow.outputs {
                let value = self
                    .resolve_input_value(
                        &output.expression,
                        &workflow_execution.inputs,
                        &workflow_execution.task_executions,
                    )
                    .await?;

                workflow_execution
                    .outputs
                    .insert(output.name.clone(), serde_json::Value::String(value));
            }
        }

        Ok(())
    }

    fn json_value_to_string(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => value.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::engine::TaskStatus;
    use crate::parser::{DataType, Task, TaskInput, TaskOutput, Workflow};

    fn create_test_wdl_document() -> WdlDocument {
        WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![
                Task {
                    name: "task1".to_string(),
                    inputs: vec![TaskInput {
                        name: "input1".to_string(),
                        data_type: DataType::String,
                        default: None,
                    }],
                    command: "echo ${input1} > output1.txt".to_string(),
                    outputs: vec![TaskOutput {
                        name: "output1".to_string(),
                        data_type: DataType::String,
                        expression: "stdout()".to_string(),
                    }],
                    runtime: None,
                },
                Task {
                    name: "task2".to_string(),
                    inputs: vec![TaskInput {
                        name: "input2".to_string(),
                        data_type: DataType::String,
                        default: None,
                    }],
                    command: "echo \"Received: ${input2}\"".to_string(),
                    outputs: vec![TaskOutput {
                        name: "final_output".to_string(),
                        data_type: DataType::String,
                        expression: "stdout()".to_string(),
                    }],
                    runtime: None,
                },
            ],
            workflows: vec![Workflow {
                name: "test_workflow".to_string(),
                inputs: vec![TaskInput {
                    name: "workflow_input".to_string(),
                    data_type: DataType::String,
                    default: None,
                }],
                calls: vec![
                    TaskCall {
                        task_name: "task1".to_string(),
                        alias: None,
                        inputs: HashMap::from([("input1".to_string(), "workflow_input".to_string())]),
                    },
                    TaskCall {
                        task_name: "task2".to_string(),
                        alias: None,
                        inputs: HashMap::from([("input2".to_string(), "task1.output1".to_string())]),
                    },
                ],
                outputs: vec![TaskOutput {
                    name: "workflow_output".to_string(),
                    data_type: DataType::String,
                    expression: "task2.final_output".to_string(),
                }],
            }],
        }
    }

    #[tokio::test]
    async fn test_workflow_executor_simple_workflow() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions.clone(), workflow_executions.clone());

        let wdl_doc = create_test_wdl_document();
        let mut inputs = HashMap::new();
        inputs.insert(
            "workflow_input".to_string(),
            serde_json::Value::String("Hello".to_string()),
        );

        let workflow_id = executor
            .execute(wdl_doc, "test_workflow".to_string(), inputs)
            .await;
        assert!(workflow_id.is_ok());

        let wf_id = workflow_id.unwrap();
        let wf_execs = workflow_executions.read().await;
        let workflow_exec = wf_execs.get(&wf_id);

        assert!(workflow_exec.is_some());
        let wf_exec = workflow_exec.unwrap();
        assert_eq!(wf_exec.status, WorkflowStatus::Completed);
        assert_eq!(wf_exec.task_executions.len(), 2);
        assert!(wf_exec.outputs.contains_key("workflow_output"));
    }

    #[tokio::test]
    async fn test_workflow_executor_workflow_not_found() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let wdl_doc = create_test_wdl_document();
        let inputs = HashMap::new();

        let result = executor
            .execute(wdl_doc, "nonexistent_workflow".to_string(), inputs)
            .await;
        assert!(result.is_err());

        match result {
            Err(SprocketError::WorkflowNotFound(name)) => {
                assert_eq!(name, "nonexistent_workflow");
            }
            _ => panic!("Expected WorkflowNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_workflow_executor_task_not_found() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let wdl_doc = WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![], // No tasks defined
            workflows: vec![Workflow {
                name: "broken_workflow".to_string(),
                inputs: vec![],
                calls: vec![TaskCall {
                    task_name: "missing_task".to_string(),
                    alias: None,
                    inputs: HashMap::new(),
                }],
                outputs: vec![],
            }],
        };

        let inputs = HashMap::new();
        let result = executor
            .execute(wdl_doc, "broken_workflow".to_string(), inputs)
            .await;
        assert!(result.is_err());

        match result {
            Err(SprocketError::TaskNotFound(name)) => {
                assert_eq!(name, "missing_task");
            }
            _ => panic!("Expected TaskNotFound error"),
        }
    }

    #[test]
    fn test_json_value_to_string_string() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let value = serde_json::Value::String("test string".to_string());
        assert_eq!(executor.json_value_to_string(&value), "test string");
    }

    #[test]
    fn test_json_value_to_string_number() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let value = serde_json::Value::Number(serde_json::Number::from(42));
        assert_eq!(executor.json_value_to_string(&value), "42");

        let float_value = serde_json::json!(3.14);
        assert_eq!(executor.json_value_to_string(&float_value), "3.14");
    }

    #[test]
    fn test_json_value_to_string_bool() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let value_true = serde_json::Value::Bool(true);
        assert_eq!(executor.json_value_to_string(&value_true), "true");

        let value_false = serde_json::Value::Bool(false);
        assert_eq!(executor.json_value_to_string(&value_false), "false");
    }

    #[test]
    fn test_json_value_to_string_array() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let value = serde_json::json!(["item1", "item2", "item3"]);
        assert_eq!(
            executor.json_value_to_string(&value),
            "[\"item1\",\"item2\",\"item3\"]"
        );
    }

    #[tokio::test]
    async fn test_resolve_input_value_from_workflow_input() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let mut workflow_inputs = HashMap::new();
        workflow_inputs.insert(
            "test_input".to_string(),
            serde_json::Value::String("test_value".to_string()),
        );

        let result = executor
            .resolve_input_value("test_input", &workflow_inputs, &[])
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_value");
    }

    #[tokio::test]
    async fn test_resolve_input_value_from_task_output() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions.clone(), workflow_executions);

        // Create a mock task execution
        let task_id = Uuid::new_v4();
        let task_exec = TaskExecution {
            id: task_id,
            task: Task {
                name: "previous_task".to_string(),
                inputs: vec![],
                command: "echo test".to_string(),
                outputs: vec![],
                runtime: None,
            },
            call_name: "previous_task".to_string(),
            status: TaskStatus::Completed,
            inputs: HashMap::new(),
            outputs: {
                let mut outputs = HashMap::new();
                outputs.insert("result".to_string(), "task_output_value".to_string());
                outputs
            },
            command_executed: None,
            stdout: None,
            stderr: None,
            start_time: None,
            end_time: None,
            exit_code: None,
            retry_count: 0,
            errors: Vec::new(),
        };

        {
            let mut task_execs = task_executions.write().await;
            task_execs.insert(task_id, task_exec);
        }

        let workflow_inputs = HashMap::new();
        let result = executor
            .resolve_input_value("previous_task.result", &workflow_inputs, &[task_id])
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "task_output_value");
    }

    #[tokio::test]
    async fn test_resolve_input_value_not_found() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions);

        let workflow_inputs = HashMap::new();
        let result = executor
            .resolve_input_value("nonexistent_input", &workflow_inputs, &[])
            .await;
        assert!(result.is_err());

        match result {
            Err(SprocketError::VariableNotFound(var)) => {
                assert_eq!(var, "nonexistent_input");
            }
            _ => panic!("Expected VariableNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_workflow_caching() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new_with_cache(
            task_executions.clone(),
            workflow_executions.clone(),
            1,
        );

        let wdl_document = WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![Task {
                name: "simple_task".to_string(),
                inputs: vec![TaskInput {
                    name: "message".to_string(),
                    data_type: DataType::String,
                    default: None,
                }],
                command: "echo ${message}".to_string(),
                outputs: vec![TaskOutput {
                    name: "result".to_string(),
                    data_type: DataType::String,
                    expression: "stdout()".to_string(),
                }],
                runtime: None,
            }],
            workflows: vec![Workflow {
                name: "CachedWorkflow".to_string(),
                inputs: vec![TaskInput {
                    name: "input_message".to_string(),
                    data_type: DataType::String,
                    default: None,
                }],
                calls: vec![TaskCall {
                    task_name: "simple_task".to_string(),
                    alias: None,
                    inputs: {
                        let mut inputs = HashMap::new();
                        inputs.insert("message".to_string(), "input_message".to_string());
                        inputs
                    },
                }],
                outputs: vec![TaskOutput {
                    name: "final_result".to_string(),
                    data_type: DataType::String,
                    expression: "simple_task.result".to_string(),
                }],
            }],
        };

        let mut inputs = HashMap::new();
        inputs.insert(
            "input_message".to_string(),
            serde_json::json!("Hello Cache"),
        );

        let first_id = executor
            .execute(wdl_document.clone(), "CachedWorkflow".to_string(), inputs.clone())
            .await;
        assert!(first_id.is_ok());

        let second_id = executor
            .execute(wdl_document, "CachedWorkflow".to_string(), inputs)
            .await;
        assert!(second_id.is_ok());

        assert_eq!(first_id.unwrap(), second_id.unwrap());

        let workflow_execs = workflow_executions.read().await;
        assert_eq!(workflow_execs.len(), 1);
    }

    #[tokio::test]
    async fn test_workflow_with_multiple_task_dependencies() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions.clone(), workflow_executions.clone());

        let wdl_doc = WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![
                Task {
                    name: "preprocess".to_string(),
                    inputs: vec![TaskInput {
                        name: "raw_data".to_string(),
                        data_type: DataType::File,
                        default: None,
                    }],
                    command: "echo \"Preprocessing ${raw_data}\"".to_string(),
                    outputs: vec![TaskOutput {
                        name: "processed_data".to_string(),
                        data_type: DataType::File,
                        expression: "\"processed.txt\"".to_string(),
                    }],
                    runtime: None,
                },
                Task {
                    name: "analyze".to_string(),
                    inputs: vec![
                        TaskInput {
                            name: "data".to_string(),
                            data_type: DataType::File,
                            default: None,
                        },
                        TaskInput {
                            name: "threshold".to_string(),
                            data_type: DataType::Int,
                            default: None,
                        },
                    ],
                    command: "echo \"Analyzing ${data} with threshold ${threshold}\"".to_string(),
                    outputs: vec![TaskOutput {
                        name: "results".to_string(),
                        data_type: DataType::File,
                        expression: "\"results.txt\"".to_string(),
                    }],
                    runtime: None,
                },
                Task {
                    name: "summarize".to_string(),
                    inputs: vec![TaskInput {
                        name: "analysis_results".to_string(),
                        data_type: DataType::File,
                        default: None,
                    }],
                    command: "echo \"Summary of ${analysis_results}\"".to_string(),
                    outputs: vec![TaskOutput {
                        name: "summary".to_string(),
                        data_type: DataType::String,
                        expression: "stdout()".to_string(),
                    }],
                    runtime: None,
                },
            ],
            workflows: vec![Workflow {
                name: "analysis_pipeline".to_string(),
                inputs: vec![
                    TaskInput {
                        name: "input_file".to_string(),
                        data_type: DataType::File,
                        default: None,
                    },
                    TaskInput {
                        name: "quality_threshold".to_string(),
                        data_type: DataType::Int,
                        default: Some("30".to_string()),
                    },
                ],
                calls: vec![
                    TaskCall {
                        task_name: "preprocess".to_string(),
                        alias: None,
                        inputs: HashMap::from([("raw_data".to_string(), "input_file".to_string())]),
                    },
                    TaskCall {
                        task_name: "analyze".to_string(),
                        alias: None,
                        inputs: HashMap::from([
                            ("data".to_string(), "preprocess.processed_data".to_string()),
                            ("threshold".to_string(), "quality_threshold".to_string()),
                        ]),
                    },
                    TaskCall {
                        task_name: "summarize".to_string(),
                        alias: None,
                        inputs: HashMap::from([(
                            "analysis_results".to_string(),
                            "analyze.results".to_string(),
                        )]),
                    },
                ],
                outputs: vec![
                    TaskOutput {
                        name: "final_summary".to_string(),
                        data_type: DataType::String,
                        expression: "summarize.summary".to_string(),
                    },
                    TaskOutput {
                        name: "analysis_file".to_string(),
                        data_type: DataType::File,
                        expression: "analyze.results".to_string(),
                    },
                ],
            }],
        };

        let mut inputs = HashMap::new();
        inputs.insert(
            "input_file".to_string(),
            serde_json::Value::String("data.fastq".to_string()),
        );
        inputs.insert(
            "quality_threshold".to_string(),
            serde_json::Value::Number(serde_json::Number::from(40)),
        );

        let workflow_id = executor
            .execute(wdl_doc, "analysis_pipeline".to_string(), inputs)
            .await;
        assert!(workflow_id.is_ok());

        let wf_id = workflow_id.unwrap();
        let wf_execs = workflow_executions.read().await;
        let workflow_exec = wf_execs.get(&wf_id);

        assert!(workflow_exec.is_some());
        let wf_exec = workflow_exec.unwrap();
        assert_eq!(wf_exec.status, WorkflowStatus::Completed);
        assert_eq!(wf_exec.task_executions.len(), 3);
        assert!(wf_exec.outputs.contains_key("final_summary"));
        assert!(wf_exec.outputs.contains_key("analysis_file"));

        // Verify all tasks completed successfully
        let task_execs = task_executions.read().await;
        for task_id in &wf_exec.task_executions {
            let task_exec = task_execs.get(task_id);
            assert!(task_exec.is_some());
            assert_eq!(task_exec.unwrap().status, TaskStatus::Completed);
        }
    }

    #[tokio::test]
    async fn test_workflow_execution_timestamps() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions.clone());

        let wdl_doc = WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![Task {
                name: "simple_task".to_string(),
                inputs: vec![],
                command: "echo 'test'".to_string(),
                outputs: vec![],
                runtime: None,
            }],
            workflows: vec![Workflow {
                name: "simple_workflow".to_string(),
                inputs: vec![],
                calls: vec![TaskCall {
                    task_name: "simple_task".to_string(),
                    alias: None,
                    inputs: HashMap::new(),
                }],
                outputs: vec![],
            }],
        };

        let inputs = HashMap::new();
        let workflow_id = executor
            .execute(wdl_doc, "simple_workflow".to_string(), inputs)
            .await;
        assert!(workflow_id.is_ok());

        let wf_id = workflow_id.unwrap();
        let wf_execs = workflow_executions.read().await;
        let workflow_exec = wf_execs.get(&wf_id);

        assert!(workflow_exec.is_some());
        let wf_exec = workflow_exec.unwrap();
        assert!(wf_exec.start_time.is_some());
        assert!(wf_exec.end_time.is_some());

        // Verify end time is after start time
        let duration = wf_exec.end_time.unwrap() - wf_exec.start_time.unwrap();
        assert!(duration.num_milliseconds() >= 0);
    }

    #[tokio::test]
    async fn test_workflow_with_default_inputs() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions, workflow_executions.clone());

        let wdl_doc = WdlDocument {
            version: Some("1.0".to_string()),
            imports: vec![],
            tasks: vec![Task {
                name: "greeting".to_string(),
                inputs: vec![TaskInput {
                    name: "name".to_string(),
                    data_type: DataType::String,
                    default: Some("World".to_string()),
                }],
                command: "echo \"Hello ${name}\"".to_string(),
                outputs: vec![TaskOutput {
                    name: "message".to_string(),
                    data_type: DataType::String,
                    expression: "stdout()".to_string(),
                }],
                runtime: None,
            }],
            workflows: vec![Workflow {
                name: "hello_workflow".to_string(),
                inputs: vec![TaskInput {
                    name: "person_name".to_string(),
                    data_type: DataType::String,
                    default: Some("Anonymous".to_string()),
                }],
                calls: vec![TaskCall {
                    task_name: "greeting".to_string(),
                    alias: None,
                    inputs: HashMap::from([("name".to_string(), "person_name".to_string())]),
                }],
                outputs: vec![TaskOutput {
                    name: "greeting_message".to_string(),
                    data_type: DataType::String,
                    expression: "greeting.message".to_string(),
                }],
            }],
        };

        // Execute with custom input
        let mut inputs = HashMap::new();
        inputs.insert(
            "person_name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );

        let workflow_id = executor
            .execute(wdl_doc.clone(), "hello_workflow".to_string(), inputs)
            .await;
        assert!(workflow_id.is_ok());

        let wf_id = workflow_id.unwrap();
        let wf_execs = workflow_executions.read().await;
        let workflow_exec = wf_execs.get(&wf_id);

        assert!(workflow_exec.is_some());
        let wf_exec = workflow_exec.unwrap();
        assert_eq!(wf_exec.status, WorkflowStatus::Completed);
        assert!(wf_exec.outputs.contains_key("greeting_message"));
    }
}
