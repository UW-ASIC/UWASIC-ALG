use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyAny};
use pyo3::Bound;
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generic conversion function from any Python object to Rust type using serde
pub fn convert_from_python<T>(py_obj: &Bound<PyAny>) -> PyResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    depythonize(py_obj).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!("Conversion error: {}", e))
    })
}

/// Generic conversion function from Rust type to Python object using serde
pub fn convert_to_python<T>(py: Python, value: &T) -> PyResult<Py<PyAny>>
where
    T: Serialize,
{
    pythonize(py, value).map(|bound| bound.unbind()).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!("Conversion error: {}", e))
    })
}

/// Struct definitions that match the Python data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBound {
    pub component: String,
    pub parameter: String,
    pub min_value: f64,
    pub max_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTarget {
    pub metric: String,
    pub target_value: f64,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default = "default_constraint_type")]
    pub constraint_type: String,
}

fn default_weight() -> f64 { 1.0 }
fn default_constraint_type() -> String { "eq".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfiguration {
    #[serde(flatten)]
    pub component_values: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spice: Option<String>,
}

impl TestConfiguration {
    pub fn get_spice_code(&self) -> String {
        self.spice.clone().unwrap_or_default()
    }
    
    pub fn get_component_values(&self) -> HashMap<String, String> {
        let mut values = self.component_values.clone();
        values.remove("spice"); // Remove spice key if it exists in flattened data
        values
    }
}

/// Conversion functions for the specific types used in optimize_circuit

pub fn extract_initial_params_pythonize(initial_params: &Bound<PyDict>, verbose: bool) -> PyResult<HashMap<String, HashMap<String, f64>>> {
    if verbose {
        println!("  Using pythonize to extract initial parameters ({} components)", initial_params.len());
    }
    
    let result: HashMap<String, HashMap<String, String>> = convert_from_python(initial_params.as_any())?;
    
    // Convert string values to f64
    let mut converted_result = HashMap::new();
    for (component_name, params) in result {
        let mut converted_params = HashMap::new();
        for (param_name, param_str) in params {
            let param_val = param_str.parse::<f64>()
                .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Could not parse '{}' as float for {}:{}", param_str, component_name, param_name)
                ))?;
            converted_params.insert(param_name, param_val);
        }
        converted_result.insert(component_name, converted_params);
    }
    
    if verbose {
        let total_params: usize = converted_result.values().map(|p| p.len()).sum();
        println!("  ✓ Extracted {} components with {} total parameters", converted_result.len(), total_params);
    }
    
    Ok(converted_result)
}

pub fn extract_bounds_pythonize(bounds_list: &Bound<PyList>, verbose: bool) -> PyResult<Vec<ParsedBound>> {
    if verbose {
        println!("  Using pythonize to extract {} bounds", bounds_list.len());
    }
    
    let bounds: Vec<ParsedBound> = convert_from_python(bounds_list.as_any())?;
    
    if verbose {
        for (i, bound) in bounds.iter().enumerate() {
            println!("    Bound {}: {}[{}] ∈ [{:.3}, {:.3}]", 
                     i, bound.component, bound.parameter, bound.min_value, bound.max_value);
        }
        println!("  ✓ Extracted {} bounds", bounds.len());
    }
    
    Ok(bounds)
}

pub fn extract_targets_pythonize(targets_list: &Bound<PyList>, verbose: bool) -> PyResult<Vec<ParsedTarget>> {
    if verbose {
        println!("  Using pythonize to extract {} targets", targets_list.len());
    }
    
    let targets: Vec<ParsedTarget> = convert_from_python(targets_list.as_any())?;
    
    if verbose {
        for (i, target) in targets.iter().enumerate() {
            println!("    Target {}: {} = {:.6e} (weight: {}, type: {})", 
                     i, target.metric, target.target_value, target.weight, target.constraint_type);
        }
        println!("  ✓ Extracted {} targets", targets.len());
    }
    
    Ok(targets)
}

pub fn extract_tests_pythonize(tests_dict: &Bound<PyDict>, verbose: bool) -> PyResult<HashMap<String, TestConfiguration>> {
    if verbose {
        println!("  Using pythonize to extract {} tests", tests_dict.len());
    }
    
    // First convert to a generic structure
    let raw_tests: HashMap<String, HashMap<String, String>> = convert_from_python(tests_dict.as_any())?;
    
    // Then restructure into TestConfiguration format
    let mut tests = HashMap::new();
    for (test_name, test_data) in raw_tests {
        let mut component_values = HashMap::new();
        let mut spice_code = None;
        
        for (key, value) in test_data {
            if key == "spice" {
                spice_code = Some(value);
            } else {
                component_values.insert(key, value);
            }
        }
        
        if verbose {
            println!("    Test '{}': {} components, {} chars of SPICE", 
                     test_name, component_values.len(), 
                     spice_code.as_ref().map(|s| s.len()).unwrap_or(0));
        }
        
        tests.insert(test_name, TestConfiguration {
            component_values,
            spice: spice_code,
        });
    }
    
    if verbose {
        println!("  ✓ Extracted {} tests", tests.len());
    }
    
    Ok(tests)
}

/// Convert optimization results back to Python format
pub fn convert_results_to_python(
    py: Python,
    results: &HashMap<String, HashMap<String, f64>>
) -> PyResult<Py<PyAny>> {
    // Convert f64 values to formatted strings to match original format
    let formatted_results: HashMap<String, HashMap<String, String>> = results
        .iter()
        .map(|(component, params)| {
            let formatted_params = params
                .iter()
                .map(|(param, value)| (param.clone(), format!("{:.6}", value)))
                .collect();
            (component.clone(), formatted_params)
        })
        .collect();
    
    convert_to_python(py, &formatted_results)
}

/// Helper function to extract optional values with defaults
pub fn extract_optional_with_default<T>(
    py_obj: Option<&Bound<PyAny>>,
    default: T
) -> PyResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    match py_obj {
        Some(obj) => convert_from_python(obj),
        None => Ok(default),
    }
}

/// Convenience function for the main optimize_circuit function
pub fn extract_all_parameters(
    initial_params: &Bound<PyDict>,
    tests: &Bound<PyDict>,
    targets: &Bound<PyList>,
    bounds: &Bound<PyList>,
    max_iterations: Option<&Bound<PyAny>>,
    target_precision: Option<&Bound<PyAny>>,
    template_dir: Option<&Bound<PyAny>>,
    verbose: Option<&Bound<PyAny>>,
) -> PyResult<(
    HashMap<String, HashMap<String, f64>>,
    HashMap<String, TestConfiguration>,
    Vec<ParsedTarget>,
    Vec<ParsedBound>,
    usize,
    f64,
    String,
    bool,
)> {
    let verbose_flag = extract_optional_with_default(verbose, false)?;
    
    let parsed_initial_params = extract_initial_params_pythonize(initial_params, verbose_flag)?;
    let parsed_tests = extract_tests_pythonize(tests, verbose_flag)?;
    let parsed_targets = extract_targets_pythonize(targets, verbose_flag)?;
    let parsed_bounds = extract_bounds_pythonize(bounds, verbose_flag)?;
    
    let max_iter = extract_optional_with_default(max_iterations, 100usize)?;
    let precision = extract_optional_with_default(target_precision, 0.9f64)?;
    let template_dir_str = extract_optional_with_default(template_dir, "template".to_string())?;
    
    Ok((
        parsed_initial_params,
        parsed_tests,
        parsed_targets,
        parsed_bounds,
        max_iter,
        precision,
        template_dir_str,
        verbose_flag,
    ))
}
