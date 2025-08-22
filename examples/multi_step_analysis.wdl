version 1.0

workflow MultiStepAnalysis {
  input {
    String experiment_id
    File raw_data
    String reference_genome = "GRCh38"
    Int num_threads = 4
    Boolean generate_plots = true
  }
  
  call DataPreprocessing {
    input:
      raw_data = raw_data,
      experiment_id = experiment_id
  }
  
  call QualityMetrics {
    input:
      preprocessed_data = DataPreprocessing.cleaned_data,
      experiment_id = experiment_id
  }
  
  call PrimaryAnalysis {
    input:
      input_data = DataPreprocessing.cleaned_data,
      reference = reference_genome,
      threads = num_threads
  }
  
  call SecondaryAnalysis {
    input:
      primary_results = PrimaryAnalysis.results,
      quality_metrics = QualityMetrics.metrics_file
  }
  
  call Visualization {
    input:
      analysis_results = SecondaryAnalysis.enhanced_results,
      generate = generate_plots,
      experiment_id = experiment_id
  }
  
  call CompileResults {
    input:
      preprocessed = DataPreprocessing.cleaned_data,
      primary = PrimaryAnalysis.results,
      secondary = SecondaryAnalysis.enhanced_results,
      plots = Visualization.plot_files,
      experiment_id = experiment_id
  }
  
  output {
    File final_results = CompileResults.compiled_report
    File quality_metrics = QualityMetrics.metrics_file
    File visualization = Visualization.plot_files
    Float quality_score = QualityMetrics.overall_score
    Boolean analysis_passed = SecondaryAnalysis.passed_threshold
  }
}

task DataPreprocessing {
  input {
    File raw_data
    String experiment_id
  }
  
  command <<<
    echo "Preprocessing data for experiment: ${experiment_id}"
    
    # Simulate data cleaning
    cat << EOF > cleaned_data.tsv
# Cleaned data for ${experiment_id}
# Preprocessing completed: $(date)
Sample_ID	Value1	Value2	Value3	Quality
S001	45.2	23.1	67.8	PASS
S002	44.8	22.9	68.1	PASS
S003	45.5	23.3	67.5	PASS
S004	43.9	22.5	68.9	WARNING
S005	45.1	23.0	67.7	PASS
EOF
    
    echo "Preprocessing complete: removed 2 outliers, normalized 5 samples"
  >>>
  
  output {
    File cleaned_data = "cleaned_data.tsv"
  }
}

task QualityMetrics {
  input {
    File preprocessed_data
    String experiment_id
  }
  
  command <<<
    echo "Calculating quality metrics for ${experiment_id}"
    
    cat << EOF > quality_metrics.json
{
  "experiment_id": "${experiment_id}",
  "metrics": {
    "data_completeness": 0.98,
    "signal_to_noise": 12.5,
    "replicate_correlation": 0.95,
    "outlier_percentage": 2.1,
    "overall_quality_score": 0.94
  },
  "thresholds_passed": {
    "completeness": true,
    "snr": true,
    "correlation": true,
    "outliers": true
  },
  "warnings": [],
  "timestamp": "$(date -Iseconds)"
}
EOF
    
    echo "0.94" > quality_score.txt
  >>>
  
  output {
    File metrics_file = "quality_metrics.json"
    Float overall_score = read_file("quality_score.txt")
  }
}

task PrimaryAnalysis {
  input {
    File input_data
    String reference
    Int threads
  }
  
  command <<<
    echo "Running primary analysis"
    echo "Reference: ${reference}"
    echo "Using ${threads} threads"
    
    # Simulate analysis
    cat << EOF > primary_results.txt
Primary Analysis Results
========================
Reference: ${reference}
Threads used: ${threads}
Processing time: 12.4 seconds

Key Findings:
- Pattern A detected in 78% of samples
- Pattern B detected in 45% of samples
- Significant correlation found (p < 0.01)
- Mean effect size: 1.23 (95% CI: 1.10-1.36)

Statistical Summary:
- Total features analyzed: 10000
- Significant features: 342
- FDR < 0.05: 298
- Fold change > 2: 156
EOF
  >>>
  
  output {
    File results = "primary_results.txt"
  }
}

task SecondaryAnalysis {
  input {
    File primary_results
    File quality_metrics
  }
  
  command <<<
    echo "Running secondary analysis"
    
    # Parse quality score (simulation)
    QUALITY_CHECK="true"
    
    cat << EOF > secondary_results.json
{
  "enhanced_analysis": {
    "pathway_enrichment": {
      "significant_pathways": 15,
      "top_pathway": "Cell cycle regulation",
      "enrichment_score": 3.45
    },
    "clustering_results": {
      "optimal_clusters": 4,
      "silhouette_score": 0.72
    },
    "network_analysis": {
      "hub_genes": 8,
      "modules_detected": 3,
      "connectivity_score": 0.81
    }
  },
  "validation": {
    "cross_validation_accuracy": 0.89,
    "bootstrap_confidence": 0.95
  },
  "threshold_passed": ${QUALITY_CHECK},
  "timestamp": "$(date -Iseconds)"
}
EOF
    
    echo "${QUALITY_CHECK}" > passed.txt
  >>>
  
  output {
    File enhanced_results = "secondary_results.json"
    Boolean passed_threshold = read_file("passed.txt")
  }
}

task Visualization {
  input {
    File analysis_results
    Boolean generate
    String experiment_id
  }
  
  command <<<
    if [ "${generate}" = "true" ]; then
      echo "Generating visualizations for ${experiment_id}"
      
      # Simulate plot generation
      cat << EOF > plots_manifest.json
{
  "experiment_id": "${experiment_id}",
  "plots_generated": [
    {
      "name": "heatmap.png",
      "type": "heatmap",
      "description": "Expression heatmap"
    },
    {
      "name": "volcano_plot.png",
      "type": "volcano",
      "description": "Differential expression volcano plot"
    },
    {
      "name": "pca_plot.png",
      "type": "pca",
      "description": "Principal component analysis"
    },
    {
      "name": "network_graph.png",
      "type": "network",
      "description": "Gene interaction network"
    }
  ],
  "total_plots": 4,
  "format": "PNG",
  "resolution": "300dpi"
}
EOF
    else
      echo "Visualization skipped"
      echo '{"plots_generated": [], "total_plots": 0}' > plots_manifest.json
    fi
  >>>
  
  output {
    File plot_files = "plots_manifest.json"
  }
}

task CompileResults {
  input {
    File preprocessed
    File primary
    File secondary
    File plots
    String experiment_id
  }
  
  command <<<
    echo "Compiling final report for ${experiment_id}"
    
    cat << EOF > final_report.md
# Analysis Report for Experiment ${experiment_id}

## Generated: $(date)

## Executive Summary
Complete multi-step analysis pipeline executed successfully.

## Data Processing
- Input data preprocessed and cleaned
- Quality metrics calculated and passed

## Primary Analysis
$(cat ${primary})

## Secondary Analysis
Advanced statistical analysis and pathway enrichment completed.
See detailed results in accompanying JSON files.

## Visualizations
Multiple plots generated for data exploration and presentation.

## Files Generated
1. Cleaned data: $(basename ${preprocessed})
2. Primary results: $(basename ${primary})
3. Secondary results: $(basename ${secondary})
4. Visualization manifest: $(basename ${plots})

## Conclusion
Analysis pipeline completed successfully with all quality checks passed.

---
*Report generated by Sprocket Workflow Engine*
EOF
  >>>
  
  output {
    File compiled_report = "final_report.md"
  }
}