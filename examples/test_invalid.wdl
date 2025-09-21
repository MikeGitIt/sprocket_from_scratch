version 1.0

workflow TestWorkflow {
  input {
    String name
  }
  
  call InvalidSyntax {
    this is invalid syntax
  }
  
  output {
    String result = InvalidSyntax.output
  }
}