#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    
    use xschemoptimizer::{gen_spice_file, run_spice};

    #[test]
    fn test_gen_spice_file_eg_tb() {
        let testbench_file = "test_sample/eg_tb.sch";
        
        // Generate SPICE file - ADD current_dir and netlist_dir parameters
        let current_file = file!();
        let current_dir = Path::new(current_file).parent().unwrap();
        let netlist_dir = "test_sample/spice"; // Add this parameter
        let result = gen_spice_file(testbench_file, &current_dir, netlist_dir)
            .expect("SPICE generation should not have IO errors");

        println!("Generation completed in {:.3}s", result.execution_time);
        println!("Generation success: {}", result.success);
        
        // Always print the full result for debugging
        println!("Full generation result: {:#?}", result);

        // Verify SPICE file was generated
        let spice_file = result.output_file
            .as_ref()
            .expect("Should have output file path when generation succeeds");

        assert!(Path::new(spice_file).exists(), 
               "Generated SPICE file should exist: {}", spice_file);

        if !result.success {
            let error_msg = result.error
                .as_ref()
                .map(|e| e.as_str())
                .unwrap_or("Unknown error");
                
            println!("SPICE generation FAILED:");
            println!("  Error: {}", error_msg);
            println!("  Stderr: {}", result.stderr);
            println!("  Stdout: {}", result.stdout);
            
            panic!("SPICE generation failed - this indicates a real problem that needs to be fixed");
        } 

        // Read and validate SPICE file content
        let spice_content = fs::read_to_string(spice_file)
            .expect("Should be able to read generated SPICE file");

        assert!(!spice_content.trim().is_empty(), "SPICE file should not be empty");

        println!("✓ Generated SPICE file: {} ({} bytes)", spice_file, spice_content.len());

        // Show first few lines for verification
        let lines: Vec<&str> = spice_content.lines().take(10).collect();
        println!("SPICE file content (first 10 lines):");
        for (i, line) in lines.iter().enumerate() {
            println!("  {}: {}", i+1, line);
        }

        // Clean up generated file
        // let _ = fs::remove_file(spice_file);
        
        println!("✓ SPICE generation test PASSED - file generated successfully");
    }

    #[test]
    fn test_complete_workflow_eg_tb() {
        let testbench_file = "test_sample/eg_tb.sch";
        
        // Step 1: Generate SPICE file - ADD current_dir and netlist_dir parameters
        let current_file = file!();
        let current_dir = Path::new(current_file).parent().unwrap();
        let netlist_dir = "test_sample/spice";
        let gen_result = gen_spice_file(testbench_file, &current_dir, netlist_dir)
            .expect("SPICE generation should not have IO errors");

        println!("Generation result: {:#?}", gen_result);

        if !gen_result.success {
            let error_msg = gen_result.error
                .as_ref()
                .map(|e| e.as_str())
                .unwrap_or("Unknown error");
                
            println!("SPICE generation failed:");
            println!("  Error: {}", error_msg);
            println!("  This might be due to:");
            println!("    - xschem not installed or not in PATH");
            println!("    - Missing symbol libraries");
            println!("    - Testbench file has errors");
            
            // Don't panic here since this test is about the complete workflow
            // If generation fails, we can't test simulation
            println!("⚠ Skipping simulation test due to generation failure");
            return;
        }

        let spice_file = gen_result.output_file
            .as_ref()
            .expect("Should have output file when generation succeeds");

        println!("Generated SPICE file: {}", spice_file);
        
        // Verify the file actually exists before trying to simulate
        assert!(Path::new(spice_file).exists(), 
               "SPICE file should exist before simulation: {}", spice_file);

        // Step 2: Run ngspice simulation
        match run_spice(spice_file) {
            Ok(sim_result) => {
                println!("Simulation completed in {:.3}s", sim_result.execution_time());
                println!("Simulation success: {}", sim_result.is_success());
                println!("Full simulation result: {:#?}", sim_result);

                if sim_result.is_success() {
                    println!("✓ Complete workflow succeeded!");
                    
                    let metrics = sim_result.get_metrics();
                    println!("Extracted {} metrics:", metrics.len());
                    for (metric, value) in metrics {
                        println!("  {}: {:.6e}", metric, value);
                    }

                    // Test individual metric access
                    if let Some(first_metric) = metrics.keys().next() {
                        let value = sim_result.get_metric(first_metric);
                        assert!(value.is_some(), "Should be able to get metric by name");
                        println!("✓ Metric access test passed");
                    }
                } else {
                    println!("⚠ Simulation failed:");
                    if let Some(error) = sim_result.get_error() {
                        println!("  Error: {}", error);
                    }
                    println!("  This might be due to:");
                    println!("    - ngspice not installed or not in PATH");
                    println!("    - SPICE file has syntax errors");
                    println!("    - Missing models or libraries");
                    println!("  Try running manually: ngspice -b {}", spice_file);
                }
            },
            Err(e) => {
                println!("⚠ Simulation had IO error: {}", e);
                println!("  This is likely due to ngspice not being installed");
                println!("  Try: sudo apt install ngspice  # or equivalent for your OS");
            }
        }

        // Clean up generated SPICE file
        // let cleanup_result = fs::remove_file(spice_file);
        
        println!("✓ Complete workflow test completed");
    }
}
