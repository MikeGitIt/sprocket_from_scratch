use super::engine::{TaskExecution, TaskStatus};
use crate::error::{Result, SprocketError};
use crate::parser::Task;
use chrono::Utc;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct TaskExecutor {
    executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>,
}

impl TaskExecutor {
    pub fn new(executions: Arc<RwLock<HashMap<Uuid, TaskExecution>>>) -> Self {
        Self { executions }
    }

    pub async fn execute(&self, task: Task, inputs: HashMap<String, String>) -> Result<Uuid> {
        let execution_id = Uuid::new_v4();
        let mut execution = TaskExecution {
            id: execution_id,
            task: task.clone(),
            status: TaskStatus::Queued,
            inputs: inputs.clone(),
            outputs: HashMap::new(),
            command_executed: None,
            stdout: None,
            stderr: None,
            start_time: Some(Utc::now()),
            end_time: None,
            exit_code: None,
        };

        // Store initial execution state
        {
            let mut execs = self.executions.write().await;
            execs.insert(execution_id, execution.clone());
        }

        // Execute the task
        self.run_task(&mut execution).await?;

        // Update final execution state
        {
            let mut execs = self.executions.write().await;
            execs.insert(execution_id, execution);
        }

        Ok(execution_id)
    }

    async fn run_task(&self, execution: &mut TaskExecution) -> Result<()> {
        execution.status = TaskStatus::Running;

        // Substitute variables in command
        let command = self.substitute_variables(&execution.task.command, &execution.inputs)?;
        execution.command_executed = Some(command.clone());

        // Execute command
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output()
            .await
            .map_err(|e| SprocketError::CommandFailed(e.to_string()))?;

        execution.stdout = Some(String::from_utf8_lossy(&output.stdout).to_string());
        execution.stderr = Some(String::from_utf8_lossy(&output.stderr).to_string());
        execution.exit_code = output.status.code();
        execution.end_time = Some(Utc::now());

        if output.status.success() {
            execution.status = TaskStatus::Completed;
            // Extract outputs
            self.extract_outputs(execution)?;
        } else {
            execution.status = TaskStatus::Failed;
        }

        Ok(())
    }

    fn substitute_variables(
        &self,
        command: &str,
        inputs: &HashMap<String, String>,
    ) -> Result<String> {
        let mut result = command.to_string();

        // Simple variable substitution using ${var} pattern
        let re = Regex::new(r"\$\{([^}]+)\}")
            .map_err(|e| SprocketError::ExecutionError(e.to_string()))?;

        for cap in re.captures_iter(command) {
            if let Some(var_name) = cap.get(1) {
                let var_name_str = var_name.as_str();
                if let Some(value) = inputs.get(var_name_str) {
                    result = result.replace(&format!("${{{}}}", var_name_str), value);
                } else {
                    return Err(SprocketError::VariableNotFound(var_name_str.to_string()));
                }
            }
        }

        Ok(result)
    }

    fn extract_outputs(&self, execution: &mut TaskExecution) -> Result<()> {
        // For each output defined in the task, extract its value
        for output in &execution.task.outputs {
            let value = self.evaluate_output_expression(
                &output.expression,
                execution.stdout.as_deref(),
                execution.stderr.as_deref(),
            )?;
            execution.outputs.insert(output.name.clone(), value);
        }

        Ok(())
    }

    fn evaluate_output_expression(
        &self,
        expression: &str,
        stdout: Option<&str>,
        _stderr: Option<&str>,
    ) -> Result<String> {
        // Simple output expression evaluation
        if expression == "stdout()" {
            Ok(stdout.unwrap_or("").to_string())
        } else if expression.starts_with("\"") && expression.ends_with("\"") {
            // String literal
            Ok(expression[1..expression.len() - 1].to_string())
        } else if expression.starts_with("read_file(") && expression.ends_with(")") {
            // Extract filename from read_file() function call
            let filename = expression[10..expression.len() - 1]
                .trim()
                .trim_matches('"');

            // Read the actual file
            match std::fs::read_to_string(filename) {
                Ok(contents) => Ok(contents.trim().to_string()),
                Err(e) => Err(SprocketError::ExecutionError(format!(
                    "Failed to read file '{}': {}",
                    filename, e
                ))),
            }
        } else {
            // Assume it's a file path
            Ok(expression.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{DataType, TaskInput, TaskOutput};

    fn create_test_task(name: &str) -> Task {
        Task {
            name: name.to_string(),
            inputs: vec![
                TaskInput {
                    name: "input_file".to_string(),
                    data_type: DataType::File,
                    default: None,
                },
                TaskInput {
                    name: "threshold".to_string(),
                    data_type: DataType::Int,
                    default: Some("10".to_string()),
                },
            ],
            command: "echo \"Processing ${input_file} with threshold ${threshold}\"".to_string(),
            outputs: vec![TaskOutput {
                name: "result".to_string(),
                data_type: DataType::String,
                expression: "stdout()".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn test_task_executor_successful_execution() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        let task = create_test_task("test_task");
        let mut inputs = HashMap::new();
        inputs.insert("input_file".to_string(), "test.txt".to_string());
        inputs.insert("threshold".to_string(), "25".to_string());

        let execution_id = executor.execute(task, inputs).await;
        assert!(execution_id.is_ok());

        let exec_id = execution_id.unwrap();
        let execs = executions.read().await;
        let execution = execs.get(&exec_id);

        assert!(execution.is_some());
        let exec = execution.unwrap();
        assert_eq!(exec.status, TaskStatus::Completed);
        assert!(exec.command_executed.is_some());
        assert!(exec.command_executed.as_ref().unwrap().contains("test.txt"));
        assert!(exec.command_executed.as_ref().unwrap().contains("25"));
        assert!(exec.stdout.is_some());
        assert!(exec.outputs.contains_key("result"));
    }

    #[tokio::test]
    async fn test_task_executor_missing_input_variable() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        let task = create_test_task("test_task");
        let mut inputs = HashMap::new();
        // Missing required input_file variable
        inputs.insert("threshold".to_string(), "25".to_string());

        let result = executor.execute(task, inputs).await;
        assert!(result.is_err());

        match result {
            Err(SprocketError::VariableNotFound(var)) => {
                assert_eq!(var, "input_file");
            }
            _ => panic!("Expected VariableNotFound error"),
        }
    }

    #[test]
    fn test_substitute_variables_simple() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let command = "echo ${name} ${age}";
        let mut inputs = HashMap::new();
        inputs.insert("name".to_string(), "Alice".to_string());
        inputs.insert("age".to_string(), "30".to_string());

        let result = executor.substitute_variables(command, &inputs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "echo Alice 30");
    }

    #[test]
    fn test_substitute_variables_nested_braces() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let command = "grep '${pattern}' ${file} | wc -l > ${output}";
        let mut inputs = HashMap::new();
        inputs.insert("pattern".to_string(), "error".to_string());
        inputs.insert("file".to_string(), "/var/log/app.log".to_string());
        inputs.insert("output".to_string(), "count.txt".to_string());

        let result = executor.substitute_variables(command, &inputs);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "grep 'error' /var/log/app.log | wc -l > count.txt"
        );
    }

    #[test]
    fn test_substitute_variables_missing_variable() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let command = "echo ${name} ${missing_var}";
        let mut inputs = HashMap::new();
        inputs.insert("name".to_string(), "Bob".to_string());

        let result = executor.substitute_variables(command, &inputs);
        assert!(result.is_err());

        match result {
            Err(SprocketError::VariableNotFound(var)) => {
                assert_eq!(var, "missing_var");
            }
            _ => panic!("Expected VariableNotFound error"),
        }
    }

    #[test]
    fn test_substitute_variables_no_variables() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let command = "echo 'Hello World'";
        let inputs = HashMap::new();

        let result = executor.substitute_variables(command, &inputs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "echo 'Hello World'");
    }

    #[test]
    fn test_substitute_variables_complex_paths() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let command = "cat ${input_dir}/${filename}.${extension} | grep ${pattern}";
        let mut inputs = HashMap::new();
        inputs.insert("input_dir".to_string(), "/data/genomics".to_string());
        inputs.insert("filename".to_string(), "sample_001".to_string());
        inputs.insert("extension".to_string(), "fastq".to_string());
        inputs.insert("pattern".to_string(), "ATCG".to_string());

        let result = executor.substitute_variables(command, &inputs);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "cat /data/genomics/sample_001.fastq | grep ATCG"
        );
    }

    #[test]
    fn test_evaluate_output_expression_stdout() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let stdout_content = "Hello from stdout";
        let result = executor.evaluate_output_expression("stdout()", Some(stdout_content), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello from stdout");
    }

    #[test]
    fn test_evaluate_output_expression_stdout_empty() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let result = executor.evaluate_output_expression("stdout()", None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_evaluate_output_expression_string_literal() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let result = executor.evaluate_output_expression("\"output.txt\"", None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "output.txt");
    }

    #[test]
    fn test_evaluate_output_expression_string_literal_with_spaces() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let result = executor.evaluate_output_expression("\"output file.txt\"", None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "output file.txt");
    }

    #[test]
    fn test_evaluate_output_expression_read_file() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        // Create a temporary test file
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("sprocket_test_output.txt");
        std::fs::write(&test_file, "42\n").unwrap();

        let expression = format!("read_file(\"{}\")", test_file.display());
        let result = executor.evaluate_output_expression(&expression, None, None);

        // Clean up
        std::fs::remove_file(test_file).unwrap();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "42");
    }

    #[test]
    fn test_evaluate_output_expression_read_file_nonexistent() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let result =
            executor.evaluate_output_expression("read_file(\"/nonexistent/file.txt\")", None, None);
        assert!(result.is_err());

        match result {
            Err(SprocketError::ExecutionError(msg)) => {
                assert!(msg.contains("Failed to read file"));
            }
            _ => panic!("Expected ExecutionError for nonexistent file"),
        }
    }

    #[test]
    fn test_evaluate_output_expression_plain_path() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions);

        let result = executor.evaluate_output_expression("/path/to/file.txt", None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/path/to/file.txt");
    }

    #[tokio::test]
    async fn test_task_execution_with_command_failure() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        let task = Task {
            name: "fail_task".to_string(),
            inputs: vec![],
            command: "exit 1".to_string(),
            outputs: vec![],
        };

        let inputs = HashMap::new();
        let execution_id = executor.execute(task, inputs).await;
        assert!(execution_id.is_ok());

        let exec_id = execution_id.unwrap();
        let execs = executions.read().await;
        let execution = execs.get(&exec_id);

        assert!(execution.is_some());
        let exec = execution.unwrap();
        assert_eq!(exec.status, TaskStatus::Failed);
        assert_eq!(exec.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_task_execution_with_stderr_output() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        let task = Task {
            name: "stderr_task".to_string(),
            inputs: vec![],
            command: "echo 'Error message' >&2; echo 'Normal output'".to_string(),
            outputs: vec![TaskOutput {
                name: "stdout_result".to_string(),
                data_type: DataType::String,
                expression: "stdout()".to_string(),
            }],
        };

        let inputs = HashMap::new();
        let execution_id = executor.execute(task, inputs).await;
        assert!(execution_id.is_ok());

        let exec_id = execution_id.unwrap();
        let execs = executions.read().await;
        let execution = execs.get(&exec_id);

        assert!(execution.is_some());
        let exec = execution.unwrap();
        assert_eq!(exec.status, TaskStatus::Completed);
        assert!(exec.stderr.as_ref().unwrap().contains("Error message"));
        assert!(exec.stdout.as_ref().unwrap().contains("Normal output"));
        assert!(exec
            .outputs
            .get("stdout_result")
            .unwrap()
            .contains("Normal output"));
    }

    #[tokio::test]
    async fn test_task_execution_tracking_timestamps() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        let task = Task {
            name: "timing_task".to_string(),
            inputs: vec![],
            command: "sleep 0.1".to_string(),
            outputs: vec![],
        };

        let inputs = HashMap::new();
        let execution_id = executor.execute(task, inputs).await;
        assert!(execution_id.is_ok());

        let exec_id = execution_id.unwrap();
        let execs = executions.read().await;
        let execution = execs.get(&exec_id);

        assert!(execution.is_some());
        let exec = execution.unwrap();
        assert!(exec.start_time.is_some());
        assert!(exec.end_time.is_some());

        // Verify end time is after start time
        let duration = exec.end_time.unwrap() - exec.start_time.unwrap();
        assert!(duration.num_milliseconds() >= 0);
    }

    #[tokio::test]
    async fn test_extract_outputs_multiple_types() {
        let executions = Arc::new(RwLock::new(HashMap::new()));
        let executor = TaskExecutor::new(executions.clone());

        // Create temporary file for read_file test
        let temp_dir = std::env::temp_dir();
        let count_file = temp_dir.join("sprocket_count.txt");
        std::fs::write(&count_file, "100\n").unwrap();

        let task = Task {
            name: "multi_output_task".to_string(),
            inputs: vec![],
            command: format!("echo 'Test output'; echo '100' > {}", count_file.display()),
            outputs: vec![
                TaskOutput {
                    name: "message".to_string(),
                    data_type: DataType::String,
                    expression: "stdout()".to_string(),
                },
                TaskOutput {
                    name: "output_file".to_string(),
                    data_type: DataType::File,
                    expression: "\"results.txt\"".to_string(),
                },
                TaskOutput {
                    name: "count".to_string(),
                    data_type: DataType::Int,
                    expression: format!("read_file(\"{}\")", count_file.display()),
                },
            ],
        };

        let inputs = HashMap::new();
        let execution_id = executor.execute(task, inputs).await;

        // Clean up
        std::fs::remove_file(count_file).unwrap();

        assert!(execution_id.is_ok());

        let exec_id = execution_id.unwrap();
        let execs = executions.read().await;
        let execution = execs.get(&exec_id);

        assert!(execution.is_some());
        let exec = execution.unwrap();
        assert_eq!(exec.status, TaskStatus::Completed);
        assert_eq!(exec.outputs.len(), 3);
        assert!(exec.outputs.get("message").unwrap().contains("Test output"));
        assert_eq!(exec.outputs.get("output_file").unwrap(), "results.txt");
        assert_eq!(exec.outputs.get("count").unwrap(), "100");
    }
}
