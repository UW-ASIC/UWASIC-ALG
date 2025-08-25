use xschemoptimizer::{OptimizationProblem, SpiceRunConfig, ComponentParameter, NgSpiceInterface, SimulationResult};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    /// Create test SPICE runs configuration
    fn create_test_spice_runs() -> Vec<SpiceRunConfig> {
        vec![
            SpiceRunConfig {
                expected_metrics: vec!["DC_GAIN".to_string(), "GBW".to_string()],
                weight: 1.0,
            }
        ]
    }

    /// Create test component parameters based on eg.sch transistors
    fn create_test_component_parameters() -> Vec<ComponentParameter> {
        vec![
            // M1 - Input differential pair transistor 1
            ComponentParameter {
                component_name: "XM1".to_string(),
                parameter_name: "W".to_string(),
                min_value: 1.0,
                max_value: 50.0,
                current_value: 16.0, // From eg.sch
            },
            ComponentParameter {
                component_name: "XM1".to_string(),
                parameter_name: "L".to_string(),
                min_value: 0.15,
                max_value: 2.0,
                current_value: 0.15, // From eg.sch
            },
            // M2 - Input differential pair transistor 2
            ComponentParameter {
                component_name: "XM2".to_string(),
                parameter_name: "W".to_string(),
                min_value: 1.0,
                max_value: 50.0,
                current_value: 16.0, // From eg.sch
            },
            ComponentParameter {
                component_name: "XM2".to_string(),
                parameter_name: "L".to_string(),
                min_value: 0.15,
                max_value: 2.0,
                current_value: 0.15, // From eg.sch
            },
            // M3 - Current mirror PMOS 1
            ComponentParameter {
                component_name: "XM3".to_string(),
                parameter_name: "W".to_string(),
                min_value: 0.5,
                max_value: 20.0,
                current_value: 1.0, // From eg.sch
            },
            // M6 - Output stage PMOS
            ComponentParameter {
                component_name: "XM6".to_string(),
                parameter_name: "W".to_string(),
                min_value: 0.5,
                max_value: 30.0,
                current_value: 1.0, // From eg.sch
            },
            // M7 - Output stage NMOS
            ComponentParameter {
                component_name: "XM7".to_string(),
                parameter_name: "W".to_string(),
                min_value: 0.5,
                max_value: 20.0,
                current_value: 1.0, // From eg.sch
            }
        ]
    }

    /// Create mock simulation results for testing
    fn create_mock_simulation_results(iteration: usize) -> Vec<SimulationResult> {
        let mut metrics = HashMap::new();
        
        // Simulate realistic op-amp metrics that change with iterations
        let base_dc_gain = 45.0 + (iteration as f64 * 2.0); // Improving DC gain
        let base_gbw = 1e6 + (iteration as f64 * 5e5); // Improving GBW
        
        metrics.insert("DC_GAIN".to_string(), base_dc_gain);
        metrics.insert("GBW".to_string(), base_gbw);

        vec![SimulationResult {
            metrics,
            stdout: format!("DC_GAIN: {:.2}\nGBW: {:.2e}", base_dc_gain, base_gbw),
            stderr: String::new(),
            simulator_used: "ngspice".to_string(),
            execution_time: 0.5,
            success: true,
            error: None,
        }]
    }

    #[test]
    fn test_optimization_problem_creation() {
        let spice_runs = create_test_spice_runs();
        let component_parameters = create_test_component_parameters();
        
        let opt_problem = OptimizationProblem::new(spice_runs, component_parameters.clone());
        
        // Verify initial state
        assert_eq!(opt_problem.get_iteration(), 0);
        assert_eq!(opt_problem.get_current_parameters().len(), component_parameters.len());
        assert_eq!(opt_problem.get_result_history().len(), 0);
        
        // Verify parameter bounds
        let (lower_bounds, upper_bounds) = opt_problem.get_parameter_bounds();
        assert_eq!(lower_bounds.len(), component_parameters.len());
        assert_eq!(upper_bounds.len(), component_parameters.len());
        
        for i in 0..component_parameters.len() {
            assert_eq!(lower_bounds[i], component_parameters[i].min_value);
            assert_eq!(upper_bounds[i], component_parameters[i].max_value);
        }
    }

    #[test]
    fn test_optimization_iterations() {
        let spice_runs = create_test_spice_runs();
        let component_parameters = create_test_component_parameters();
        
        let mut opt_problem = OptimizationProblem::new(spice_runs, component_parameters.clone());
        
        // Set target values for optimization
        let mut targets = HashMap::new();
        targets.insert("DC_GAIN_0".to_string(), 60.0); // Target 60dB DC gain
        targets.insert("GBW_0".to_string(), 10e6); // Target 10MHz GBW
        opt_problem.set_target_values(targets);
        
        println!("=== Starting Optimization Iterations ===");
        println!("Target Values: DC_GAIN = 60.0 dB, GBW = 10.0 MHz");
        
        // Print initial parameters
        println!("\n--- Initial Parameters ---");
        let initial_params = opt_problem.get_current_parameters();
        for (i, param) in component_parameters.iter().enumerate() {
            if i < initial_params.len() {
                println!("  {:<8} {:<2}: {:<8.3} (range: {:.3} - {:.3})", 
                    param.component_name, 
                    param.parameter_name, 
                    initial_params[i],
                    param.min_value,
                    param.max_value
                );
            }
        }
        
        // Run multiple iterations
        for iteration in 1..=5 {
            println!("\n--- Iteration {} ---", iteration);
            
            // Get current parameters before iteration
            let current_params = opt_problem.get_current_parameters();
            
            // Simulate SPICE run with current parameters
            let sim_results = create_mock_simulation_results(iteration);
            println!("Simulation results:");
            for result in &sim_results {
                for (metric, value) in &result.metrics {
                    if metric == "DC_GAIN" {
                        println!("  {}: {:.3} dB (target: 60.0)", metric, value);
                    } else if metric == "GBW" {
                        println!("  {}: {:.3e} Hz (target: 1.0e7)", metric, value);
                    } else {
                        println!("  {}: {:.3}", metric, value);
                    }
                }
            }
            
            // Run optimization iteration
            let new_params = opt_problem.iterate(sim_results);
            
            println!("Parameter changes:");
            for (i, param) in component_parameters.iter().enumerate() {
                if i < new_params.len() && i < current_params.len() {
                    let change = new_params[i] - current_params[i];
                    let change_sign = if change > 0.0 { "+" } else { "" };
                    println!("  {:<8} {:<2}: {:<8.3} -> {:<8.3} ({}{:.6})", 
                        param.component_name, 
                        param.parameter_name, 
                        current_params[i],
                        new_params[i],
                        change_sign,
                        change
                    );
                }
            }
            
            // Verify iteration count increased
            assert_eq!(opt_problem.get_iteration(), iteration);
            
            // Verify parameters are within bounds
            for (i, &param_value) in new_params.iter().enumerate() {
                if i < component_parameters.len() {
                    assert!(param_value >= component_parameters[i].min_value, 
                           "Parameter {} below minimum bound", i);
                    assert!(param_value <= component_parameters[i].max_value, 
                           "Parameter {} above maximum bound", i);
                }
            }
        }
        
        // Verify we have accumulated results
        assert_eq!(opt_problem.get_result_history().len(), 5);
        
        println!("\n=== Optimization Complete ===");
        println!("Total iterations: {}", opt_problem.get_iteration());
        println!("Total results collected: {}", opt_problem.get_result_history().len());
        
        println!("\n--- Final Parameters ---");
        let final_params = opt_problem.get_current_parameters();
        for (i, param) in component_parameters.iter().enumerate() {
            if i < final_params.len() {
                println!("  {:<8} {:<2}: {:<8.3}", 
                    param.component_name, 
                    param.parameter_name, 
                    final_params[i]
                );
            }
        }
    }

    #[test]
    fn test_parameter_bounds_enforcement() {
        let spice_runs = create_test_spice_runs();
        let mut component_parameters = create_test_component_parameters();
        
        // Set a parameter very close to its bounds
        component_parameters[0].current_value = component_parameters[0].max_value - 0.001;
        
        let mut opt_problem = OptimizationProblem::new(spice_runs, component_parameters.clone());
        
        // Create simulation results that would push parameter beyond bounds
        let mut metrics = HashMap::new();
        metrics.insert("DC_GAIN".to_string(), 20.0); // Low gain to trigger large adjustment
        metrics.insert("GBW".to_string(), 1e5); // Low GBW
        
        let sim_results = vec![SimulationResult {
            metrics,
            stdout: String::new(),
            stderr: String::new(),
            simulator_used: "ngspice".to_string(),
            execution_time: 0.5,
            success: true,
            error: None,
        }];
        
        // Set high target values to force large adjustments
        let mut targets = HashMap::new();
        targets.insert("DC_GAIN_0".to_string(), 80.0);
        targets.insert("GBW_0".to_string(), 50e6);
        opt_problem.set_target_values(targets);
        
        let new_params = opt_problem.iterate(sim_results);
        
        // Verify all parameters stay within bounds
        for (i, &param_value) in new_params.iter().enumerate() {
            if i < component_parameters.len() {
                assert!(param_value >= component_parameters[i].min_value, 
                       "Parameter {} = {:.3} below minimum {:.3}", 
                       i, param_value, component_parameters[i].min_value);
                assert!(param_value <= component_parameters[i].max_value, 
                       "Parameter {} = {:.3} above maximum {:.3}", 
                       i, param_value, component_parameters[i].max_value);
            }
        }
    }

    #[test]
    fn test_real_spice_simulation() {
        // This test requires ngspice to be installed
        let ngspice = NgSpiceInterface::new();
        
        // Test with the sample SPICE file
        let spice_file_path = "tests/test_sample/spice/eg_tb.spice";
        let spice_path = PathBuf::from(spice_file_path);
        
        if spice_path.exists() {
            println!("Testing with real SPICE file: {:?}", spice_path);
            
            // Test using the file directly
            match ngspice.run_simulation_from_file(&spice_path) {
                Ok(result) => {
                    println!("Simulation success: {}", result.success);
                    println!("Execution time: {:.3}s", result.execution_time);
                    println!("Metrics found: {}", result.metrics.len());
                    
                    if result.success {
                        for (metric, value) in &result.metrics {
                            println!("  {}: {:.3}", metric, value);
                        }
                    } else {
                        println!("Simulation failed: {:?}", result.error);
                        println!("STDOUT:\n{}", result.stdout);
                        println!("STDERR:\n{}", result.stderr);
                    }
                    
                    // If simulation succeeds, test integration with optimization
                    if result.success && !result.metrics.is_empty() {
                        let spice_runs = create_test_spice_runs();
                        let component_parameters = create_test_component_parameters();
                        let mut opt_problem = OptimizationProblem::new(spice_runs, component_parameters);
                        
                        // Use real simulation results
                        let new_params = opt_problem.iterate(vec![result]);
                        
                        println!("Optimization updated parameters:");
                        for (i, &param) in new_params.iter().enumerate() {
                            println!("  Parameter {}: {:.3}", i, param);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to run simulation: {}", e);
                }
            }
            
            // Also test reading content and running simulation
            if let Ok(spice_content) = std::fs::read_to_string(&spice_path) {
                match ngspice.run_simulation(&spice_content, "ngspice") {
                    Ok(result) => {
                        println!("\nContent-based simulation success: {}", result.success);
                        println!("Metrics found: {}", result.metrics.len());
                    }
                    Err(e) => {
                        println!("Content-based simulation failed: {}", e);
                    }
                }
            }
        } else {
            println!("SPICE file not found at {:?}, skipping real simulation test", spice_path);
        }
    }
}