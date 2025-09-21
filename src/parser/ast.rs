use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    String,
    Int,
    Float,
    File,
    Boolean,
    Array(Box<DataType>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInput {
    pub name: String,
    pub data_type: DataType,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutput {
    pub name: String,
    pub data_type: DataType,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu: Option<f32>,
    pub memory: Option<String>,
    pub disk: Option<String>,
    pub docker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    pub inputs: Vec<TaskInput>,
    pub command: String,
    pub outputs: Vec<TaskOutput>,
    pub runtime: Option<ResourceRequirements>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCall {
    pub task_name: String,
    pub alias: Option<String>,
    pub inputs: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub inputs: Vec<TaskInput>,
    pub calls: Vec<TaskCall>,
    pub outputs: Vec<TaskOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub uri: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdlDocument {
    pub version: Option<String>,
    pub imports: Vec<Import>,
    pub tasks: Vec<Task>,
    pub workflows: Vec<Workflow>,
}
