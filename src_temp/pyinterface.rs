use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pythonize::{depythonize, pythonize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Test Configuration structures
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
}

// Alias for backward compatibility
pub type ParsedTestConfiguration = TestConfiguration;

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
pub struct ParsedBound {
    pub component: String,
    pub parameter: String,
    pub min_value: f64,
    pub max_value: f64,
}

pub struct PyInterface;

impl PyInterface {
    /// Generic conversion function from Python object to Rust type using serde
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
        pythonize(py, value)
            .map(|bound| bound.unbind())
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!("Conversion error: {}", e))
            })
    }

    /// Extract optional parameter with default value
    pub fn extract_optional_with_default<T>(
        opt_param: Option<&Bound<PyAny>>,
        default: T,
    ) -> PyResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        match opt_param {
            Some(param) => Self::convert_from_python(param),
            None => Ok(default),
        }
    }

    /// Main conversion function with proper error handling and generic conversions
    pub fn convert_types(
        py: Python,
        initial_params: &Bound<PyDict>,
        tests: &Bound<PyDict>,
        targets: &Bound<PyList>,
        bounds: &Bound<PyList>,
        template_dir: Option<&Bound<PyAny>>,
        max_iterations: Option<&Bound<PyAny>>,
        target_precision: Option<&Bound<PyAny>>,
        solver_type: Option<&Bound<PyAny>>,
        verbose: Option<&Bound<PyAny>>,
    ) -> PyResult<(
        HashMap<String, HashMap<String, f64>>,
        HashMap<String, ParsedTestConfiguration>,
        Vec<ParsedTarget>,
        Vec<ParsedBound>,
        usize,
        f64,
        String,
        String,
        bool,
    )> {
        // Extract optional parameters with defaults using generic function
        let verbose_flag = Self::extract_optional_with_default(verbose, false)?;
        let max_iter = Self::extract_optional_with_default(max_iterations, 100usize)?;
        let precision = Self::extract_optional_with_default(target_precision, 0.9f64)?;
        let template_dir_str = Self::extract_optional_with_default(
            template_dir, 
            "template".to_string()
        )?;
        let solver_type_str = Self::extract_optional_with_default(
            solver_type, 
            "auto".to_string()
        )?;

        // Convert required parameters using generic conversion
        let parsed_initial_params: HashMap<String, HashMap<String, f64>> = 
            Self::convert_from_python(initial_params.as_any())?;
        
        let parsed_tests: HashMap<String, ParsedTestConfiguration> = 
            Self::convert_from_python(tests.as_any())?;
        
        let parsed_targets: Vec<ParsedTarget> = 
            Self::convert_from_python(targets.as_any())?;
        
        let parsed_bounds: Vec<ParsedBound> = 
            Self::convert_from_python(bounds.as_any())?;

        Ok((
            parsed_initial_params,
            parsed_tests,
            parsed_targets,
            parsed_bounds,
            max_iter,
            precision,
            template_dir_str,
            solver_type_str,
            verbose_flag,
        ))
    }

    /// Convert ParsedTarget to optimizer TargetMetric
    pub fn convert_to_target_metric(parsed_target: &ParsedTarget, spice_code: &str) -> crate::optimizer::TargetMetric {
        crate::optimizer::TargetMetric::new(
            &parsed_target.metric,
            parsed_target.target_value,
            spice_code,
        ).with_weight(parsed_target.weight)
    }
}
