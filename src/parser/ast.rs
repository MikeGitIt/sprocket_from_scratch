use serde::{Deserialize, Serialize};

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
pub struct Task {
    pub name: String,
    pub inputs: Vec<TaskInput>,
    pub command: String,
    pub outputs: Vec<TaskOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCall {
    pub task_name: String,
    pub alias: Option<String>,
    pub inputs: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub inputs: Vec<TaskInput>,
    pub calls: Vec<TaskCall>,
    pub outputs: Vec<TaskOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdlDocument {
    pub version: Option<String>,
    pub tasks: Vec<Task>,
    pub workflows: Vec<Workflow>,
}
