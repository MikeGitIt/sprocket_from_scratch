use super::ast::*;
use crate::error::{Result, SprocketError};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_until},
    character::complete::{alpha1, alphanumeric1, char, digit1, multispace0, none_of},
    combinator::{map, opt, recognize, value},
    multi::{many0, many1, separated_list0, separated_list1},
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};

fn ws<'a, F, O>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O>
where
    F: FnMut(&'a str) -> IResult<&'a str, O>,
{
    delimited(multispace0, inner, multispace0)
}

fn identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_")))),
    ))(input)
}

fn parse_data_type(input: &str) -> IResult<&str, DataType> {
    alt((
        value(DataType::String, tag("String")),
        value(DataType::Int, tag("Int")),
        value(DataType::Float, tag("Float")),
        value(DataType::File, tag("File")),
        value(DataType::Boolean, tag("Boolean")),
        map(delimited(tag("Array["), parse_data_type, tag("]")), |dt| {
            DataType::Array(Box::new(dt))
        }),
    ))(input)
}

fn parse_default_value(input: &str) -> IResult<&str, Option<String>> {
    opt(preceded(
        ws(tag("=")),
        alt((
            delimited(char('"'), take_until("\""), char('"')),
            digit1,
            tag("true"),
            tag("false"),
        )),
    ))(input)
    .map(|(rest, val)| (rest, val.map(|v| v.to_string())))
}

fn parse_task_input(input: &str) -> IResult<&str, TaskInput> {
    map(
        tuple((ws(parse_data_type), ws(identifier), parse_default_value)),
        |(data_type, name, default)| TaskInput {
            name: name.to_string(),
            data_type,
            default,
        },
    )(input)
}

fn parse_task_inputs(input: &str) -> IResult<&str, Vec<TaskInput>> {
    preceded(
        ws(tag("input")),
        delimited(ws(tag("{")), many0(parse_task_input), ws(tag("}"))),
    )(input)
}

fn parse_command(input: &str) -> IResult<&str, String> {
    preceded(
        ws(tag("command")),
        alt((
            delimited(tag("<<<"), take_until(">>>"), tag(">>>")),
            delimited(tag("{"), take_until("}"), tag("}")),
        )),
    )(input)
    .map(|(rest, cmd)| (rest, cmd.trim().to_string()))
}

fn parse_task_output(input: &str) -> IResult<&str, TaskOutput> {
    map(
        tuple((
            ws(parse_data_type),
            ws(identifier),
            preceded(ws(tag("=")), is_not("\n}")),
        )),
        |(data_type, name, expression)| TaskOutput {
            name: name.to_string(),
            data_type,
            expression: expression.trim().to_string(),
        },
    )(input)
}

fn parse_task_outputs(input: &str) -> IResult<&str, Vec<TaskOutput>> {
    preceded(
        ws(tag("output")),
        delimited(ws(tag("{")), many0(parse_task_output), ws(tag("}"))),
    )(input)
}

fn parse_task(input: &str) -> IResult<&str, Task> {
    map(
        tuple((
            preceded(ws(tag("task")), ws(identifier)),
            delimited(
                ws(tag("{")),
                tuple((
                    opt(parse_task_inputs),
                    parse_command,
                    opt(parse_task_outputs),
                )),
                ws(tag("}")),
            ),
        )),
        |(name, (inputs, command, outputs))| Task {
            name: name.to_string(),
            inputs: inputs.unwrap_or_default(),
            command,
            outputs: outputs.unwrap_or_default(),
        },
    )(input)
}

fn parse_call_inputs(input: &str) -> IResult<&str, Vec<(String, String)>> {
    preceded(
        ws(tag("input:")),
        separated_list0(
            tag(","),
            map(
                tuple((
                    ws(identifier),
                    preceded(
                        ws(tag("=")),
                        ws(alt((
                            digit1,
                            recognize(separated_list1(char('.'), identifier)),
                            identifier,
                        ))),
                    ),
                )),
                |(key, val)| (key.to_string(), val.to_string()),
            ),
        ),
    )(input)
}

fn parse_task_call(input: &str) -> IResult<&str, TaskCall> {
    let (input, _) = ws(tag("call"))(input)?;
    let (input, task_name) = identifier(input)?;
    let (input, alias) = opt(preceded(ws(tag("as")), identifier))(input)?;

    let (input, inputs) = if let Ok((remaining, _)) = ws(tag("{"))(input) {
        let (remaining, inputs) = parse_call_inputs(remaining)?; // Remove opt() here
        let (remaining, _) = ws(tag("}"))(remaining)?;
        (remaining, inputs)
    } else {
        (input, vec![])
    };

    Ok((
        input,
        TaskCall {
            task_name: task_name.to_string(),
            alias: alias.map(|a| a.to_string()),
            inputs,
        },
    ))
}

fn parse_workflow_output(input: &str) -> IResult<&str, TaskOutput> {
    map(
        tuple((
            ws(parse_data_type),
            ws(identifier),
            preceded(ws(tag("=")), is_not("\n}")),
        )),
        |(data_type, name, expression)| TaskOutput {
            name: name.to_string(),
            data_type,
            expression: expression.trim().to_string(),
        },
    )(input)
}

fn parse_workflow_outputs(input: &str) -> IResult<&str, Vec<TaskOutput>> {
    preceded(
        ws(tag("output")),
        delimited(ws(tag("{")), many0(parse_workflow_output), ws(tag("}"))),
    )(input)
}

fn parse_workflow(input: &str) -> IResult<&str, Workflow> {
    map(
        tuple((
            preceded(ws(tag("workflow")), ws(identifier)),
            delimited(
                ws(tag("{")),
                tuple((
                    opt(parse_task_inputs),
                    many0(parse_task_call),
                    opt(parse_workflow_outputs),
                )),
                ws(tag("}")),
            ),
        )),
        |(name, (inputs, calls, outputs))| Workflow {
            name: name.to_string(),
            inputs: inputs.unwrap_or_default(),
            calls,
            outputs: outputs.unwrap_or_default(),
        },
    )(input)
}

fn parse_version(input: &str) -> IResult<&str, String> {
    preceded(
        ws(tag("version")),
        alt((
            delimited(char('"'), take_until("\""), char('"')),
            recognize(pair(digit1, opt(preceded(char('.'), digit1)))),
        )),
    )(input)
    .map(|(rest, ver)| (rest, ver.to_string()))
}

pub fn parse_wdl(input: &str) -> Result<WdlDocument> {
    let mut remaining = input;
    let mut version = None;
    let mut tasks = Vec::new();
    let mut workflows = Vec::new();

    // Try to parse version first
    if let Ok((rest, ver)) = parse_version(remaining) {
        version = Some(ver);
        remaining = rest;
    }

    // Parse tasks and workflows
    while !remaining.trim().is_empty() {
        remaining = remaining.trim();

        // Skip comments
        if remaining.starts_with('#') {
            if let Some(pos) = remaining.find('\n') {
                remaining = &remaining[pos + 1..];
                continue;
            } else {
                break;
            }
        }

        let initial_len = remaining.len();

        if let Ok((rest, task)) = parse_task(remaining) {
            tasks.push(task);
            remaining = rest;
        } else if let Ok((rest, workflow)) = parse_workflow(remaining) {
            workflows.push(workflow);
            remaining = rest;
        } else {
            // If we can't parse anything and there's non-whitespace content, it's an error
            let next_line = match remaining.lines().next() {
                Some(line) => line,
                None => remaining, // No newlines, use whole string
            };
            if !next_line.trim().is_empty() && !next_line.trim().starts_with('#') {
                return Err(SprocketError::ParseError(format!(
                    "Invalid WDL syntax at: '{}'",
                    next_line.chars().take(50).collect::<String>()
                )));
            }

            // Skip to next line if it's just whitespace or we made no progress
            if let Some(pos) = remaining.find('\n') {
                remaining = &remaining[pos + 1..];
            } else {
                break;
            }
        }

        // Prevent infinite loops
        if remaining.len() == initial_len {
            return Err(SprocketError::ParseError(
                "Parser stuck - unable to process remaining input".to_string(),
            ));
        }
    }

    // Validate we have at least one workflow or task
    if tasks.is_empty() && workflows.is_empty() {
        return Err(SprocketError::ParseError(
            "No valid tasks or workflows found in WDL document".to_string(),
        ));
    }

    Ok(WdlDocument {
        version,
        tasks,
        workflows,
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_data_type_string() {
        let input = "String";
        let result = parse_data_type(input);
        assert!(result.is_ok());
        let (remaining, dtype) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(dtype, DataType::String);
    }

    #[test]
    fn test_parse_data_type_int() {
        let input = "Int";
        let result = parse_data_type(input);
        assert!(result.is_ok());
        let (remaining, dtype) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(dtype, DataType::Int);
    }

    #[test]
    fn test_parse_data_type_array() {
        let input = "Array[String]";
        let result = parse_data_type(input);
        assert!(result.is_ok());
        let (remaining, dtype) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(dtype, DataType::Array(Box::new(DataType::String)));
    }

    #[test]
    fn test_parse_data_type_nested_array() {
        let input = "Array[Array[Int]]";
        let result = parse_data_type(input);
        assert!(result.is_ok());
        let (remaining, dtype) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(
            dtype,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Int))))
        );
    }

    #[test]
    fn test_parse_task_input_without_default() {
        let input = "String name";
        let result = parse_task_input(input);
        assert!(result.is_ok());
        let (remaining, task_input) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(task_input.name, "name");
        assert_eq!(task_input.data_type, DataType::String);
        assert_eq!(task_input.default, None);
    }

    #[test]
    fn test_parse_task_input_with_default() {
        let input = "Int count = 42";
        let result = parse_task_input(input);
        assert!(result.is_ok());
        let (remaining, task_input) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(task_input.name, "count");
        assert_eq!(task_input.data_type, DataType::Int);
        assert_eq!(task_input.default, Some("42".to_string()));
    }

    #[test]
    fn test_parse_task_input_with_quoted_default() {
        let input = r#"String greeting = "Hello World""#;
        let result = parse_task_input(input);
        assert!(result.is_ok());
        let (remaining, task_input) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(task_input.name, "greeting");
        assert_eq!(task_input.data_type, DataType::String);
        assert_eq!(task_input.default, Some("Hello World".to_string()));
    }

    #[test]
    fn test_parse_task_inputs_multiple() {
        let input = r#"input {
    String name
    Int count = 10
    File data_file
}"#;
        let result = parse_task_inputs(input);
        assert!(result.is_ok());
        let (remaining, inputs) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[0].name, "name");
        assert_eq!(inputs[0].default, None);
        assert_eq!(inputs[1].name, "count");
        assert_eq!(inputs[1].default, Some("10".to_string()));
        assert_eq!(inputs[2].name, "data_file");
        assert_eq!(inputs[2].data_type, DataType::File);
    }

    #[test]
    fn test_parse_task_outputs_section() {
        let input = r#"output {
    String result = stdout()
    File output_file = "results.txt"
}"#;
        let result = parse_task_outputs(input);
        assert!(result.is_ok());
        let (remaining, outputs) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].name, "result");
        assert_eq!(outputs[0].expression, "stdout()");
        assert_eq!(outputs[1].name, "output_file");
        assert_eq!(outputs[1].expression, "\"results.txt\"");
    }

    #[test]
    fn test_parse_command_simple() {
        let input = r#"command <<<
    echo "Hello World"
>>>"#;
        let result = parse_command(input);
        assert!(result.is_ok());
        let (remaining, command) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert!(command.contains("echo \"Hello World\""));
    }

    #[test]
    fn test_parse_command_with_variables() {
        let input = r#"command <<<
    echo "Processing ${input_file}"
    wc -l ${input_file} > ${output_file}
>>>"#;
        let result = parse_command(input);
        assert!(result.is_ok());
        let (remaining, command) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert!(command.contains("${input_file}"));
        assert!(command.contains("${output_file}"));
    }

    #[test]
    fn test_parse_task_complete() {
        let input = r#"task process_data {
  input {
    File input_file
    Int threshold = 30
  }
  
  command <<<
    grep -v "^#" ${input_file} | awk '$3 > ${threshold}' > filtered.txt
  >>>
  
  output {
    File filtered = "filtered.txt"
    Int line_count = read_int("count.txt")
  }
}"#;
        let result = parse_task(input);
        assert!(result.is_ok());
        let (remaining, task) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(task.name, "process_data");
        assert_eq!(task.inputs.len(), 2);
        assert_eq!(task.outputs.len(), 2);
        assert!(task.command.contains("grep"));
        assert!(task.command.contains("${threshold}"));
    }

    #[test]
    fn test_parse_task_call() {
        let input = r#"call process_data {input: file = input_file, threshold = 40 }"#;
        let result = parse_task_call(input);
        assert!(result.is_ok(), "Failed to parse: {:?}", result);
        /* let (remaining, call) =*/
        match result {
            Ok((remaining, call)) => {
                assert!(
                    remaining.trim().is_empty(),
                    "Remaining text: '{}'",
                    remaining
                );

                assert_eq!(call.task_name, "process_data");
                assert_eq!(
                    call.inputs.len(),
                    2,
                    "Expected 2 inputs, got {}: {:?}",
                    call.inputs.len(),
                    call.inputs
                );

                assert!(call
                    .inputs
                    .iter()
                    .any(|(k, v)| k == "file" && v == "input_file"));
                assert!(call
                    .inputs
                    .iter()
                    .any(|(k, v)| k == "threshold" && v == "40"));
            }
            Err(e) => {
                eprintln!("There was an error with the parser:\n{e}");
            }
        };
    }

    #[test]
    fn test_parse_workflow_complete() {
        let input = r#"workflow analysis_pipeline {
  input {
    File raw_data
    Int quality_threshold = 30
  }
  
  call quality_check {
    input: data = raw_data, threshold = quality_threshold
  }
  
  call process_data {
    input: input_file = quality_check.passed_data
  }
  
  output {
    File final_output = process_data.filtered
    Boolean qc_passed = quality_check.passed
  }
}"#;
        let result = parse_workflow(input);
        assert!(result.is_ok(), "Failed to parse workflow: {:?}", result);
        let (remaining, workflow) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(workflow.name, "analysis_pipeline");
        assert_eq!(workflow.inputs.len(), 2);
        assert_eq!(
            workflow.calls.len(),
            2,
            "Expected 2 calls but got {}: {:?}",
            workflow.calls.len(),
            workflow.calls
        );
        assert_eq!(workflow.outputs.len(), 2);
        assert_eq!(workflow.calls[0].task_name, "quality_check");
        assert_eq!(workflow.calls[1].task_name, "process_data");
    }

    #[test]
    fn test_parse_version_integer() {
        let input = "version 1";
        let result = parse_version(input);
        assert!(result.is_ok());
        let (remaining, version) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(version, "1");
    }

    #[test]
    fn test_parse_version_decimal() {
        let input = "version 1.0";
        let result = parse_version(input);
        assert!(result.is_ok());
        let (remaining, version) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(version, "1.0");
    }

    #[test]
    fn test_parse_version_quoted() {
        let input = r#"version "1.2.3""#;
        let result = parse_version(input);
        assert!(result.is_ok());
        let (remaining, version) = result.unwrap();
        assert_eq!(remaining.trim(), "");
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn test_parse_wdl_document_minimal() {
        let source = r#"
task minimal {
  command <<<
    echo "test"
  >>>
}"#;
        let result = parse_wdl(source);
        assert!(result.is_ok());
        let doc = result.unwrap();
        assert_eq!(doc.version, None);
        assert_eq!(doc.tasks.len(), 1);
        assert_eq!(doc.workflows.len(), 0);
    }

    #[test]
    fn test_parse_wdl_invalid_content_returns_error() {
        let source = r#"
version 1.0

this is not valid WDL syntax
random text here
"#;
        let result = parse_wdl(source);
        assert!(result.is_err());
        match result {
            Err(SprocketError::ParseError(msg)) => {
                assert!(msg.contains("Invalid WDL syntax"));
            }
            _ => panic!("Expected ParseError for invalid syntax"),
        }
    }

    #[test]
    fn test_parse_wdl_empty_document_returns_error() {
        let source = r#"
version 1.0
# Just comments and whitespace


"#;
        let result = parse_wdl(source);
        assert!(result.is_err());
        match result {
            Err(SprocketError::ParseError(msg)) => {
                assert!(msg.contains("No valid tasks or workflows found"));
            }
            _ => panic!("Expected ParseError for empty document"),
        }
    }

    #[test]
    fn test_parse_wdl_complete_genomic_workflow() {
        let source = r#"
version 1.0

workflow GenomicAnalysis {
  input {
    File sample_fastq
    String sample_id
    Int quality_threshold = 30
  }
  
  call QualityControl {
    input: 
      fastq_file = sample_fastq,
      threshold = quality_threshold
  }
  
  call CountReads {
    input: fastq_file = sample_fastq
  }
  
  output {
    File qc_report = QualityControl.report
    Int total_reads = CountReads.count
  }
}

task QualityControl {
  input {
    File fastq_file
    Int threshold
  }
  
  command <<<
    echo "Running QC on ${fastq_file}"
    echo "Quality threshold: ${threshold}"
    echo "QC PASSED" > qc_report.txt
  >>>
  
  output {
    File report = "qc_report.txt"
  }
}

task CountReads {
  input {
    File fastq_file
  }
  
  command <<<
    echo "Counting reads in ${fastq_file}"
    echo "1000000" > read_count.txt
  >>>
  
  output {
    Int count = read_int("read_count.txt")
  }
}"#;

        let result = parse_wdl(source);
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert_eq!(doc.version, Some("1.0".to_string()));
        assert_eq!(doc.workflows.len(), 1);
        assert_eq!(doc.tasks.len(), 2);

        let workflow = &doc.workflows[0];
        assert_eq!(workflow.name, "GenomicAnalysis");
        assert_eq!(workflow.inputs.len(), 3);
        assert_eq!(workflow.calls.len(), 2);
        assert_eq!(workflow.outputs.len(), 2);

        // Verify task names
        let task_names: Vec<&str> = doc.tasks.iter().map(|t| t.name.as_str()).collect();
        assert!(task_names.contains(&"QualityControl"));
        assert!(task_names.contains(&"CountReads"));
    }
}
