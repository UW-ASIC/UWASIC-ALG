use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use crate::{run_spice, ComponentParameter, NgSpiceInterface, OptimizationProblem, SpiceRunConfig, SimulationResult};

#[derive(Debug)]
struct ParsedBound {
    component: String,
    parameter: String,
    min_value: f64,
    max_value: f64,
}

#[derive(Debug)]
struct ParsedTarget {
    metric: String,
    target_value: f64,
    weight: f64,
    constraint_type: String,
}

#[pyfunction]
fn optimize_circuit(
    circuit_name: String,
    initial_params: &PyDict,
    tests: &PyDict,
    targets: &PyList,
    bounds: &PyList,
    template_dir: Option<String>,
    max_iterations: Option<usize>,
    target_precision: Option<f64>,
    verbose: Option<bool>,
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
        let _template_dir = template_dir.unwrap_or_else(|| "template".to_string());
        let max_iter = max_iterations.unwrap_or(5);
        let _precision = target_precision.unwrap_or(0.9);
        let verbose_flag = verbose.unwrap_or(false);
        
        if verbose_flag {
            println!("Starting optimization for {}", circuit_name);
            println!("Max iterations: {}", max_iter);
        }
        
        // Parse input data
        let parsed_initial_params = extract_initial_params_from_python(initial_params)?;
        let parsed_bounds = extract_bounds_from_python(bounds)?;
        let parsed_targets = extract_targets_from_python(targets)?;
        
        if verbose_flag {
            println!("Parsed {} bounds and {} targets", parsed_bounds.len(), parsed_targets.len());
        }
        
        // Create component parameters from bounds and initial values
        let mut component_parameters = Vec::new();
        
        for bound in &parsed_bounds {
            let current_value = parsed_initial_params
                .get(&bound.component)
                .and_then(|params| params.get(&bound.parameter))
                .copied()
                .unwrap_or(bound.min_value);
            
            component_parameters.push(ComponentParameter {
                component_name: bound.component.clone(),
                parameter_name: bound.parameter.clone(),
                min_value: bound.min_value,
                max_value: bound.max_value,
                current_value,
            });
        }
        
        if verbose_flag {
            println!("Created {} component parameters:", component_parameters.len());
            for param in &component_parameters {
                println!("  {}.{}: {} (range: {:.3} - {:.3})", 
                    param.component_name, param.parameter_name, 
                    param.current_value, param.min_value, param.max_value);
            }
        }
        
        // Create SPICE run configurations from targets
        let mut spice_runs = Vec::new();
        for target in &parsed_targets {
            spice_runs.push(SpiceRunConfig {
                expected_metrics: vec![target.metric.clone()],
                weight: target.weight,
            });
        }
        
        // Create optimization problem
        let mut optimization_problem = OptimizationProblem::new(spice_runs, component_parameters.clone());
        
        // Set target values from parsed targets
        let mut target_map = HashMap::new();
        for (i, target) in parsed_targets.iter().enumerate() {
            let key = format!("{}_{}", target.metric, i);
            target_map.insert(key, target.target_value);
        }
        optimization_problem.set_target_values(target_map);
        
        // Run optimization iterations using real SPICE simulations
        let ngspice_interface = NgSpiceInterface::new();
        
        for iteration in 0..max_iter {
            if verbose_flag {
                println!("Iteration {}/{}", iteration + 1, max_iter);
            }
            
            // Generate and run actual SPICE simulations with current parameters
            let mut simulation_results = Vec::new();
            
            for (run_idx, run_config) in spice_runs.iter().enumerate() {
                // Generate SPICE netlist with current parameter values
                // This is a basic template - you should customize based on your circuit needs
                let mut spice_content = format!(
                    "* Circuit simulation for {} - Run {}\n",
                    circuit_name, run_idx
                );
                
                // Add parameter definitions
                for param in optimization_problem.get_current_parameters().iter().zip(&component_parameters) {
                    let (value, param_info) = (param.0, param.1);
                    spice_content.push_str(&format!(
                        ".param {}_{} = {}\n",
                        param_info.component_name, param_info.parameter_name, value
                    ));
                }
                
                // Add your actual circuit netlist here based on circuit_name
                // This is a placeholder - replace with your actual circuit generation logic
                spice_content.push_str(&format!(
                    "* Circuit: {}\n",
                    circuit_name
                ));
                spice_content.push_str("* TODO: Add your actual circuit netlist here\n");
                spice_content.push_str("* For now, this is a placeholder that will generate basic output\n");
                spice_content.push_str(".control\n");
                spice_content.push_str("op\n");
                spice_content.push_str("print all\n");
                
                // Add measurement commands for expected metrics
                for metric in &run_config.expected_metrics {
                    spice_content.push_str(&format!("print {}\n", metric));
                }
                
                spice_content.push_str(".endc\n");
                spice_content.push_str(".end\n");
                
                if verbose_flag {
                    println!("Running SPICE simulation {} with {} parameters", 
                             run_idx, component_parameters.len());
                }
                
                // Run the actual SPICE simulation
                match ngspice_interface.run_simulation(&spice_content, "ngspice") {
                    Ok(result) => {
                        if verbose_flag {
                            println!("Simulation {}: success={}, metrics={}", 
                                     run_idx, result.success, result.metrics.len());
                        }
                        simulation_results.push(result);
                    }
                    Err(e) => {
                        if verbose_flag {
                            eprintln!("Simulation {} failed: {}", run_idx, e);
                        }
                        // Create a failed simulation result
                        simulation_results.push(SimulationResult {
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                            error: Some(e),
                            simulator_used: "ngspice".to_string(),
                            execution_time: 0.0,
                            metrics: HashMap::new(),
                        });
                    }
                }
            }
            
            // Use the optimizer to calculate next parameters based on real SPICE results
            optimization_problem.iterate(simulation_results);
        }
        
        // Build result dictionary matching the expected format
        let result_dict = PyDict::new(py);
        
        for param in &current_params {
            // Get or create component dict
            let component_dict = if let Some(existing) = result_dict.get_item(&param.component_name) {
                existing.downcast::<PyDict>()?
            } else {
                let new_dict = PyDict::new(py);
                result_dict.set_item(&param.component_name, new_dict)?;
                new_dict
            };
            
            // Set parameter value as string (matching the expected format)
            component_dict.set_item(&param.parameter_name, format!("{:.6}", param.current_value))?;
        }
        
        if verbose_flag {
            println!("Optimization completed for {}", circuit_name);
        }
        
        Ok(result_dict.into())
    })
}

#[pymodule]
pub fn create_module(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(optimize_circuit, m)?)?;
    Ok(())
}

fn extract_bounds_from_python(bounds_list: &PyList) -> PyResult<Vec<ParsedBound>> {
    let mut bounds = Vec::new();
    
    for item in bounds_list.iter() {
        let bound_dict = item.downcast::<PyDict>()?;
        
        // Extract required fields
        let component = bound_dict
            .get_item("component")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'component' key"))?
            .extract::<String>()?;
            
        let parameter = bound_dict
            .get_item("parameter")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'parameter' key"))?
            .extract::<String>()?;
            
        let min_value = bound_dict
            .get_item("min_value")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'min_value' key"))?
            .extract::<f64>()?;
            
        let max_value = bound_dict
            .get_item("max_value")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'max_value' key"))?
            .extract::<f64>()?;
        
        bounds.push(ParsedBound {
            component,
            parameter,
            min_value,
            max_value,
        });
    }
    
    Ok(bounds)
}

fn extract_targets_from_python(targets_list: &PyList) -> PyResult<Vec<ParsedTarget>> {
    let mut targets = Vec::new();
    
    for item in targets_list.iter() {
        let target_dict = item.downcast::<PyDict>()?;
        
        let metric = target_dict
            .get_item("metric")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'metric' key"))?
            .extract::<String>()?;
            
        let target_value = target_dict
            .get_item("target_value")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'target_value' key"))?
            .extract::<f64>()?;
            
        let weight = target_dict
            .get_item("weight")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'weight' key"))?
            .extract::<f64>()?;
            
        let constraint_type = target_dict
            .get_item("constraint_type")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'constraint_type' key"))?
            .extract::<String>()?;
        
        targets.push(ParsedTarget {
            metric,
            target_value,
            weight,
            constraint_type,
        });
    }
    
    Ok(targets)
}

fn extract_initial_params_from_python(initial_params: &PyDict) -> PyResult<HashMap<String, HashMap<String, f64>>> {
    let mut params = HashMap::new();
    
    for (component_key, component_value) in initial_params.iter() {
        let component_name = component_key.extract::<String>()?;
        let component_dict = component_value.downcast::<PyDict>()?;
        let mut component_params = HashMap::new();
        
        for (param_key, param_value) in component_dict.iter() {
            let param_name = param_key.extract::<String>()?;
            let param_str = param_value.extract::<String>()?;
            let param_val = param_str.parse::<f64>()
                .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Could not parse '{}' as float for {}:{}", param_str, component_name, param_name)
                ))?;
            component_params.insert(param_name, param_val);
        }
        
        params.insert(component_name, component_params);
    }
    
    Ok(params)
}
