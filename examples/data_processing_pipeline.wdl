version 1.0

workflow DataProcessingPipeline {
  input {
    File input_data
    String output_prefix
    Int batch_size = 100
    Boolean enable_validation = true
    Float error_threshold = 0.05
  }
  
  call ValidateInput {
    input:
      data_file = input_data,
      enable = enable_validation
  }
  
  call SplitData {
    input:
      data_file = ValidateInput.validated_file,
      batch_size = batch_size,
      prefix = output_prefix
  }
  
  call ProcessBatch as ProcessBatch1 {
    input:
      batch_file = SplitData.batch1,
      batch_id = "1",
      error_threshold = error_threshold
  }
  
  call ProcessBatch as ProcessBatch2 {
    input:
      batch_file = SplitData.batch2,
      batch_id = "2",
      error_threshold = error_threshold
  }
  
  call MergeResults {
    input:
      result1 = ProcessBatch1.processed,
      result2 = ProcessBatch2.processed,
      output_prefix = output_prefix
  }
  
  call GenerateStatistics {
    input:
      merged_data = MergeResults.merged_file,
      validation_report = ValidateInput.report
  }
  
  output {
    File processed_data = MergeResults.merged_file
    File statistics = GenerateStatistics.stats_file
    File validation_report = ValidateInput.report
    Int total_records = GenerateStatistics.record_count
    Float error_rate = GenerateStatistics.error_rate
  }
}

task ValidateInput {
  input {
    File data_file
    Boolean enable
  }
  
  command <<<
    echo "Validating input file: $(basename ${data_file})"
    
    if [ "${enable}" = "true" ]; then
      echo "Validation enabled"
      
      # Simulate validation
      cat << EOF > validation_report.txt
Validation Report
=================
File: $(basename ${data_file})
Status: PASSED
Records checked: 1000
Invalid records: 0
Warnings: 2
- Line 45: Missing optional field
- Line 892: Unusual value in column 3
EOF
      
      cp ${data_file} validated.dat
    else
      echo "Validation skipped"
      echo "Validation skipped by user" > validation_report.txt
      cp ${data_file} validated.dat
    fi
  >>>
  
  output {
    File validated_file = "validated.dat"
    File report = "validation_report.txt"
  }
}

task SplitData {
  input {
    File data_file
    Int batch_size
    String prefix
  }
  
  command <<<
    echo "Splitting data into batches of ${batch_size}"
    
    # Simulate splitting data
    echo "Batch 1 data (${batch_size} records)" > ${prefix}_batch1.dat
    echo "Sample data line 1" >> ${prefix}_batch1.dat
    echo "Sample data line 2" >> ${prefix}_batch1.dat
    
    echo "Batch 2 data (${batch_size} records)" > ${prefix}_batch2.dat
    echo "Sample data line 3" >> ${prefix}_batch2.dat
    echo "Sample data line 4" >> ${prefix}_batch2.dat
  >>>
  
  output {
    File batch1 = "${prefix}_batch1.dat"
    File batch2 = "${prefix}_batch2.dat"
  }
}

task ProcessBatch {
  input {
    File batch_file
    String batch_id
    Float error_threshold
  }
  
  command <<<
    echo "Processing batch ${batch_id}"
    echo "Error threshold: ${error_threshold}"
    
    # Simulate processing
    cat << EOF > processed_batch_${batch_id}.dat
# Processed Batch ${batch_id}
# Error threshold: ${error_threshold}
# Processing timestamp: $(date)
PROCESSED_DATA_START
ID,Value,Status
1,100.5,OK
2,98.3,OK
3,102.1,WARNING
4,99.8,OK
PROCESSED_DATA_END
EOF
    
    echo "Batch ${batch_id} processing complete"
  >>>
  
  output {
    File processed = "processed_batch_${batch_id}.dat"
  }
}

task MergeResults {
  input {
    File result1
    File result2
    String output_prefix
  }
  
  command <<<
    echo "Merging results"
    
    cat << EOF > ${output_prefix}_merged.dat
# Merged Results
# Generated: $(date)
# Source files: $(basename ${result1}), $(basename ${result2})

$(cat ${result1})

$(cat ${result2})

# End of merged data
EOF
    
    echo "Merge complete"
  >>>
  
  output {
    File merged_file = "${output_prefix}_merged.dat"
  }
}

task GenerateStatistics {
  input {
    File merged_data
    File validation_report
  }
  
  command <<<
    echo "Generating statistics"
    
    # Simulate statistics generation
    cat << EOF > statistics.json
{
  "summary": {
    "total_records": 200,
    "processed_records": 198,
    "failed_records": 2,
    "error_rate": 0.01,
    "processing_time_seconds": 45.3
  },
  "quality_metrics": {
    "completeness": 0.99,
    "accuracy": 0.98,
    "consistency": 0.97
  },
  "validation_status": "PASSED",
  "timestamp": "$(date -Iseconds)"
}
EOF
    
    echo "200" > record_count.txt
    echo "0.01" > error_rate.txt
  >>>
  
  output {
    File stats_file = "statistics.json"
    Int record_count = read_file("record_count.txt")
    Float error_rate = read_file("error_rate.txt")
  }
}