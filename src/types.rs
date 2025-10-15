use crate::expression::CompiledExpression;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

// ===== ENUMS =====

#[pyclass(eq, eq_int)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetMode {
    Min,
    Max,
    Target,
}

#[pymethods]
impl TargetMode {
    fn __repr__(&self) -> &str {
        match self {
            Self::Min => "TargetMode.Min",
            Self::Max => "TargetMode.Max",
            Self::Target => "TargetMode.Target",
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipType {
    Equals,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

#[pymethods]
impl RelationshipType {
    fn __repr__(&self) -> &str {
        match self {
            Self::Equals => "RelationshipType.Equals",
            Self::GreaterThan => "RelationshipType.GreaterThan",
            Self::LessThan => "RelationshipType.LessThan",
            Self::GreaterThanOrEqual => "RelationshipType.GreaterThanOrEqual",
            Self::LessThanOrEqual => "RelationshipType.LessThanOrEqual",
        }
    }
}

// ===== CORE DATA TYPES =====

#[pyclass]
#[derive(Clone, Debug)]
pub struct Environment {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub value: String,
}

#[pymethods]
impl Environment {
    #[new]
    fn new(name: String, value: String) -> Self {
        Self { name, value }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct Parameter {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub value: f64,
    #[pyo3(get, set)]
    pub min_val: f64,
    #[pyo3(get, set)]
    pub max_val: f64,
}

#[pymethods]
impl Parameter {
    #[new]
    fn new(name: String, value: f64, min_val: f64, max_val: f64) -> Self {
        Self {
            name,
            value,
            min_val,
            max_val,
        }
    }

    pub fn clamp(&mut self) {
        self.value = self.value.clamp(self.min_val, self.max_val);
    }

    pub fn is_within_bounds(&self) -> bool {
        self.value >= self.min_val && self.value <= self.max_val
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct Target {
    #[pyo3(get, set)]
    pub metric: String,
    #[pyo3(get, set)]
    pub value: f64,
    #[pyo3(get, set)]
    pub weight: f64,
    #[pyo3(get, set)]
    pub mode: TargetMode,
    #[pyo3(get, set)]
    pub unit: String,
}

#[pymethods]
impl Target {
    #[new]
    fn new(metric: String, value: f64, weight: f64, mode: TargetMode, unit: String) -> Self {
        Self {
            metric,
            value,
            weight,
            mode,
            unit,
        }
    }

    pub fn compute_cost(&self, achieved: f64) -> f64 {
        let error = match self.mode {
            TargetMode::Min => {
                if achieved < self.value {
                    0.0
                } else {
                    achieved - self.value
                }
            }
            TargetMode::Max => {
                if achieved > self.value {
                    0.0
                } else {
                    self.value - achieved
                }
            }
            TargetMode::Target => (achieved - self.value).abs(),
        };
        error * self.weight
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct Test {
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub spice_code: String,
    #[pyo3(get, set)]
    pub description: String,
    #[pyo3(get)]
    pub environment: Vec<Environment>,
}

#[pymethods]
impl Test {
    #[new]
    fn new(
        name: String,
        environment: Vec<Environment>,
        spice_code: String,
        description: String,
    ) -> Self {
        Self {
            name,
            spice_code,
            description,
            environment,
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct ParameterConstraint {
    #[pyo3(get, set)]
    pub relationship: RelationshipType,
    #[pyo3(get, set)]
    pub description: String,
    #[pyo3(get, set)]
    pub expression: String,
    #[pyo3(get)]
    pub target_param: Parameter,
    #[pyo3(get)]
    pub source_params: Vec<Parameter>,
    // Internal: compiled expression (not exposed to Python, lazily compiled)
    #[pyo3(get, set)]
    pub compiled: Option<CompiledExpression>,
}

#[pymethods]
impl ParameterConstraint {
    #[new]
    fn new(
        target_param: Parameter,
        source_params: Vec<Parameter>,
        expression: String,
        relationship: RelationshipType,
        description: String,
    ) -> Self {
        // Don't compile yet - will be done during validation
        Self {
            relationship,
            description,
            expression,
            target_param,
            source_params,
            compiled: None,
        }
    }

    /// Evaluate the constraint expression with given parameter values
    /// Raises error if not compiled yet
    pub fn evaluate(&self, param_values: Vec<f64>) -> PyResult<f64> {
        match &self.compiled {
            Some(expr) => expr.eval(param_values),
            None => Err(PyValueError::new_err(
                "Constraint not compiled. Call Optimizer.validate_constraints() first.",
            )),
        }
    }

    /// Check if the constraint is satisfied
    pub fn is_satisfied(&self, param_values: Vec<f64>, tolerance: f64) -> PyResult<bool> {
        let computed_value = self.evaluate(param_values)?;
        let target_value = self.target_param.value;

        Ok(match self.relationship {
            RelationshipType::Equals => (computed_value - target_value).abs() <= tolerance,
            RelationshipType::GreaterThan => computed_value > target_value + tolerance,
            RelationshipType::LessThan => computed_value < target_value - tolerance,
            RelationshipType::GreaterThanOrEqual => computed_value >= target_value - tolerance,
            RelationshipType::LessThanOrEqual => computed_value <= target_value + tolerance,
        })
    }
}

impl ParameterConstraint {
    pub fn compile(&mut self, param_names: &[String]) -> Result<(), String> {
        if self.compiled.is_some() {
            return Ok(()); // Already compiled
        }

        // Verify all source params exist in param_names
        for src_param in &self.source_params {
            if !param_names.contains(&src_param.name) {
                return Err(format!(
                    "Source parameter '{}' not found in parameter list",
                    src_param.name
                ));
            }
        }

        // Get source param names in order
        let source_names: Vec<String> = self.source_params.iter().map(|p| p.name.clone()).collect();

        // Compile the expression
        match CompiledExpression::new(self.expression.clone(), source_names) {
            Ok(expr) => {
                self.compiled = Some(expr);
                Ok(())
            }
            Err(e) => Err(format!(
                "Failed to compile expression '{}' for constraint on '{}': {}",
                self.expression, self.target_param.name, e
            )),
        }
    }

    /// Internal accessor for compiled expression
    pub fn get_compiled(&self) -> Option<&CompiledExpression> {
        self.compiled.as_ref()
    }

    /// Internal evaluation (no Python overhead)
    pub fn evaluate_internal(&self, param_values: &[f64]) -> Result<f64, &'static str> {
        match &self.compiled {
            Some(expr) => expr.evaluate(param_values),
            None => Err("Constraint not compiled"),
        }
    }

    /// Get the index of target parameter in the parameter list
    pub fn find_target_index(&self, params: &[Parameter]) -> Option<usize> {
        params.iter().position(|p| p.name == self.target_param.name)
    }

    /// Get indices of source parameters in the parameter list
    pub fn find_source_indices(&self, params: &[Parameter]) -> Vec<usize> {
        self.source_params
            .iter()
            .filter_map(|sp| params.iter().position(|p| p.name == sp.name))
            .collect()
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct OptimizationResult {
    #[pyo3(get, set)]
    pub success: bool,
    #[pyo3(get, set)]
    pub cost: f64,
    #[pyo3(get, set)]
    pub iterations: u32,
    #[pyo3(get, set)]
    pub message: String,
    #[pyo3(get)]
    pub parameters: Vec<Parameter>,
}

#[pymethods]
impl OptimizationResult {
    #[new]
    fn new(
        success: bool,
        parameters: Vec<Parameter>,
        cost: f64,
        iterations: u32,
        message: String,
    ) -> Self {
        Self {
            success,
            cost,
            iterations,
            message,
            parameters,
        }
    }

    pub fn get_parameter(&self, name: &str) -> Option<Parameter> {
        self.parameters.iter().find(|p| p.name == name).cloned()
    }
}
