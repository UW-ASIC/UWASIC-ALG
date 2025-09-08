mod optimizer;
mod utilities;
mod xschem;
mod ngspice;
mod pyinterface;
mod solvers;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::path::PathBuf;
use optimizer::{OptimizationProblem, TargetMetric};
use solvers::{SolverManager, SolverConfig, SolverType};
use pyinterface::{extract_all_parameters, convert_results_to_python};

// Constants
const DEFAULT_TOLERANCE: f64 = 1e-6;
const PERTURBATION_FACTOR: f64 = 0.05;
const MIN_PERTURBATION: f64 = 0.1;

// Runtime verbose macros
#[macro_export]
macro_rules! vprintln {
    ($verbose:expr, $($arg:tt)*) => {
        if $verbose {
            use std::io::{self, Write};
            let _ = writeln!(io::stdout(), $($arg)*);
        }
    };
}

// Safe printing macro that handles broken pipe errors
#[macro_export]
macro_rules! safe_println {
    ($($arg:tt)*) => {
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), $($arg)*);
    };
}

#[pyfunction]
fn optimize_circuit(
    py: Python,
    circuit_name: String,
    initial_params: &Bound<PyDict>,
    tests: &Bound<PyDict>,
    targets: &Bound<PyList>,
    bounds: &Bound<PyList>,
    template_dir: Option<&Bound<PyAny>>,
    max_iterations: Option<&Bound<PyAny>>,
    target_precision: Option<&Bound<PyAny>>,
    solver_type: Option<&Bound<PyAny>>,
    verbose: Option<&Bound<PyAny>>,
) -> PyResult<Py<PyAny>> {
    vprintln!(true, "🚀 Starting optimization for {}", circuit_name);
    
    // Parse all input data using pyinterface
    vprintln!(true, "\n📊 Parsing input data using pythonize...");
    let (
        parsed_initial_params,
        parsed_tests,
        parsed_targets,
        parsed_bounds,
        max_iter,
        precision,
        template_dir_str,
        verbose_flag,
    ) = extract_all_parameters(
        initial_params,
        tests,
        targets,
        bounds,
        max_iterations,
        target_precision,
        template_dir,
        verbose,
    )?;
    
    vprintln!(verbose_flag, "Configuration:");
    vprintln!(verbose_flag, "  Max iterations: {}", max_iter);
    vprintln!(verbose_flag, "  Target precision: {}", precision);
    vprintln!(verbose_flag, "  Template directory: {}", template_dir_str);
    vprintln!(verbose_flag, "  Press Ctrl+C to gracefully stop optimization");
    
    // Inline count_total_params
    let total_params: usize = parsed_initial_params.values().map(|p| p.len()).sum();
    vprintln!(verbose_flag, "✓ Parsing completed:");
    vprintln!(verbose_flag, "  Parameters: {}", total_params);
    vprintln!(verbose_flag, "  Bounds: {}", parsed_bounds.len());
    vprintln!(verbose_flag, "  Targets: {}", parsed_targets.len());
    vprintln!(verbose_flag, "  Tests: {}", parsed_tests.len());
    
    // Inline convert_to_component_data
    vprintln!(verbose_flag, "\n🔄 Converting data structures...");
    vprintln!(verbose_flag, "  Converting {} components to optimization format", parsed_initial_params.len());
    let component_data: Vec<_> = parsed_initial_params.clone().into_iter().collect();
    if verbose_flag {
        for (component, params) in &component_data {
            vprintln!(verbose_flag, "    Component {}: {} parameters", component, params.len());
        }
    }
    
    // Inline create_target_metrics_from_tests_and_targets
    vprintln!(verbose_flag, "🎯 Creating target metrics...");
    let mut target_metrics = Vec::new();
    vprintln!(verbose_flag, "  Processing {} targets with {} available tests", parsed_targets.len(), parsed_tests.len());
    
    // Inline build_metric_to_spice_map
    let mut metric_to_spice = HashMap::new();
    vprintln!(verbose_flag, "  Building metric to SPICE code mapping...");
    
    for (test_name, test_config) in &parsed_tests {
        let spice_code = test_config.get_spice_code();
        if !spice_code.is_empty() {
            vprintln!(verbose_flag, "    Scanning test '{}' ({} chars of SPICE)", 
                     test_name, spice_code.len());
            
            // Inline extract_metrics_from_spice
            let mut metrics = Vec::new();
            for line in spice_code.lines() {
                if line.contains("echo '") && line.contains(":' $") {
                    if let Some(start) = line.find("echo '") {
                        let after_echo = &line[start + 6..];
                        if let Some(end) = after_echo.find(":'") {
                            let metric = after_echo[..end].to_string();
                            if !metrics.contains(&metric) {
                                metrics.push(metric);
                            }
                        }
                    }
                }
            }
            
            if verbose_flag && !metrics.is_empty() {
                vprintln!(verbose_flag, "        Extracted metrics: {:?}", metrics);
            }
            
            // Associate each metric with this SPICE code
            for metric in metrics {
                vprintln!(verbose_flag, "      Found metric: {}", metric);
                metric_to_spice.insert(metric, spice_code.clone());
            }
        }
    }
    
    vprintln!(verbose_flag, "  ✓ Mapped {} metrics to SPICE code", metric_to_spice.len());
    
    // Create target metrics
    for target in &parsed_targets {
        vprintln!(verbose_flag, "    Target {}: value={:.6e}, weight={}", 
                 target.metric, target.target_value, target.weight);
        
        let spice_code = metric_to_spice.get(&target.metric)
            .cloned()
            .unwrap_or_else(|| {
                vprintln!(verbose_flag, "      ⚠️ No SPICE code found for metric '{}', using empty string", target.metric);
                String::new()
            });
        
        if !spice_code.is_empty() {
            vprintln!(verbose_flag, "      ✓ Found SPICE code for metric ({} characters)", spice_code.len());
        }
        
        target_metrics.push(TargetMetric::new(
            &target.metric,
            target.target_value,
            &spice_code
        ));
    }
    
    vprintln!(verbose_flag, "  ✓ Created {} target metrics", target_metrics.len());
    
    // Determine work directory - make it absolute
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(template_dir_str);
    let netlist_dir = PathBuf::from("spice");
    
    vprintln!(verbose_flag, "📁 Directory setup:");
    vprintln!(verbose_flag, "  Current directory: {}", current_dir.display());
    vprintln!(verbose_flag, "  Netlist directory: {}", netlist_dir.display());
    
    // Create optimization problem
    vprintln!(verbose_flag, "\n🏗️ Creating optimization problem...");
    let Ok((optimization_problem, initial_params_vec)) = OptimizationProblem::new(
        target_metrics,
        component_data,
        parsed_tests,
        current_dir,
        netlist_dir,
        verbose_flag,
    ) else {
        vprintln!(verbose_flag, "❌ Failed to create optimization problem");
        return convert_results_to_python(py, &parsed_initial_params);
    };
    
    vprintln!(verbose_flag, "✓ Optimization problem created:");
    vprintln!(verbose_flag, "  Parameter vector length: {}", initial_params_vec.len());
    vprintln!(verbose_flag, "  Initial parameters: {:?}", initial_params_vec);
    
    // Parse solver type
    let solver_type_str = solver_type
        .and_then(|s| s.extract::<String>().ok())
        .unwrap_or_else(|| "auto".to_string());
    
    let selected_solver = match solver_type_str.parse::<SolverType>() {
        Ok(solver) => solver,
        Err(_) => {
            vprintln!(verbose_flag, "⚠️ Invalid solver type '{}', falling back to Auto", solver_type_str);
            SolverType::Auto
        }
    };
    
    vprintln!(verbose_flag, "🔧 Selected solver: {}", selected_solver);
    vprintln!(verbose_flag, "   {}", selected_solver.description());
    
    // Create solver configuration
    let config = SolverConfig::new(selected_solver)
        .with_tolerance(precision)
        .with_max_iterations(max_iter as u64);
    
    // Create solver manager and run optimization
    let solver_manager = SolverManager::new(verbose_flag);
    let result = solver_manager.run_optimization(
        optimization_problem.clone(),
        initial_params_vec.clone(),
        config,
    );
    
    match result {
        Ok((best_params, best_cost, iterations)) => {
            vprintln!(verbose_flag, "\n🎉 Optimization completed successfully!");
            vprintln!(verbose_flag, "Results:");
            vprintln!(verbose_flag, "  Best cost: {:.6e}", best_cost);
            vprintln!(verbose_flag, "  Iterations: {}", iterations);
            vprintln!(verbose_flag, "  Optimized parameters: {:?}", best_params);
            
            // Inline convert_params_vector_to_map
            vprintln!(verbose_flag, "\n📦 Converting results back to Python format...");
            let mut result_params = HashMap::new();
            let mut param_index = 0;
            
            // Sort component names for consistent ordering
            let mut components: Vec<_> = parsed_initial_params.keys().collect();
            components.sort();
            
            for component_name in components {
                if let Some(params) = parsed_initial_params.get(component_name) {
                    let mut component_params = HashMap::new();
                    
                    // Sort parameter names for consistent ordering
                    let mut param_names: Vec<_> = params.keys().collect();
                    param_names.sort();
                    
                    vprintln!(verbose_flag, "  Component {}: {} parameters", component_name, param_names.len());
                    
                    for param_name in param_names {
                        if param_index < best_params.len() {
                            let value = best_params[param_index];
                            component_params.insert(param_name.clone(), value);
                            
                            vprintln!(verbose_flag, "    {}[{}] = {:.6}", component_name, param_name, value);
                            param_index += 1;
                        }
                    }
                    
                    result_params.insert(component_name.clone(), component_params);
                }
            }
            
            convert_results_to_python(py, &result_params)
        }
        Err(e) => {
            vprintln!(verbose_flag, "\n❌ Optimization failed: {}", e);
            vprintln!(verbose_flag, "📤 Returning original parameters as fallback");
            
            // Return the original parameters if optimization fails
            convert_results_to_python(py, &parsed_initial_params)
        }
    }
}

#[pyfunction]
fn list_available_solvers(py: Python) -> PyResult<Py<PyAny>> {
    let solvers = solvers::list_available_solvers();
    let solver_list: Vec<(String, String)> = solvers
        .into_iter()
        .map(|(solver_type, description)| (solver_type.to_string(), description.to_string()))
        .collect();
    
    Ok(pythonize::pythonize(py, &solver_list).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to convert solver list: {}", e))
    })?.into())
}

#[pyfunction]
fn recommend_solver(
    py: Python,
    num_params: usize,
    has_noise: Option<bool>,
    is_multimodal: Option<bool>,
    requires_global: Option<bool>,
) -> PyResult<Py<PyAny>> {
    let recommendations = solvers::recommend_solver_for_problem(
        num_params,
        has_noise.unwrap_or(true), // Assume noisy by default for circuit optimization
        is_multimodal.unwrap_or(false),
        requires_global.unwrap_or(false),
    );
    
    let recommendation_list: Vec<(String, String)> = recommendations
        .into_iter()
        .map(|solver_type| (solver_type.to_string(), solver_type.description().to_string()))
        .collect();
    
    Ok(pythonize::pythonize(py, &recommendation_list).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to convert recommendations: {}", e))
    })?.into())
}

#[pyfunction]
fn get_solver_info(py: Python, solver_name: String) -> PyResult<Py<PyAny>> {
    match solver_name.parse::<SolverType>() {
        Ok(solver_type) => {
            let info = std::collections::HashMap::from([
                ("name".to_string(), solver_type.to_string()),
                ("description".to_string(), solver_type.description().to_string()),
                ("requires_gradients".to_string(), solver_type.requires_gradients().to_string()),
                ("supports_multidimensional".to_string(), solver_type.supports_multidimensional().to_string()),
            ]);
            
            Ok(pythonize::pythonize(py, &info).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to convert solver info: {}", e))
            })?.into())
        },
        Err(e) => {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(e))
        }
    }
}

#[pymodule]
fn xschemoptimizer(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(optimize_circuit, m)?)?;
    m.add_function(wrap_pyfunction!(list_available_solvers, m)?)?;
    m.add_function(wrap_pyfunction!(recommend_solver, m)?)?;
    m.add_function(wrap_pyfunction!(get_solver_info, m)?)?;
    Ok(())
}
