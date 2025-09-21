use sprocket::parser::wdl_parser::parse_wdl;

#[test]
fn test_parse_namespaced_task_calls() {
    let source = r#"
version 1.0

import "examples/common_tasks.wdl" as common

workflow AnalysisWithImport {
  input {
    File raw_data
    Int quality_threshold = 25
  }
  
  call common.QualityCheck {
    input:
      data_file = raw_data,
      threshold = quality_threshold
  }
  
  call common.DataValidation {
    input:
      input_file = raw_data
  }
  
  output {
    File qc_result = QualityCheck.result
    File validation = DataValidation.count
  }
}
"#;

    let result = parse_wdl(source);
    assert!(result.is_ok(), "Failed to parse WDL with namespaced calls: {:?}", result);
    
    let doc = result.unwrap();
    assert_eq!(doc.imports.len(), 1);
    assert_eq!(doc.imports[0].uri, "examples/common_tasks.wdl");
    assert_eq!(doc.imports[0].alias, Some("common".to_string()));
    
    assert_eq!(doc.workflows.len(), 1);
    let workflow = &doc.workflows[0];
    assert_eq!(workflow.calls.len(), 2);
    assert_eq!(workflow.calls[0].task_name, "common.QualityCheck");
    assert_eq!(workflow.calls[1].task_name, "common.DataValidation");
}

#[test]
fn test_mixed_simple_and_namespaced_calls() {
    let source = r#"
version 1.0

import "lib/tasks.wdl" as lib

workflow MixedCalls {
  input {
    File data
  }
  
  call LocalTask {
    input: file = data
  }
  
  call lib.ImportedTask {
    input: file = LocalTask.output
  }
  
  output {
    File result = ImportedTask.result
  }
}

task LocalTask {
  input {
    File file
  }
  
  command <<<
    echo "Processing ${file}"
  >>>
  
  output {
    File output = "local_output.txt"
  }
}
"#;

    let result = parse_wdl(source);
    assert!(result.is_ok(), "Failed to parse mixed calls: {:?}", result);
    
    let doc = result.unwrap();
    assert_eq!(doc.workflows[0].calls.len(), 2);
    assert_eq!(doc.workflows[0].calls[0].task_name, "LocalTask");
    assert_eq!(doc.workflows[0].calls[1].task_name, "lib.ImportedTask");
}