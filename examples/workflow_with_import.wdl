version 1.0

import "common_tasks.wdl" as common

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
    File qc_result = common.QualityCheck.result
    File validation = common.DataValidation.count
  }
}