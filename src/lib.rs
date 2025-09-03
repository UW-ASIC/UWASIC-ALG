mod optimizer;
mod utilities;
mod xschem;
mod ngspice;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::path::PathBuf;
use optimizer::{OptimizationProblem, TargetMetric};
use argmin::core::{Executor};
use argmin::solver::neldermead::NelderMead;

// Runtime verbose macros
macro_rules! vprintln {
    ($verbose:expr, $($arg:tt)*) => {
        if $verbose {
            println!($($arg)*);
        }
    };
}

macro_rules! vprint {
    ($verbose:expr, $($arg:tt)*) => {
        if $verbose {
            print!($($arg)*);
        }
    };
}

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
    py: Python,
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
    let _template_dir = template_dir.unwrap_or_else(|| "template".to_string());
    let max_iter = max_iterations.unwrap_or(100);
    let _precision = target_precision.unwrap_or(0.9);
    let verbose_flag = verbose.unwrap_or(false);
    
    vprintln!(verbose_flag, "🚀 Starting optimization for {}", circuit_name);
    vprintln!(verbose_flag, "Configuration:");
    vprintln!(verbose_flag, "  Max iterations: {}", max_iter);
    vprintln!(verbose_flag, "  Template directory: {}", _template_dir);
    vprintln!(verbose_flag, "  Target precision: {}", _precision);
    
    // Parse input data
    vprintln!(verbose_flag, "\n📊 Parsing input data...");
    let parsed_initial_params = extract_initial_params_from_python(initial_params, verbose_flag)?;
    let parsed_bounds = extract_bounds_from_python(bounds, verbose_flag)?;
    let parsed_targets = extract_targets_from_python(targets, verbose_flag)?;
    let parsed_tests = extract_tests_from_python(tests, verbose_flag)?;
    
    let total_params = count_total_params(&parsed_initial_params);
    vprintln!(verbose_flag, "✓ Parsing completed:");
    vprintln!(verbose_flag, "  Parameters: {}", total_params);
    vprintln!(verbose_flag, "  Bounds: {}", parsed_bounds.len());
    vprintln!(verbose_flag, "  Targets: {}", parsed_targets.len());
    vprintln!(verbose_flag, "  Tests: {}", parsed_tests.len());
    
    // Convert parsed data to component data format for OptimizationProblem
    vprintln!(verbose_flag, "\n🔄 Converting data structures...");
    let component_data = convert_to_component_data(parsed_initial_params.clone(), verbose_flag);
    
    // Create target metrics from tests and targets
    vprintln!(verbose_flag, "🎯 Creating target metrics...");
    let target_metrics = create_target_metrics_from_tests_and_targets(&parsed_tests, &parsed_targets, verbose_flag);
    
    // Determine work directory (assume current directory if not specified)
    let current_dir = PathBuf::from(".");
    let netlist_dir = current_dir.join("spice");
    
    vprintln!(verbose_flag, "📁 Directory setup:");
    vprintln!(verbose_flag, "  Current directory: {}", current_dir.display());
    vprintln!(verbose_flag, "  Netlist directory: {}", netlist_dir.display());
    
    // Create optimization problem
    vprintln!(verbose_flag, "\n🏗️ Creating optimization problem...");
    let (optimization_problem, initial_params_vec) = OptimizationProblem::with_component_data(
        target_metrics,
        component_data,
        current_dir,
        netlist_dir,
        verbose_flag,  // Pass verbose flag here
    );
    
    vprintln!(verbose_flag, "✓ Optimization problem created:");
    vprintln!(verbose_flag, "  Parameter vector length: {}", initial_params_vec.len());
    vprintln!(verbose_flag, "  Initial parameters: {:?}", initial_params_vec);
    
    // Run optimization using Nelder-Mead (default solver)
    vprintln!(verbose_flag, "\n🎲 Starting Nelder-Mead optimization...");
    let result = run_nelder_mead_optimization(
        optimization_problem.clone(),
        initial_params_vec,
        max_iter,
        1e-6, // Default tolerance
        verbose_flag
    );
    
    match result {
        Ok((best_params, best_cost, iterations)) => {
            vprintln!(verbose_flag, "\n🎉 Optimization completed successfully!");
            vprintln!(verbose_flag, "Results:");
            vprintln!(verbose_flag, "  Best cost: {:.6e}", best_cost);
            vprintln!(verbose_flag, "  Iterations: {}", iterations);
            vprintln!(verbose_flag, "  Optimized parameters: {:?}", best_params);
            
            // Convert results back to the exact same format as input
            vprintln!(verbose_flag, "\n📦 Converting results back to Python format...");
            let result_dict = PyDict::new(py);
            let mut param_index = 0;
            
            // Iterate through the original parameter structure to maintain order
            for (component_name, params) in parsed_initial_params {
                let component_dict = PyDict::new(py);
                
                // Sort parameters for consistent ordering
                let mut sorted_params: Vec<_> = params.keys().collect();
                sorted_params.sort();
                
                vprintln!(verbose_flag, "  Component {}: {} parameters", component_name, sorted_params.len());
                
                for param_name in sorted_params {
                    if param_index < best_params.len() {
                        // Format as string with high precision to match expected format
                        let formatted_value = format!("{:.6}", best_params[param_index]);
                        component_dict.set_item(param_name, &formatted_value)?;
                        
                        vprintln!(verbose_flag, "    {}[{}] = {}", component_name, param_name, formatted_value);
                        param_index += 1;
                    }
                }
                
                result_dict.set_item(component_name, component_dict)?;
            }
            
            vprintln!(verbose_flag, "✓ Result conversion completed");
            Ok(result_dict.into())
        }
        Err(e) => {
            vprintln!(verbose_flag, "\n❌ Optimization failed: {}", e);
            vprintln!(verbose_flag, "📤 Returning original parameters as fallback");
            
            // Return the original parameters if optimization fails
            Ok(initial_params.into())
        }
    }
}

fn run_nelder_mead_optimization(
    problem: OptimizationProblem,
    initial_params: Vec<f64>,
    max_iter: usize,
    tolerance: f64,
    verbose: bool,
) -> Result<(Vec<f64>, f64, u64), String> {
    vprintln!(verbose, "🔺 Setting up Nelder-Mead solver...");
    
    // Create simplex vertices with small perturbations
    let n = initial_params.len();
    let mut simplex = vec![initial_params.clone()];
    
    vprintln!(verbose, "  Creating simplex with {} vertices for {} parameters", n + 1, n);
    
    // Add perturbed vertices
    for i in 0..n {
        let mut vertex = initial_params.clone();
        let perturbation = 0.05 * vertex[i].abs().max(0.1); // 5% perturbation or minimum 0.05
        vertex[i] += perturbation;
        simplex.push(vertex);
        
        vprintln!(verbose, "  Vertex {}: parameter {} perturbed by {:.6}", i + 1, i, perturbation);
    }
    
    vprintln!(verbose, "  Tolerance: {:.2e}", tolerance);
    
    let solver = NelderMead::new(simplex)
        .with_sd_tolerance(tolerance)
        .map_err(|e| format!("Failed to create Nelder-Mead solver: {}", e))?;
    
    vprintln!(verbose, "🎯 Running optimization...");
    
    let result = Executor::new(problem, solver)
        .configure(|state| {
            state
                .param(initial_params)
                .max_iters(max_iter.try_into().unwrap())
        })
        .run()
        .map_err(|e| format!("Optimization failed: {}", e))?;
    
    vprintln!(verbose, "✓ Nelder-Mead optimization completed in {} iterations", result.state.iter);
    vprintln!(verbose, "  Final cost: {:.6e}", result.state.best_cost);
    
    Ok((
        result.state.best_param.unwrap_or_default(),
        result.state.best_cost,
        result.state.iter,
    ))
}

fn convert_to_component_data(parsed_params: HashMap<String, HashMap<String, f64>>, verbose: bool) -> Vec<(String, HashMap<String, f64>)> {
    vprintln!(verbose, "  Converting {} components to optimization format", parsed_params.len());
    
    let result: Vec<_> = parsed_params.into_iter().collect();
    
    if verbose {
        for (component, params) in &result {
            vprintln!(verbose, "    Component {}: {} parameters", component, params.len());
        }
    }
    
    result
}

fn create_target_metrics_from_tests_and_targets(
    tests: &HashMap<String, HashMap<String, String>>,
    targets: &[ParsedTarget],
    verbose: bool,
) -> Vec<TargetMetric> {
    let mut target_metrics = Vec::new();
    
    vprintln!(verbose, "  Processing {} targets with {} available tests", targets.len(), tests.len());
    
    for target in targets {
        vprintln!(verbose, "    Target {}: value={:.6e}, weight={}", 
                 target.metric, target.target_value, target.weight);
        
        // Look for the SPICE code in the tests that might measure this metric
        let spice_code = find_spice_code_for_metric(&target.metric, tests, verbose)
            .unwrap_or_else(|| {
                vprintln!(verbose, "      No specific test found, generating default SPICE code");
                generate_default_spice_code(&target.metric, verbose)
            });
        
        target_metrics.push(TargetMetric::new(
            &target.metric,
            target.target_value,
            &spice_code
        ));
        
        vprintln!(verbose, "      ✓ Target metric created");
    }
    
    vprintln!(verbose, "  ✓ Created {} target metrics", target_metrics.len());
    target_metrics
}

fn find_spice_code_for_metric(metric: &str, tests: &HashMap<String, HashMap<String, String>>, verbose: bool) -> Option<String> {
    vprintln!(verbose, "      Searching for SPICE code for metric: {}", metric);
    
    // Search through all tests for SPICE code that contains the metric
    for (test_name, test_config) in tests {
        if let Some(spice_code) = test_config.get("spice") {
            if spice_code.contains(&format!("'{}':", metric)) || 
               spice_code.contains(&format!("'{}:'", metric)) {
                vprintln!(verbose, "        ✓ Found SPICE code in test: {}", test_name);
                return Some(spice_code.clone());
            }
        }
    }
    
    vprintln!(verbose, "        No matching SPICE code found in tests");
    None
}

fn generate_default_spice_code(metric: &str, verbose: bool) -> String {
    vprintln!(verbose, "        Generating default SPICE code for: {}", metric);
    
    let code = match metric.to_uppercase().as_str() {
        "DC_GAIN" => {
            vprintln!(verbose, "          Using AC analysis template for DC_GAIN");
            concat!(
                ".ac dec 100 0.1 1G\n",
                ".control\n",
                "run\n",
                "let dc_gain_val = vdb(vout)[0]\n",
                "echo 'DC_GAIN:' $&dc_gain_val\n",
                ".endc"
            ).to_string()
        },
        
        "GBW" => {
            vprintln!(verbose, "          Using AC analysis template for GBW");
            concat!(
                ".ac dec 100 0.1 1G\n",
                ".control\n",
                "run\n",
                "let gbw_freq = vecmax(frequency)\n",
                "echo 'GBW:' $&gbw_freq\n",
                ".endc"
            ).to_string()
        },
        
        "POWER" => {
            vprintln!(verbose, "          Using operating point template for POWER");
            concat!(
                ".op\n",
                ".control\n",
                "run\n",
                "let power_consumption = vdd#branch * 1.8\n",
                "echo 'POWER:' $&power_consumption\n",
                ".endc"
            ).to_string()
        },
        
        _ => {
            vprintln!(verbose, "          Using generic template for: {}", metric);
            format!(concat!(
                ".op\n",
                ".control\n",
                "run\n",
                "echo '{}:' 0\n",
                ".endc"
            ), metric.to_uppercase())
        }
    };
    
    vprintln!(verbose, "          ✓ Generated {} lines of SPICE code", code.lines().count());
    code
}

fn count_total_params(params: &HashMap<String, HashMap<String, f64>>) -> usize {
    params.values().map(|p| p.len()).sum()
}

fn extract_tests_from_python(tests_dict: &PyDict, verbose: bool) -> PyResult<HashMap<String, HashMap<String, String>>> {
    let mut tests = HashMap::new();
    
    vprintln!(verbose, "  Extracting {} tests from Python", tests_dict.len());
    
    for (test_name, test_config) in tests_dict.iter() {
        let name = test_name.extract::<String>()?;
        let config_dict = test_config.downcast::<PyDict>()?;
        let mut config = HashMap::new();
        
        vprintln!(verbose, "    Test '{}': {} configuration items", name, config_dict.len());
        
        for (key, value) in config_dict.iter() {
            let key_str = key.extract::<String>()?;
            let value_str = value.extract::<String>()?;
            config.insert(key_str.clone(), value_str);
            
            vprintln!(verbose, "      {}: {} characters", key_str, value.extract::<String>()?.len());
        }
        
        tests.insert(name, config);
    }
    
    vprintln!(verbose, "  ✓ Extracted {} tests", tests.len());
    Ok(tests)
}

fn extract_bounds_from_python(bounds_list: &PyList, verbose: bool) -> PyResult<Vec<ParsedBound>> {
    let mut bounds = Vec::new();
    
    vprintln!(verbose, "  Extracting {} bounds from Python", bounds_list.len());
    
    for (i, item) in bounds_list.iter().enumerate() {
        let bound_dict = item.downcast::<PyDict>()?;
        
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
        
        vprintln!(verbose, "    Bound {}: {}[{}] ∈ [{:.3}, {:.3}]", 
                 i, component, parameter, min_value, max_value);
        
        bounds.push(ParsedBound {
            component,
            parameter,
            min_value,
            max_value,
        });
    }
    
    vprintln!(verbose, "  ✓ Extracted {} bounds", bounds.len());
    Ok(bounds)
}

fn extract_targets_from_python(targets_list: &PyList, verbose: bool) -> PyResult<Vec<ParsedTarget>> {
    let mut targets = Vec::new();
    
    vprintln!(verbose, "  Extracting {} targets from Python", targets_list.len());
    
    for (i, item) in targets_list.iter().enumerate() {
        let target_dict = item.downcast::<PyDict>()?;
        
        let metric = target_dict
            .get_item("metric")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'metric' key"))?
            .extract::<String>()?;
            
        let target_value = target_dict
            .get_item("target_value")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("Missing 'target_value' key"))?
            .extract::<f64>()?;
            
        let weight = match target_dict.get_item("weight") {
            Some(item) => item.extract::<f64>()?,
            None => 1.0,
        };
            
        let constraint_type = match target_dict.get_item("constraint_type") {
            Some(item) => item.extract::<String>()?,
            None => "eq".to_string(),
        };
        
        vprintln!(verbose, "    Target {}: {} = {:.6e} (weight: {}, type: {})", 
                 i, metric, target_value, weight, constraint_type);
        
        targets.push(ParsedTarget {
            metric,
            target_value,
            weight,
            constraint_type,
        });
    }
    
    vprintln!(verbose, "  ✓ Extracted {} targets", targets.len());
    Ok(targets)
}

fn extract_initial_params_from_python(initial_params: &PyDict, verbose: bool) -> PyResult<HashMap<String, HashMap<String, f64>>> {
    let mut params = HashMap::new();
    
    vprintln!(verbose, "  Extracting initial parameters from Python ({} components)", initial_params.len());
    
    for (component_key, component_value) in initial_params.iter() {
        let component_name = component_key.extract::<String>()?;
        let component_dict = component_value.downcast::<PyDict>()?;
        let mut component_params = HashMap::new();
        
        vprintln!(verbose, "    Component '{}': {} parameters", component_name, component_dict.len());
        
        for (param_key, param_value) in component_dict.iter() {
            let param_name = param_key.extract::<String>()?;
            let param_str = param_value.extract::<String>()?;
            let param_val = param_str.parse::<f64>()
                .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Could not parse '{}' as float for {}:{}", param_str, component_name, param_name)
                ))?;
            
            vprintln!(verbose, "      {}[{}] = {:.6}", component_name, param_name, param_val);
            component_params.insert(param_name, param_val);
        }
        
        params.insert(component_name, component_params);
    }
    
    let total_params: usize = params.values().map(|p| p.len()).sum();
    vprintln!(verbose, "  ✓ Extracted {} components with {} total parameters", params.len(), total_params);
    
    Ok(params)
}

#[pymodule]
fn xschemoptimizer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(optimize_circuit, m)?)?;
    Ok(())
}

// Example usage documentation
/*
Python Example Usage:

```python
import xschemoptimizer

# Define initial parameters
INITIAL_PARAMS = {
    "M1": {"W": "10.0", "L": "0.5"},
    "M2": {"W": "10.0", "L": "0.5"}
}

# Define tests with SPICE code
TESTS = {
    "dc_analysis": {
        "spice": '''
            .ac dec 100 0.1 1G
            .control
            run
            let dc_gain_val = vdb(vout)[0]
            echo 'DC_GAIN:' $&dc_gain_val
            .endc
        '''
    }
}

# Define optimization targets
TARGETS = [
    {"metric": "DC_GAIN", "target_value": 60.0, "weight": 1.0, "constraint_type": "min"}
]

# Define parameter bounds
BOUNDS = [
    {"component": "M1", "parameter": "W", "min_value": 0.5, "max_value": 50.0},
    {"component": "M1", "parameter": "L", "min_value": 0.1, "max_value": 5.0},
    {"component": "M2", "parameter": "W", "min_value": 0.5, "max_value": 50.0},
    {"component": "M2", "parameter": "L", "min_value": 0.1, "max_value": 5.0}
]

# Run optimization
result = xschemoptimizer.optimize_circuit(
    "TestCircuit",
    INITIAL_PARAMS,
    TESTS, 
    TARGETS,
    BOUNDS,
    template_dir="template",
    max_iterations=100,
    target_precision=0.9,
    verbose=True
)

print("Optimized parameters:", result)
```
*/
