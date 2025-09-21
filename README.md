# Sprocket - Workflow Execution Engine

A simplified workflow execution engine inspired by the Workflow Description Language (WDL) standard, designed for genomic analysis pipelines and scientific computing workflows.

## Overview

Sprocket enables scientists to describe complex computational pipelines using a human-readable domain-specific language (DSL) and execute them at scale. Built for the Princeton-Plainsboro Teaching Hospital's pediatric genomics research team, it provides a robust platform for analyzing terabytes of genomic data to help diagnose rare diseases in children.

## Features

### Core Capabilities
- **WDL-Inspired DSL**: Simple, human-readable syntax for defining tasks and workflows
- **Parallel Task Execution**: Automatic dependency analysis and concurrent execution
- **Docker Container Support**: Run tasks in isolated Docker containers with resource limits
- **Workflow Caching**: Intelligent caching based on inputs to avoid redundant computation
- **Workflow Visualization**: Generate GraphViz diagrams of workflow dependencies
- **Task Retry Logic**: Configurable retry mechanism with exponential backoff
- **REST API**: Full-featured API for workflow submission and monitoring
- **SQLite Storage**: Persistent storage with optimized indexing and auto-creation
- **Workflow Imports**: Support for importing and reusing tasks from external WDL files
- **Resource Requirements**: Define CPU, memory, and disk requirements for tasks
- **Enhanced Error Messages**: Line-number aware error reporting for better debugging

### Production Features
- Comprehensive error handling without panics
- Audit trail with preserved error logs
- Resource management and cleanup
- Async/concurrent execution
- Database transaction support

## Installation

### Prerequisites
- Rust 1.80+ (install from [rustup.rs](https://rustup.rs))
- SQLite 3.x
- Docker (optional, for container execution)
- GraphViz (optional, for visualization)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/sprocket.git
cd sprocket

# Build the project
cargo build --release

# Run tests
cargo test

# Start the server
cargo run --release
```

### Docker Installation

```bash
# Build the Docker image
docker build -t sprocket:latest .

# Run with Docker
docker run -d \
  --name sprocket \
  -p 3000:3000 \
  -v sprocket-data:/data \
  -v ./workflow_workspace:/app/workflow_workspace \
  sprocket:latest

# Or use Docker Compose
docker-compose up -d
```

## Quick Start

### 1. Start the Server

```bash
# Default configuration (port 3000, SQLite database)
cargo run

# Custom database location
DATABASE_URL=sqlite:mydata.db cargo run

# With debug logging
RUST_LOG=debug cargo run
```

### 2. Write a Workflow

Create a file `hello_world.wdl`:

```wdl
version 1.0

workflow HelloWorld {
  input {
    String name
    File input_file
  }
  
  call ProcessData {
    input: 
      file = input_file,
      prefix = name
  }
  
  call Analyze {
    input: 
      data = ProcessData.result
  }
  
  output {
    File final_result = Analyze.output
  }
}

task ProcessData {
  input {
    File file
    String prefix
  }
  
  command <<<
    echo "Processing ${file} with prefix ${prefix}"
    cat ${file} | wc -l > result.txt
  >>>
  
  output {
    File result = "result.txt"
  }
}

task Analyze {
  input {
    File data
  }
  
  command <<<
    echo "Analyzing ${data}"
    cat ${data} > output.txt
    echo "Analysis complete" >> output.txt
  >>>
  
  output {
    File output = "output.txt"
  }
}
```

### 3. Submit the Workflow

```bash
# Submit workflow for execution
curl -X POST http://localhost:3000/workflows \
  -H "Content-Type: application/json" \
  -d @- <<EOF
{
  "workflow_source": "$(cat hello_world.wdl)",
  "workflow_name": "HelloWorld",
  "inputs": {
    "name": "TestRun",
    "input_file": "/path/to/data.txt"
  }
}
EOF

# Response: {"workflow_id": "550e8400-e29b-41d4-a716-446655440000", "status": "queued"}
```

### 4. Check Status

```bash
# Get workflow status
curl http://localhost:3000/workflows/550e8400-e29b-41d4-a716-446655440000

# Get workflow results
curl http://localhost:3000/workflows/550e8400-e29b-41d4-a716-446655440000/results
```

### 5. Visualize Workflow

```bash
# Generate workflow visualization
curl -X POST http://localhost:3000/workflows/visualize \
  -H "Content-Type: application/json" \
  -d '{
    "workflow_source": "$(cat hello_world.wdl)",
    "workflow_name": "HelloWorld"
  }' | jq -r .dot_graph > workflow.dot

# Render with GraphViz
dot -Tpng workflow.dot -o workflow.png
```

## Advanced Features

### Workflow Imports

Import and reuse tasks from other WDL files:

```wdl
version 1.0

import "common_tasks.wdl" as common

workflow AnalysisWithImport {
  input {
    File raw_data
  }
  
  call common.QualityCheck {
    input: data_file = raw_data
  }
  
  call common.DataValidation {
    input: input_file = raw_data
  }
  
  output {
    File qc_result = common.QualityCheck.result
    File validation = common.DataValidation.count
  }
}
```

### Resource Requirements

Define resource requirements for tasks:

```wdl
task ResourceIntensiveTask {
  input {
    File large_dataset
  }
  
  runtime {
    docker: "genomics:latest"
    cpu: 4
    memory: "8GB"
    disk: "100GB"
  }
  
  command <<<
    process_large_dataset ${large_dataset}
  >>>
  
  output {
    File result = "processed.dat"
  }
}
```

### Docker Support

Tasks can also run in Docker containers using the `#DOCKER:` directive:

```wdl
task RunInContainer {
  input {
    File input_data
  }
  
  command <<<
    #DOCKER: image=python:3.9, memory=512m, cpu=0.5
    python -c "
    import pandas as pd
    df = pd.read_csv('${input_data}')
    print(df.describe())
    "
  >>>
  
  output {
    String stats = stdout()
  }
}
```

### Workflow Caching

Workflows are automatically cached based on their inputs. To control caching:

```rust
// In your code, configure cache TTL (in hours)
let executor = WorkflowExecutor::new_with_cache(
    task_executions,
    workflow_executions,
    24  // Cache for 24 hours
);
```

### Task Retry Configuration

Tasks automatically retry on failure with exponential backoff:

```rust
// Configure retry behavior
let executor = TaskExecutor::with_retry_config(
    executions,
    3,     // max retries
    1000,  // base delay in ms
);
```

### Parallel Execution

Tasks with no dependencies run in parallel automatically. The execution engine analyzes the workflow DAG and executes independent tasks concurrently.

## API Documentation

See [API.md](docs/API.md) for complete API reference.

## DSL Syntax

See [DSL_SYNTAX.md](docs/DSL_SYNTAX.md) for WDL syntax documentation.

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | SQLite database path | `sqlite:sprocket.db` |
| `PORT` | Server port | `3000` |
| `RUST_LOG` | Log level | `info` |
| `CACHE_TTL_HOURS` | Workflow cache TTL | `24` |
| `MAX_TASK_RETRIES` | Maximum task retry attempts | `3` |

### Database Schema

Sprocket uses SQLite with the following tables:
- `workflows` - Workflow definitions
- `workflow_executions` - Workflow execution history
- `task_executions` - Task execution details

Indexes are automatically created for optimal query performance.

## Development

### Project Structure

```
sprocket/
├── src/
│   ├── api/           # REST API handlers and routes
│   ├── cache/         # Workflow caching implementation
│   ├── execution/     # Task and workflow executors
│   ├── parser/        # WDL parser (nom-based)
│   ├── storage/       # Database storage layer
│   └── visualization/ # Workflow visualization
├── tests/             # Integration tests
├── examples/          # Example workflows
└── docs/             # Documentation
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test parser::tests

# Run with coverage
cargo tarpaulin --out Html
```

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Performance

Sprocket is designed for high-performance workflow execution:

- **Async I/O**: Non-blocking execution using Tokio
- **Connection Pooling**: Efficient database connection management
- **Query Optimization**: Indexed queries for fast lookups
- **Memory Efficiency**: Streaming for large outputs
- **Caching**: Intelligent result caching to skip redundant work

## Security

- No hardcoded credentials
- Input validation and sanitization
- SQL injection prevention via parameterized queries
- Docker container isolation
- Secure error handling without information leakage

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by the [WDL specification](https://github.com/openwdl/wdl)
- Built for Princeton-Plainsboro Teaching Hospital
- Powered by Rust and the amazing crates ecosystem

## Support

For issues, questions, or contributions, please visit our [GitHub repository](https://github.com/your-org/sprocket).
