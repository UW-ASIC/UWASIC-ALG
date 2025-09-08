mod optimizer;
mod xschem;
mod utilities;
mod pyinterface;
mod pydata;

// External crate uses
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pythonize;
use std::collections::HashMap;
use std::path::PathBuf;

// Local uses
use optimizer::{CircuitOptimizer, TargetMetric, OptimizationResult};
use optimizer::solver::{SolverType, SolverConfig};
use pyinterface::{extract_all_parameters, convert_results_to_python};

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
) -> PyResult<Py<PyDict>> {
    // Parse all input data using pyinterface (same as src/)
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

    // Create target metrics like src/ does
    let mut target_metrics = Vec::new();
    
    // Build metric to SPICE map first (from src/)
    let mut metric_to_spice = HashMap::new();
    for (test_name, test_config) in &parsed_tests {
        let spice_code = test_config.get_spice_code();
        if !spice_code.is_empty() {
            // Extract metrics from SPICE code (simplified)
            if let Some(start) = spice_code.find("echo '") {
                let after_echo = &spice_code[start + 6..];
                if let Some(end) = after_echo.find(":'") {
                    let metric = after_echo[..end].to_string();
                    metric_to_spice.insert(metric, spice_code.clone());
                }
            }
        }
    }
    
    // Create target metrics
    for target in &parsed_targets {
        let spice_code = metric_to_spice.get(&target.metric)
            .cloned()
            .unwrap_or_else(|| String::new());
        
        target_metrics.push(TargetMetric::new(
            &target.metric,
            target.target_value,
            &spice_code,
        ));
    }

    // Convert initial parameters to component data format
    let component_data: Vec<(String, HashMap<String, f64>)> = parsed_initial_params
        .into_iter()
        .collect();

    // Set up directories
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(template_dir_str);
    let netlist_dir = PathBuf::from("spice");

    // Parse solver type
    let solver_type_str = solver_type
        .and_then(|s| s.extract::<String>().ok())
        .unwrap_or_else(|| "auto".to_string());
    
    let solver_type = solver_type_str.parse::<SolverType>()
        .unwrap_or(SolverType::Auto);

    // Create optimizer and run optimization
    let optimizer = CircuitOptimizer::new(verbose_flag);
    
    let result = match optimizer.optimize_with_solver(
        target_metrics,
        component_data,
        parsed_tests,
        current_dir,
        netlist_dir,
        solver_type,
        max_iter as u64,
        precision,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Return original parameters as fallback (like src/)
            return convert_results_to_python(py, &parsed_initial_params);
        }
    };

    // Convert results back to Python format (same as src/)
    convert_results_to_python(py, &result.optimized_params)
}

/// List available solvers
#[pyfunction]
fn list_available_solvers(py: Python) -> PyResult<Py<PyAny>> {
    let solvers = optimizer::solver::list_solvers();
    let solver_list: Vec<(String, String)> = solvers
        .into_iter()
        .map(|(solver_type, description)| (solver_type.to_string(), description.to_string()))
        .collect();
    
    Ok(pythonize::pythonize(py, &solver_list).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Failed to convert solver list: {}", e))
    })?.into())
}

/// Get solver recommendation based on problem characteristics
#[pyfunction]
fn recommend_solver(
    py: Python,
    num_params: usize,
    has_noise: Option<bool>,
    is_multimodal: Option<bool>,
    requires_global: Option<bool>,
) -> PyResult<Py<PyAny>> {
    let recommendations = optimizer::solver::recommend_solver(
        num_params,
        has_noise.unwrap_or(true),
        requires_global.unwrap_or(false),
    );
    
    // Return single recommendation as a list for consistency with src/
    let recommendation_list = vec![(recommendations.to_string(), recommendations.description().to_string())];
    
    Ok(pythonize::pythonize(py, &recommendation_list).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Failed to convert recommendations: {}", e))
    })?.into())
}

/// Get detailed solver information (matches src/ API)
#[pyfunction]
fn get_solver_info(py: Python, solver_name: String) -> PyResult<Py<PyAny>> {
    match solver_name.parse::<SolverType>() {
        Ok(solver_type) => {
            let info = std::collections::HashMap::from([
                ("name".to_string(), solver_type.to_string()),
                ("description".to_string(), solver_type.description().to_string()),
                ("requires_gradients".to_string(), (!solver_type.is_derivative_free()).to_string()),
                ("supports_multidimensional".to_string(), "true".to_string()),
            ]);
            
            Ok(pythonize::pythonize(py, &info).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Failed to convert solver info: {}", e))
            })?.into())
        },
        Err(e) => {
            Err(pyo3::exceptions::PyValueError::new_err(e))
        }
    }
}

#[pymodule]
fn xschemoptimizer(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add main optimization function
    m.add_function(wrap_pyfunction!(optimize_circuit, m)?)?;
    
    // Add utility functions
    m.add_function(wrap_pyfunction!(list_available_solvers, m)?)?;
    m.add_function(wrap_pyfunction!(recommend_solver, m)?)?;
    m.add_function(wrap_pyfunction!(get_solver_info, m)?)?;
    
    Ok(())
}
