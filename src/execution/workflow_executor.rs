use super::engine::{TaskExecution, WorkflowExecution, WorkflowStatus};
use super::task_executor::TaskExecutor;
use crate::error::{Result, SprocketError};
use crate::parser::{TaskCall, WdlDocument};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct WorkflowExecutor {
    task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
    workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
}

impl WorkflowExecutor {
    pub fn new(
        task_executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
        workflow_executions: Arc<RwLock<HashMap<Uuid, WorkflowExecution>>>,
    ) -> Self {
        Self {
            task_executions,
            workflow_executions,
        }
    }

    pub async fn execute(
        &self,
        wdl_document: WdlDocument,
        workflow_name: String,
        inputs: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        // Find the workflow
        let workflow = wdl_document
            .workflows
            .iter()
            .find(|w| w.name == workflow_name)
            .ok_or_else(|| SprocketError::WorkflowNotFound(workflow_name.clone()))?
            .clone();

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

        // Store initial workflow state
        {
            let mut execs = self.workflow_executions.write().await;
            execs.insert(workflow_id, workflow_execution.clone());
        }

        // Execute the workflow
        let result = self
            .run_workflow(&mut workflow_execution, &wdl_document)
            .await;

        // Update final workflow state
        match result {
            Ok(_) => {
                workflow_execution.status = WorkflowStatus::Completed;
            }
            Err(_) => {
                workflow_execution.status = WorkflowStatus::Failed;
            }
        }
        workflow_execution.end_time = Some(Utc::now());

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

        // Execute each task call in sequence
        for call in &workflow_execution.workflow.calls {
            let task_id = self
                .execute_task_call(
                    call,
                    wdl_document,
                    &workflow_execution.inputs,
                    &workflow_execution.task_executions,
                )
                .await?;

            workflow_execution.task_executions.push(task_id);
        }

        // Extract workflow outputs
        self.extract_workflow_outputs(workflow_execution).await?;

        Ok(())
    }

    async fn execute_task_call(
        &self,
        call: &TaskCall,
        wdl_document: &WdlDocument,
        workflow_inputs: &HashMap<String, serde_json::Value>,
        previous_task_ids: &[Uuid],
    ) -> Result<Uuid> {
        // Find the task definition
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
        task_executor.execute(task, task_inputs).await
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
            if parts.len() == 2 {
                let task_name = parts[0];
                let output_name = parts[1];

                // Find the task execution by name
                let task_executions = self.task_executions.read().await;
                for task_id in previous_task_ids {
                    if let Some(task_exec) = task_executions.get(task_id) {
                        if task_exec.task.name == task_name {
                            if let Some(output_value) = task_exec.outputs.get(output_name) {
                                return Ok(output_value.clone());
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
                        inputs: vec![("input1".to_string(), "workflow_input".to_string())],
                    },
                    TaskCall {
                        task_name: "task2".to_string(),
                        alias: None,
                        inputs: vec![("input2".to_string(), "task1.output1".to_string())],
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
            tasks: vec![], // No tasks defined
            workflows: vec![Workflow {
                name: "broken_workflow".to_string(),
                inputs: vec![],
                calls: vec![TaskCall {
                    task_name: "missing_task".to_string(),
                    alias: None,
                    inputs: vec![],
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
            },
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
    async fn test_workflow_with_multiple_task_dependencies() {
        let task_executions = Arc::new(RwLock::new(HashMap::new()));
        let workflow_executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = WorkflowExecutor::new(task_executions.clone(), workflow_executions.clone());

        let wdl_doc = WdlDocument {
            version: Some("1.0".to_string()),
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
                        inputs: vec![("raw_data".to_string(), "input_file".to_string())],
                    },
                    TaskCall {
                        task_name: "analyze".to_string(),
                        alias: None,
                        inputs: vec![
                            ("data".to_string(), "preprocess.processed_data".to_string()),
                            ("threshold".to_string(), "quality_threshold".to_string()),
                        ],
                    },
                    TaskCall {
                        task_name: "summarize".to_string(),
                        alias: None,
                        inputs: vec![(
                            "analysis_results".to_string(),
                            "analyze.results".to_string(),
                        )],
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
            tasks: vec![Task {
                name: "simple_task".to_string(),
                inputs: vec![],
                command: "echo 'test'".to_string(),
                outputs: vec![],
            }],
            workflows: vec![Workflow {
                name: "simple_workflow".to_string(),
                inputs: vec![],
                calls: vec![TaskCall {
                    task_name: "simple_task".to_string(),
                    alias: None,
                    inputs: vec![],
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
                    inputs: vec![("name".to_string(), "person_name".to_string())],
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
