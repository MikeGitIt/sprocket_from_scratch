use sprocket::api::{create_router, AppState};
use sprocket::storage::sqlite_store::SqliteStore;
use sprocket::execution::engine::ExecutionEngine;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn create_test_app() -> Result<axum::Router, Box<dyn std::error::Error>> {
    let store = Arc::new(SqliteStore::new(":memory:").await?);
    let engine = ExecutionEngine::new();
    let app_state = Arc::new(RwLock::new(AppState {
        engine,
        storage: store,
        start_time: std::time::Instant::now(),
    }));
    Ok(create_router(app_state))
}

#[tokio::test]
async fn test_health_endpoint() -> TestResult {
    let app = create_test_app().await?;

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty())?)
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn test_submit_simple_workflow() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

workflow HelloWorld {
    input {
        String name
    }
    
    call SayHello {
        input: name = name
    }
    
    output {
        String greeting = SayHello.message
    }
}

task SayHello {
    input {
        String name
    }
    
    command <<<
        echo "Hello, ${name}!"
    >>>
    
    output {
        String message = stdout()
    }
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "HelloWorld",
        "inputs": {
            "name": "Integration Test"
        }
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let result: serde_json::Value = serde_json::from_slice(&body)?;
    
    assert!(result["workflow_id"].is_string());
    assert_eq!(result["status"], "queued");
    
    Ok(())
}

#[tokio::test]
async fn test_workflow_execution_and_status() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

task SimpleTask {
    command <<<
        echo "Task executed successfully"
    >>>
    
    output {
        String result = stdout()
    }
}

workflow SimpleWorkflow {
    call SimpleTask
    
    output {
        String final_result = SimpleTask.result
    }
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "SimpleWorkflow",
        "inputs": {}
    });

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(submit_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(submit_response.into_body(), usize::MAX).await?;
    let submit_result: serde_json::Value = serde_json::from_slice(&body)?;
    let workflow_id = submit_result["workflow_id"]
        .as_str()
        .ok_or("Missing workflow_id")?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let status_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/workflows/{}", workflow_id))
                .body(Body::empty())?
        )
        .await?;

    assert_eq!(status_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(status_response.into_body(), usize::MAX).await?;
    let status_result: serde_json::Value = serde_json::from_slice(&body)?;
    
    assert_eq!(status_result["workflow_id"], workflow_id);
    assert!(status_result["status"].is_string());
    assert!(status_result["tasks"].is_array());
    
    Ok(())
}

#[tokio::test]
async fn test_workflow_with_multiple_tasks() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

task PreProcess {
    input {
        String data
    }
    
    command <<<
        echo "Preprocessing: ${data}"
    >>>
    
    output {
        String processed = stdout()
    }
}

task Analyze {
    input {
        String input_data
    }
    
    command <<<
        echo "Analyzing: ${input_data}"
    >>>
    
    output {
        String result = stdout()
    }
}

workflow Pipeline {
    input {
        String raw_data
    }
    
    call PreProcess {
        input: data = raw_data
    }
    
    call Analyze {
        input: input_data = PreProcess.processed
    }
    
    output {
        String final_output = Analyze.result
    }
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "Pipeline",
        "inputs": {
            "raw_data": "test data"
        }
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let result: serde_json::Value = serde_json::from_slice(&body)?;
    let workflow_id = result["workflow_id"]
        .as_str()
        .ok_or("Missing workflow_id")?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let results_response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/workflows/{}/results", workflow_id))
                .body(Body::empty())?
        )
        .await?;

    assert_eq!(results_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(results_response.into_body(), usize::MAX).await?;
    let results: serde_json::Value = serde_json::from_slice(&body)?;
    
    assert!(results["outputs"].is_object());
    assert!(results["tasks"].is_array());
    
    Ok(())
}

#[tokio::test]
async fn test_invalid_workflow_syntax() -> TestResult {
    let app = create_test_app().await?;

    let invalid_workflow = r#"
version 1.0

this is not valid WDL syntax
"#;

    let request_body = json!({
        "workflow_source": invalid_workflow,
        "workflow_name": "InvalidWorkflow",
        "inputs": {}
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let error_result: serde_json::Value = serde_json::from_slice(&body)?;
    
    let error_msg = error_result["error"]
        .as_str()
        .ok_or("Missing error message")?;
    assert!(error_msg.contains("Invalid WDL syntax"));
    
    Ok(())
}

#[tokio::test]
async fn test_workflow_not_found() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

task Test {
    command <<<
        echo "test"
    >>>
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "NonExistentWorkflow",
        "inputs": {}
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let error_result: serde_json::Value = serde_json::from_slice(&body)?;
    
    let error_msg = error_result["error"]
        .as_str()
        .ok_or("Missing error message")?;
    assert!(error_msg.contains("Workflow not found"));
    
    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_workflow_status() -> TestResult {
    let app = create_test_app().await?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/workflows/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn test_task_with_default_values() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

task TaskWithDefaults {
    input {
        String name = "DefaultName"
        Int count = 5
    }
    
    command <<<
        echo "${name} - ${count}"
    >>>
    
    output {
        String result = stdout()
    }
}

workflow WorkflowWithDefaults {
    input {
        String user_name = "User"
    }
    
    call TaskWithDefaults {
        input: name = user_name
    }
    
    output {
        String output = TaskWithDefaults.result
    }
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "WorkflowWithDefaults",
        "inputs": {}
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn test_task_execution_failure() -> TestResult {
    let app = create_test_app().await?;

    let workflow_source = r#"
version 1.0

task FailingTask {
    command <<<
        echo "This task will fail" >&2
        exit 1
    >>>
    
    output {
        String result = stdout()
    }
}

workflow FailingWorkflow {
    call FailingTask
    
    output {
        String output = FailingTask.result
    }
}
"#;

    let request_body = json!({
        "workflow_source": workflow_source,
        "workflow_name": "FailingWorkflow",
        "inputs": {}
    });

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body)?))?
        )
        .await?;

    assert_eq!(submit_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(submit_response.into_body(), usize::MAX).await?;
    let submit_result: serde_json::Value = serde_json::from_slice(&body)?;
    let workflow_id = submit_result["workflow_id"]
        .as_str()
        .ok_or("Missing workflow_id")?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let status_response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/workflows/{}", workflow_id))
                .body(Body::empty())?
        )
        .await?;

    assert_eq!(status_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(status_response.into_body(), usize::MAX).await?;
    let status_result: serde_json::Value = serde_json::from_slice(&body)?;
    
    assert_eq!(status_result["status"], "Failed");
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_workflow_submissions() -> TestResult {
    // Fix: Handle the Result before wrapping in Arc
    let store = SqliteStore::new(":memory:")
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    let store = Arc::new(store);
    
    let engine = ExecutionEngine::new();
    let app_state = Arc::new(RwLock::new(AppState {
        engine,
        storage: store,
        start_time: std::time::Instant::now(),
    }));
    let app = Arc::new(create_router(app_state));

    let workflow_source = r#"
version 1.0

task EchoTask {
    input {
        String message
    }
    
    command <<<
        echo "${message}"
    >>>
    
    output {
        String result = stdout()
    }
}

workflow EchoWorkflow {
    input {
        String input_message
    }
    
    call EchoTask {
        input: message = input_message
    }
    
    output {
        String output = EchoTask.result
    }
}
"#;

    let mut handles = vec![];
    
    for i in 0..5 {
        let app_clone = app.clone();
        let workflow_source_clone = workflow_source.to_string();
        
        let handle = tokio::spawn(async move {
            let request_body = json!({
                "workflow_source": workflow_source_clone,
                "workflow_name": "EchoWorkflow",
                "inputs": {
                    "input_message": format!("Message {}", i)
                }
            });

            let app_instance = (*app_clone).clone();
            let response = app_instance
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/workflows")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_string(&request_body)?))?
                )
                .await?;

            assert_eq!(response.status(), StatusCode::OK);
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });
        
        handles.push(handle);
    }
    
    // Fix: Properly handle the JoinError and inner error
    for handle in handles {
        match handle.await {
            Ok(result) => result.map_err(|e| e as Box<dyn std::error::Error>)?,
            Err(join_err) => return Err(Box::new(join_err) as Box<dyn std::error::Error>),
        }
    }
    
    Ok(())
}
