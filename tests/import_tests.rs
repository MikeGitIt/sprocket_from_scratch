use sprocket::parser::parse_wdl;

#[test]
fn test_parse_import() {
    let wdl = r#"version 1.0

import "common_tasks.wdl" as common

workflow AnalysisWithImport {
  input {
    File raw_data
  }
  
  call SomeTask {}
  
  output {
    String result = SomeTask.message
  }
}

task SomeTask {
  input {}
  
  command <<<
    echo "test"
  >>>
  
  output {
    String message = stdout()
  }
}"#;

    let doc = parse_wdl(wdl).expect("Failed to parse WDL with import");
    
    assert_eq!(doc.version, Some("1.0".to_string()));
    assert_eq!(doc.imports.len(), 1);
    assert_eq!(doc.imports[0].uri, "common_tasks.wdl");
    assert_eq!(doc.imports[0].alias, Some("common".to_string()));
    assert_eq!(doc.tasks.len(), 1);
    assert_eq!(doc.workflows.len(), 1);
}

#[test]
fn test_parse_multiple_imports() {
    let wdl = r#"version 1.0

import "tasks/alignment.wdl" as align
import "tasks/qc.wdl" as qc
import "utils.wdl"

workflow Pipeline {
  input {
    File data
  }
  
  call Task1 {}
}

task Task1 {
  command <<<
    echo "test"
  >>>
}"#;

    let doc = parse_wdl(wdl).expect("Failed to parse WDL with multiple imports");
    
    assert_eq!(doc.imports.len(), 3);
    assert_eq!(doc.imports[0].alias, Some("align".to_string()));
    assert_eq!(doc.imports[1].alias, Some("qc".to_string()));
    assert_eq!(doc.imports[2].alias, None);
}