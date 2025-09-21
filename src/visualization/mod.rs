use crate::parser::{TaskCall, Workflow};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub struct WorkflowVisualizer;

impl WorkflowVisualizer {
    pub fn generate_dot(workflow: &Workflow) -> String {
        let mut dot = String::new();
        
        writeln!(&mut dot, "digraph {} {{", workflow.name).unwrap_or(());
        writeln!(&mut dot, "  rankdir=TB;").unwrap_or(());
        writeln!(&mut dot, "  node [shape=box, style=rounded];").unwrap_or(());
        writeln!(&mut dot, "").unwrap_or(());
        
        writeln!(&mut dot, "  // Workflow inputs").unwrap_or(());
        writeln!(&mut dot, "  subgraph cluster_inputs {{").unwrap_or(());
        writeln!(&mut dot, "    label=\"Inputs\";").unwrap_or(());
        writeln!(&mut dot, "    style=filled;").unwrap_or(());
        writeln!(&mut dot, "    color=lightgrey;").unwrap_or(());
        
        for input in &workflow.inputs {
            writeln!(
                &mut dot,
                "    \"input_{}\" [label=\"{}: {}\", shape=ellipse];",
                input.name,
                input.name,
                format!("{:?}", input.data_type).replace("\"", "")
            ).unwrap_or(());
        }
        writeln!(&mut dot, "  }}").unwrap_or(());
        writeln!(&mut dot, "").unwrap_or(());
        
        writeln!(&mut dot, "  // Tasks").unwrap_or(());
        for (idx, call) in workflow.calls.iter().enumerate() {
            let node_id = call.alias.as_ref().unwrap_or(&call.task_name);
            writeln!(
                &mut dot,
                "  \"task_{}\" [label=\"{}\", style=\"filled,rounded\", fillcolor=lightblue];",
                node_id, call.task_name
            ).unwrap_or(());
        }
        writeln!(&mut dot, "").unwrap_or(());
        
        writeln!(&mut dot, "  // Dependencies").unwrap_or(());
        let dependencies = Self::analyze_dependencies(&workflow.calls);
        
        for call in &workflow.calls {
            let node_id = call.alias.as_ref().unwrap_or(&call.task_name);
            
            for (input_name, input_ref) in &call.inputs {
                if input_ref.contains('.') {
                    let parts: Vec<&str> = input_ref.split('.').collect();
                    if parts.len() == 2 {
                        let dep_task = parts[0];
                        writeln!(
                            &mut dot,
                            "  \"task_{}\" -> \"task_{}\" [label=\"{}\"];",
                            dep_task, node_id, parts[1]
                        ).unwrap_or(());
                    }
                } else if workflow.inputs.iter().any(|i| i.name == *input_ref) {
                    writeln!(
                        &mut dot,
                        "  \"input_{}\" -> \"task_{}\" [label=\"{}\"];",
                        input_ref, node_id, input_name
                    ).unwrap_or(());
                }
            }
        }
        writeln!(&mut dot, "").unwrap_or(());
        
        writeln!(&mut dot, "  // Workflow outputs").unwrap_or(());
        writeln!(&mut dot, "  subgraph cluster_outputs {{").unwrap_or(());
        writeln!(&mut dot, "    label=\"Outputs\";").unwrap_or(());
        writeln!(&mut dot, "    style=filled;").unwrap_or(());
        writeln!(&mut dot, "    color=lightgreen;").unwrap_or(());
        
        for output in &workflow.outputs {
            writeln!(
                &mut dot,
                "    \"output_{}\" [label=\"{}: {}\", shape=ellipse];",
                output.name,
                output.name,
                format!("{:?}", output.data_type).replace("\"", "")
            ).unwrap_or(());
            
            if output.expression.contains('.') {
                let parts: Vec<&str> = output.expression.split('.').collect();
                if parts.len() == 2 {
                    let source_task = parts[0];
                    writeln!(
                        &mut dot,
                        "  \"task_{}\" -> \"output_{}\" [label=\"{}\"];",
                        source_task, output.name, parts[1]
                    ).unwrap_or(());
                }
            }
        }
        writeln!(&mut dot, "  }}").unwrap_or(());
        
        writeln!(&mut dot, "}}").unwrap_or(());
        
        dot
    }
    
    fn analyze_dependencies(calls: &[TaskCall]) -> HashMap<String, HashSet<String>> {
        let mut deps = HashMap::new();
        
        for call in calls {
            let node_id = call.alias.as_ref().unwrap_or(&call.task_name).clone();
            let mut task_deps = HashSet::new();
            
            for (_input_name, input_ref) in &call.inputs {
                if input_ref.contains('.') {
                    let parts: Vec<&str> = input_ref.split('.').collect();
                    if parts.len() == 2 {
                        task_deps.insert(parts[0].to_string());
                    }
                }
            }
            
            deps.insert(node_id, task_deps);
        }
        
        deps
    }
    
    pub fn get_execution_order(workflow: &Workflow) -> Vec<String> {
        let dependencies = Self::analyze_dependencies(&workflow.calls);
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        
        fn visit(
            node: &str,
            dependencies: &HashMap<String, HashSet<String>>,
            visited: &mut HashSet<String>,
            order: &mut Vec<String>,
        ) {
            if visited.contains(node) {
                return;
            }
            
            if let Some(deps) = dependencies.get(node) {
                for dep in deps {
                    visit(dep, dependencies, visited, order);
                }
            }
            
            visited.insert(node.to_string());
            order.push(node.to_string());
        }
        
        for call in &workflow.calls {
            let node_id = call.alias.as_ref().unwrap_or(&call.task_name);
            visit(node_id, &dependencies, &mut visited, &mut order);
        }
        
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{DataType, TaskInput, TaskOutput};
    use std::collections::HashMap;

    fn create_test_workflow() -> Workflow {
        Workflow {
            name: "TestWorkflow".to_string(),
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
            calls: vec![
                TaskCall {
                    task_name: "process_data".to_string(),
                    alias: None,
                    inputs: {
                        let mut inputs = HashMap::new();
                        inputs.insert("file".to_string(), "input_file".to_string());
                        inputs.insert("threshold".to_string(), "threshold".to_string());
                        inputs
                    },
                },
                TaskCall {
                    task_name: "analyze_results".to_string(),
                    alias: None,
                    inputs: {
                        let mut inputs = HashMap::new();
                        inputs.insert("data".to_string(), "process_data.output".to_string());
                        inputs
                    },
                },
            ],
            outputs: vec![
                TaskOutput {
                    name: "final_result".to_string(),
                    data_type: DataType::File,
                    expression: "analyze_results.result".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_generate_dot() {
        let workflow = create_test_workflow();
        let dot = WorkflowVisualizer::generate_dot(&workflow);
        
        assert!(dot.contains("digraph TestWorkflow"));
        assert!(dot.contains("input_file"));
        assert!(dot.contains("threshold"));
        assert!(dot.contains("process_data"));
        assert!(dot.contains("analyze_results"));
        assert!(dot.contains("final_result"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_analyze_dependencies() {
        let workflow = create_test_workflow();
        let deps = WorkflowVisualizer::analyze_dependencies(&workflow.calls);
        
        assert_eq!(deps.len(), 2);
        assert!(deps.get("process_data").unwrap_or(&HashSet::new()).is_empty());
        assert!(deps.get("analyze_results").unwrap_or(&HashSet::new()).contains("process_data"));
    }

    #[test]
    fn test_get_execution_order() {
        let workflow = create_test_workflow();
        let order = WorkflowVisualizer::get_execution_order(&workflow);
        
        assert_eq!(order.len(), 2);
        let process_idx = order.iter().position(|x| x == "process_data").unwrap_or(usize::MAX);
        let analyze_idx = order.iter().position(|x| x == "analyze_results").unwrap_or(usize::MAX);
        assert!(process_idx < analyze_idx);
    }

    #[test]
    fn test_workflow_with_no_dependencies() {
        let workflow = Workflow {
            name: "IndependentTasks".to_string(),
            inputs: vec![
                TaskInput {
                    name: "input1".to_string(),
                    data_type: DataType::String,
                    default: None,
                },
            ],
            calls: vec![
                TaskCall {
                    task_name: "task1".to_string(),
                    alias: None,
                    inputs: {
                        let mut inputs = HashMap::new();
                        inputs.insert("data".to_string(), "input1".to_string());
                        inputs
                    },
                },
                TaskCall {
                    task_name: "task2".to_string(),
                    alias: None,
                    inputs: {
                        let mut inputs = HashMap::new();
                        inputs.insert("data".to_string(), "input1".to_string());
                        inputs
                    },
                },
            ],
            outputs: vec![],
        };
        
        let deps = WorkflowVisualizer::analyze_dependencies(&workflow.calls);
        assert!(deps.get("task1").unwrap_or(&HashSet::new()).is_empty());
        assert!(deps.get("task2").unwrap_or(&HashSet::new()).is_empty());
        
        let order = WorkflowVisualizer::get_execution_order(&workflow);
        assert_eq!(order.len(), 2);
    }
}