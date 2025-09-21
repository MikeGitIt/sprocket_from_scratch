use crate::error::{Result, SprocketError};
use crate::parser::{ResourceRequirements, Task};
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct DockerConfig {
    pub image: String,
    pub volumes: Vec<String>,
    pub environment: HashMap<String, String>,
    pub working_dir: Option<String>,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: "ubuntu:latest".to_string(),
            volumes: Vec::new(),
            environment: HashMap::new(),
            working_dir: None,
            memory_limit: None,
            cpu_limit: None,
        }
    }
}

pub struct DockerExecutor;

impl DockerExecutor {
    pub async fn check_docker_available() -> Result<bool> {
        match Command::new("docker")
            .arg("--version")
            .output()
            .await
        {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn execute_in_container(
        config: &DockerConfig,
        command: &str,
        inputs: &HashMap<String, String>,
    ) -> Result<(String, String, i32)> {
        if !Self::check_docker_available().await? {
            return Err(SprocketError::ExecutionError(
                "Docker is not available on this system".to_string(),
            ));
        }

        let container_name = format!("sprocket_{}", uuid::Uuid::new_v4());
        
        let mut docker_args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            container_name.clone(),
        ];

        if let Some(memory) = &config.memory_limit {
            docker_args.push("--memory".to_string());
            docker_args.push(memory.clone());
        }

        if let Some(cpu) = &config.cpu_limit {
            docker_args.push("--cpus".to_string());
            docker_args.push(cpu.clone());
        }

        if let Some(workdir) = &config.working_dir {
            docker_args.push("--workdir".to_string());
            docker_args.push(workdir.clone());
        }

        for volume in &config.volumes {
            docker_args.push("-v".to_string());
            docker_args.push(volume.clone());
        }

        for (key, value) in &config.environment {
            docker_args.push("-e".to_string());
            docker_args.push(format!("{}={}", key, value));
        }

        for (key, value) in inputs {
            docker_args.push("-e".to_string());
            docker_args.push(format!("{}={}", key, value));
        }

        docker_args.push(config.image.clone());
        docker_args.push("sh".to_string());
        docker_args.push("-c".to_string());
        docker_args.push(command.to_string());

        let output = Command::new("docker")
            .args(&docker_args)
            .output()
            .await
            .map_err(|e| SprocketError::ExecutionError(format!(
                "Failed to execute Docker container: {}",
                e
            )))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, exit_code))
    }

    pub async fn pull_image(image: &str) -> Result<()> {
        let output = Command::new("docker")
            .args(&["pull", image])
            .output()
            .await
            .map_err(|e| SprocketError::ExecutionError(format!(
                "Failed to pull Docker image: {}",
                e
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SprocketError::ExecutionError(format!(
                "Failed to pull Docker image '{}': {}",
                image, stderr
            )));
        }

        Ok(())
    }

    pub fn parse_docker_config_from_task(task: &Task) -> Option<DockerConfig> {
        let runtime_section = task.command.lines()
            .find(|line| line.trim().starts_with("#DOCKER:"))?;
        
        let config_str = runtime_section.trim_start_matches("#DOCKER:").trim();
        
        let mut config = DockerConfig::default();
        
        for part in config_str.split(',') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                match key.trim() {
                    "image" => config.image = value.trim().to_string(),
                    "memory" => config.memory_limit = Some(value.trim().to_string()),
                    "cpu" => config.cpu_limit = Some(value.trim().to_string()),
                    "workdir" => config.working_dir = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }
        
        Some(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_config_default() {
        let config = DockerConfig::default();
        assert_eq!(config.image, "ubuntu:latest");
        assert!(config.volumes.is_empty());
        assert!(config.environment.is_empty());
        assert!(config.working_dir.is_none());
        assert!(config.memory_limit.is_none());
        assert!(config.cpu_limit.is_none());
    }

    #[test]
    fn test_parse_docker_config_from_task() {
        let task = Task {
            name: "docker_task".to_string(),
            inputs: vec![],
            command: r#"
#DOCKER: image=python:3.9, memory=512m, cpu=0.5, workdir=/app
python -c "print('Hello from Docker')"
"#.to_string(),
            outputs: vec![],
            runtime: Some(ResourceRequirements {
                cpu: Some(0.5),
                memory: Some("512m".to_string()),
                disk: None,
                docker: Some("python:3.9".to_string()),
            }),
        };

        let config = DockerExecutor::parse_docker_config_from_task(&task);
        assert!(config.is_some());
        
        if let Some(config) = config {
            assert_eq!(config.image, "python:3.9");
            assert_eq!(config.memory_limit, Some("512m".to_string()));
            assert_eq!(config.cpu_limit, Some("0.5".to_string()));
            assert_eq!(config.working_dir, Some("/app".to_string()));
        }
    }

    #[test]
    fn test_parse_docker_config_no_docker_directive() {
        let task = Task {
            name: "regular_task".to_string(),
            inputs: vec![],
            command: "echo 'Regular task'".to_string(),
            outputs: vec![],
            runtime: None,
        };

        let config = DockerExecutor::parse_docker_config_from_task(&task);
        assert!(config.is_none());
    }

    #[test]
    fn test_parse_docker_config_partial() {
        let task = Task {
            name: "docker_task".to_string(),
            inputs: vec![],
            command: r#"
#DOCKER: image=alpine:latest
echo 'Hello from Alpine'
"#.to_string(),
            outputs: vec![],
            runtime: Some(ResourceRequirements {
                cpu: None,
                memory: None,
                disk: None,
                docker: Some("alpine:latest".to_string()),
            }),
        };

        let config = DockerExecutor::parse_docker_config_from_task(&task);
        assert!(config.is_some());
        
        if let Some(config) = config {
            assert_eq!(config.image, "alpine:latest");
            assert!(config.memory_limit.is_none());
            assert!(config.cpu_limit.is_none());
            assert!(config.working_dir.is_none());
        }
    }

    #[tokio::test]
    async fn test_check_docker_available() {
        let result = DockerExecutor::check_docker_available().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_in_container_simple() {
        let available = DockerExecutor::check_docker_available().await.unwrap_or(false);
        if !available {
            return;
        }

        let config = DockerConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };

        let inputs = HashMap::new();
        let result = DockerExecutor::execute_in_container(
            &config,
            "echo 'Hello from container'",
            &inputs,
        ).await;

        if let Ok((stdout, _stderr, exit_code)) = result {
            assert!(stdout.contains("Hello from container"));
            assert_eq!(exit_code, 0);
        }
    }

    #[tokio::test]
    async fn test_execute_with_environment_variables() {
        let available = DockerExecutor::check_docker_available().await.unwrap_or(false);
        if !available {
            return;
        }

        let config = DockerConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };

        let mut inputs = HashMap::new();
        inputs.insert("TEST_VAR".to_string(), "test_value".to_string());

        let result = DockerExecutor::execute_in_container(
            &config,
            "echo $TEST_VAR",
            &inputs,
        ).await;

        if let Ok((stdout, _, exit_code)) = result {
            assert!(stdout.contains("test_value"));
            assert_eq!(exit_code, 0);
        }
    }
}