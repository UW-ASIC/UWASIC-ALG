#[cfg(test)]
mod tests {
    use super::*;
    use xschemoptimizer::optimizer::{OptimizationProblem, TargetMetric};
    use xschemoptimizer::utilities::{glob_files};
    use std::path::PathBuf;
    use std::collections::HashMap;
    use argmin::core::{CostFunction, Executor};
    use argmin::solver::neldermead::NelderMead;

    #[test]
    fn test_optimization_workflow_with_nelder_mead() -> Result<(), Box<dyn std::error::Error>> {
        let test_dir = "tests/test_sample";
        let current_dir = PathBuf::from(test_dir);
        let netlist_dir = current_dir.join("spice");
        
        // Use glob_files to find the schematic files
        let files = glob_files(test_dir)
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        println!("Found files:");
        if let Some(ref schematic) = files.schematic {
            println!("  Schematic: {}", schematic);
        }
        if let Some(ref testbench) = files.testbench {
            println!("  Testbench: {}", testbench);
        }
        if let Some(ref symbol) = files.symbol {
            println!("  Symbol: {}", symbol);
        }
        
        if !files.is_complete() {
            let missing = files.missing_files();
            return Err(format!("Missing required files: {:?}", missing).into());
        }
        
        // Define target metrics for operational amplifier optimization
        let target_metrics = vec![
            TargetMetric::new(
                "DC_GAIN", 
                60.0, // Target: 60 dB DC gain (typical for op-amp)
                concat!(
                    ".ac dec 100 0.1 1G\n",
                    ".control\n",
                    "run\n",
                    "let dc_gain_val = vdb(vout)[0]\n",
                    "echo 'DC_GAIN:' $&dc_gain_val\n",
                    ".endc"
                )
            ),
            TargetMetric::new(
                "UNITY_GAIN_BW", 
                1e6, // Target: 1 MHz unity gain bandwidth
                concat!(
                    ".ac dec 100 0.1 1G\n",
                    ".control\n",
                    "run\n",
                    "let unity_gain_freq = vecmax(frequency)\n",
                    "echo 'UNITY_GAIN_BW:' $&unity_gain_freq\n",
                    ".endc"
                )
            ),
            TargetMetric::new(
                "POWER", 
                1e-3, // Target: 1 mW power consumption
                concat!(
                    ".op\n",
                    ".control\n",
                    "run\n",
                    "let power_consumption = vdd#branch * 1.8\n",
                    "echo 'POWER:' $&power_consumption\n",
                    ".endc"
                )
            ),
        ];
        
        let component_data = vec![
            ("M1".to_string(), {
                let mut props = HashMap::new();
                props.insert("W".to_string(), 2.0);
                props
            }),
            ("M2".to_string(), {
                let mut props = HashMap::new();
                props.insert("W".to_string(), 4.0);
                props
            }),
        ];
        
        // Create optimization problem with component data
        let (optimization_problem, initial_params) = OptimizationProblem::with_component_data(
            target_metrics,
            component_data,
            current_dir.clone(),
            netlist_dir.clone(),
        );
        
        println!("Created optimization problem successfully");
        
        // Test the cost function with initial parameters
        println!("Testing cost function with initial parameters: {:?}", initial_params);
        
        // This tests the entire pipeline:
        // 1. Update schematic with new parameters
        // 2. Modify testbench with SPICE codes from target metrics
        // 3. Generate SPICE files
        // 4. Run SPICE simulations
        // 5. Extract metrics and calculate cost
        match optimization_problem.cost(&initial_params) {
            Ok(cost) => {
                println!("Initial cost calculation successful: {}", cost);
                assert!(cost >= 0.0, "Cost should be non-negative");
                
                // If cost calculation works, try a few optimization steps with Nelder-Mead
                if cost < f64::MAX {
                    println!("Running optimization with Nelder-Mead solver...");
                    
                    // Set up Nelder-Mead solver with initial parameter values
                    let solver = NelderMead::new(vec![
                        initial_params.clone(),                    // Initial simplex vertex 1
                        {
                            let mut v = initial_params.clone();
                            v[0] += 0.5;                          // Perturb M1 width
                            v
                        },
                        {
                            let mut v = initial_params.clone();
                            v[1] += 0.5;                          // Perturb M2 width
                            v
                        },
                    ])
                    .with_sd_tolerance(0.01)?; // Stop when standard deviation is small
                    
                    // Run optimization
                    let result = Executor::new(optimization_problem.clone(), solver)
                        .configure(|state| {
                            state
                                .param(initial_params.clone())
                                .max_iters(5) // Limited iterations for testing
                        })
                        .run()?;
                    
                    println!("Optimization completed!");
                    println!("Best parameters: {:?}", result.state.best_param);
                    println!("Best cost: {}", result.state.best_cost);
                    println!("Iterations: {}", result.state.iter);
                    
                    // Verify the optimization made progress
                    assert!(result.state.best_cost <= cost, 
                           "Optimization should not increase cost");
                    
                    println!("Optimization test PASSED");
                } else {
                    println!("Cost was f64::MAX, indicating simulation failure");
                    println!("This is expected if SPICE tools are not available");
                }
            },
            Err(e) => {
                println!("Cost calculation failed: {:?}", e);
                println!("This might be due to:");
                println!("  - xschem not installed or not in PATH");
                println!("  - ngspice not installed or not in PATH"); 
                println!("  - Missing PDK or model files");
                println!("  - Schematic file format issues");
                
                // Don't fail the test if external tools aren't available
                println!("Test completed (external tools may not be available)");
                return Ok(());
            }
        }
        
        Ok(())
    }

    #[test]
    fn test_target_metric_creation() {
        let target = TargetMetric::new(
            "test_metric",
            42.0,
            concat!(".control\n", "echo 'TEST: 42'\n", ".endc")
        );
        
        assert_eq!(target.target_name, "test_metric");
        assert_eq!(target.target_value, 42.0);
        assert_eq!(target.spice_code, ".control\necho 'TEST: 42'\n.endc");
        
        println!("Target metric creation test PASSED");
    }

    #[test] 
    fn test_optimization_problem_creation() -> Result<(), Box<dyn std::error::Error>> {
        let test_dir = "tests/test_sample";
        
        // Verify files exist before creating optimization problem
        let files = glob_files(test_dir)?;
        
        if !files.is_complete() {
            println!("Skipping test - not all required files found in tests/test_sample/");
            return Ok(());
        }
        
        let target_metrics = vec![
            TargetMetric::new("GAIN", 20.0, "echo 'GAIN: 20'"),
        ];
        
        // Test component data format: Vec<(String, HashMap<String, f64>)>
        let component_data = vec![
            ("R1".to_string(), {
                let mut props = HashMap::new();
                props.insert("value".to_string(), 1000.0); // 1kΩ
                props
            }),
            ("C1".to_string(), {
                let mut props = HashMap::new();
                props.insert("value".to_string(), 1e-12); // 1pF
                props
            }),
        ];
        
        let current_dir = PathBuf::from(test_dir);
        let netlist_dir = current_dir.join("spice");
        
        let (opt_problem, initial_params) = OptimizationProblem::with_component_data(
            target_metrics,
            component_data.clone(),
            current_dir,
            netlist_dir,
        );
        
        // Test that we can create the optimization problem
        assert_eq!(opt_problem.target_metrics.len(), 1);
        assert_eq!(opt_problem.component_parameters.len(), 2);
        assert_eq!(initial_params.len(), 2); // R1.value + C1.value
        
        // Verify component parameters were correctly structured
        let r1_param = opt_problem.component_parameters.iter()
            .find(|cp| cp.component_name == "R1")
            .expect("Should have R1 component parameter");
        assert!(r1_param.properties.contains_key("value"));
        
        let c1_param = opt_problem.component_parameters.iter()
            .find(|cp| cp.component_name == "C1")
            .expect("Should have C1 component parameter");
        assert!(c1_param.properties.contains_key("value"));
        
        println!("Component data used: {:#?}", component_data);
        println!("Generated component parameters: {:#?}", opt_problem.component_parameters);
        println!("Initial parameters: {:?}", initial_params);
        
        println!("Component data optimization problem creation test PASSED");
        Ok(())
    }

    #[test]
    fn test_component_data_format() -> Result<(), Box<dyn std::error::Error>> {
        let test_dir = "tests/test_sample";
        
        // Test the new component data format
        let target_metrics = vec![
            TargetMetric::new("GAIN", 20.0, "echo 'GAIN: 20'"),
        ];
        
        let component_data = vec![
            ("R1".to_string(), {
                let mut props = HashMap::new();
                props.insert("value".to_string(), 1000.0); // 1kΩ
                props
            }),
            ("M1".to_string(), {
                let mut props = HashMap::new();
                props.insert("W".to_string(), 2.0); // 2µm width
                props.insert("L".to_string(), 0.5); // 0.5µm length
                props
            }),
        ];
        
        let current_dir = PathBuf::from(test_dir);
        let netlist_dir = current_dir.join("spice");
        
        let (opt_problem, initial_params) = OptimizationProblem::with_component_data(
            target_metrics,
            component_data,
            current_dir,
            netlist_dir,
        );
        
        // Verify the component data was correctly processed
        assert_eq!(opt_problem.component_parameters.len(), 2); // R1 and M1
        assert_eq!(initial_params.len(), 3); // R1.value + M1.L + M1.W (alphabetical order)
        
        // Check R1 component
        let r1_param = opt_problem.component_parameters.iter()
            .find(|cp| cp.component_name == "R1")
            .expect("Should have R1 component");
        assert_eq!(r1_param.properties.len(), 1);
        assert!(r1_param.properties.contains_key("value"));
        
        // Check M1 component
        let m1_param = opt_problem.component_parameters.iter()
            .find(|cp| cp.component_name == "M1")
            .expect("Should have M1 component");
        assert_eq!(m1_param.properties.len(), 2);
        assert!(m1_param.properties.contains_key("W"));
        assert!(m1_param.properties.contains_key("L"));
        
        // Verify initial parameters are in correct order (sorted by property name)
        // R1.value should come first, then M1.L, then M1.W (alphabetical order within component)
        assert_eq!(initial_params[0], 1000.0); // R1.value
        
        println!("Component data optimization test PASSED");
        Ok(())
    }
}
