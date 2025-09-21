version 1.0

task QualityCheck {
  input {
    File data_file
    Int threshold = 25
  }
  
  command <<<
    echo "Running quality check on ${data_file}"
    echo "Threshold: ${threshold}"
    echo "PASSED" > qc_result.txt
  >>>
  
  output {
    File result = "qc_result.txt"
    Boolean passed = true
  }
}

task DataValidation {
  input {
    File input_file
  }
  
  command <<<
    echo "Validating ${input_file}"
    wc -l ${input_file} > line_count.txt
  >>>
  
  output {
    File count = "line_count.txt"
  }
}
