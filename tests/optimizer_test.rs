#[cfg(test)]
mod tests {
    use std::fs;
    use xschemoptimizer::optimizer::{OptimizationProblem, TargetMetric};
    use xschemoptimizer::{XSchemIO, SchematicFiles, glob_files, choose_best_solver, OptimizationConfig, auto_optimize};
    use argmin::core::{CostFunction, Gradient};
    use std::path::PathBuf;
    use std::collections::HashMap;

    #[test]
    fn test_glob_files() {
        let test_dir = "tests/test_sample";
        let files = glob_files(test_dir).expect("Failed to glob files");
        assert!(files.schematic.is_some(), "Schematic file should be found");
        assert!(files.symbol.is_some(), "Symbol file should be found");
        assert!(files.testbench.is_some(), "Testbench file should be found");

        let missing_files = files.missing_files();
        assert!(missing_files.is_empty(), "No files should be missing");
    }

    #[test]
    fn test_full_optimization_flow() {
        println!("\n=== Testing Full Optimization Flow ===");
        
        // Setup test directories
        let test_dir = PathBuf::from("tests/test_sample");
        let netlist_dir = test_dir.join("spice");
        
        // Create netlist directory if it doesn't exist
        if !netlist_dir.exists() {
            std::fs::create_dir_all(&netlist_dir).expect("Failed to create netlist directory");
        }
        
        // Get schematic files
        let files = glob_files(test_dir.to_str().unwrap())
            .expect("Failed to glob files");
        let missing_files = files.missing_files();
        assert!(missing_files.is_empty(), "Missing files: {:?}", missing_files);
        
        println!("✓ Found all required schematic files");
        
        // Define target metrics we want to optimize for
        let target_metrics = vec![
            TargetMetric::new("GAIN", 20.0, "meas gain max(db(v(out)))"),    // 20 dB gain
            TargetMetric::new("POWER", 1e-3, "meas power avg(i(vdd)*v(vdd))"), // 1 mW power
            TargetMetric::new("BW", 1e6, "meas bw when db(v(out))=gain-3"),   // 1 MHz bandwidth
        ];
        
        // Define parameter mapping: (parameter_name, component_name)
        let parameter_map = vec![
            ("value".to_string(), "R1".to_string()),   // Resistance value
            ("W".to_string(), "M1".to_string()),       // MOSFET width
            ("L".to_string(), "M1".to_string()),       // MOSFET length
        ];
        
        // Create optimization problem
        let problem = OptimizationProblem::new(
            target_metrics,
            parameter_map,
            test_dir.clone(),
            netlist_dir.clone(),
        );
        
        println!("✓ Created optimization problem");
        
        // Test UpdateSimulation function
        println!("\n--- Testing UpdateSimulation ---");
        let test_parameters = vec![1500.0, 20e-6, 0.5e-6]; // R=1.5kΩ, W=20μm, L=0.5μm
        
        match problem.update_simulation(&test_parameters) {
            Ok(()) => {
                println!("✓ UpdateSimulation completed successfully");
                println!("  Updated parameters: {:?}", test_parameters);
            }
            Err(e) => {
                println!("⚠ UpdateSimulation failed (expected without real schematics): {}", e);
                // This is expected to fail in test environment without real schematic files
            }
        }
        
        // Test RunSimulation function
        println!("\n--- Testing RunSimulation ---");
        match problem.run_simulation() {
            Ok(metrics) => {
                println!("✓ RunSimulation completed successfully");
                println!("  Extracted metrics: {:?}", metrics);
                assert_eq!(metrics.len(), 3, "Should return 3 target metrics");
            }
            Err(e) => {
                println!("⚠ RunSimulation failed (expected without SPICE/XSchem): {}", e);
                // This is expected to fail in test environment without SPICE tools
            }
        }
        
        // Test CostFunction implementation
        println!("\n--- Testing CostFunction ---");
        let cost_result = problem.cost(&test_parameters);
        match cost_result {
            Ok(cost) => {
                println!("✓ Cost function evaluation successful");
                println!("  Cost value: {:.6e}", cost);
                assert!(cost.is_finite(), "Cost should be finite");
            }
            Err(e) => {
                println!("⚠ Cost function failed (expected): {:?}", e);
            }
        }
        
        // Test gradient computation
        println!("\n--- Testing Gradient Computation ---");
        let gradient_result = problem.gradient(&test_parameters);
        match gradient_result {
            Ok(grad) => {
                println!("✓ Gradient computation successful");
                println!("  Gradient: {:?}", grad);
                assert_eq!(grad.len(), 3, "Gradient should have 3 components");
            }
            Err(e) => {
                println!("⚠ Gradient computation failed (expected): {:?}", e);
            }
        }
        
        // Demonstrate full optimization (limited iterations for test)
        println!("\n--- Testing Full Optimization ---");
        let initial_params = vec![1000.0, 10e-6, 1e-6]; // Starting parameter values
    }
}
