use super::utils::NGSPICE_OUTPUT;
use crate::ngspice::NgSpice;
use crate::optimizer::solver::traits::{OptimizationCallback, Problem};
use crate::types::*;
use pyo3::Python;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Round parameter value to Sky130 precision constraints
/// Sky130 has discrete grid-based sizing:
/// - Width/Length: Must be multiples of 0.005µm (5nm grid)
/// - Minimum values depend on device type but grid is consistent
fn round_to_sky130_precision(value: f64) -> f64 {
    const GRID_SIZE: f64 = 0.005e-6; // 5nm grid in meters

    // Round to nearest grid point
    let grid_units = (value / GRID_SIZE).round();
    grid_units * GRID_SIZE
}

/// Internal constraint data with compiled expressions
struct ConstraintData {
    target_idx: usize,
    source_indices: Vec<usize>,
    relationship: RelationshipType,
    compiled: Option<crate::expression::CompiledExpression>,
}

/// Iteration result for tracking optimization progress
#[derive(Debug, Clone)]
pub struct IterationResult {
    pub params: Vec<f64>,
    pub cost: f64,
}

/// Circuit problem encapsulating simulation, parameters, and constraints
pub struct CircuitProblem {
    // Parameter data
    params: Vec<f64>,
    bounds: Vec<(f64, f64)>,
    param_names: Vec<String>,

    // Constraints
    constraints: Vec<ConstraintData>,

    // Simulation context
    ngspice: RefCell<NgSpice>,
    tests: Vec<Test>,
    targets: Vec<Target>,

    // Temporary netlist file (for alterparam + reset workflow)
    temp_netlist_path: PathBuf,
}

impl CircuitProblem {
    /// Create new circuit problem from parameters, constraints, and netlist
    pub fn new(
        parameters: Vec<Parameter>,
        constraints: Vec<ParameterConstraint>,
        ngspice: NgSpice,
        tests: Vec<Test>,
        targets: Vec<Target>,
        netlist_lines: Vec<String>,
        verbose: bool,
    ) -> Result<Self, String> {
        let params: Vec<f64> = parameters.iter().map(|p| p.value).collect();
        let bounds: Vec<(f64, f64)> = parameters.iter().map(|p| (p.min_val, p.max_val)).collect();
        let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();

        // Build constraint data
        let constraint_data = Self::build_constraints(&parameters, constraints)?;

        // Process environment variables in test spice_code
        let processed_tests = Self::process_test_environments(&tests, verbose)?;

        // Parameterize netlist (silently)
        let mut modified_netlist = Self::parameterize_netlist(&netlist_lines, &parameters, false)?;

        // Remove .end, add analysis directives, add .end back
        Self::add_analysis_directives(&mut modified_netlist, &tests);

        // Write to temporary file and load into NgSpice
        let temp_netlist_path = Self::write_and_load_netlist(&modified_netlist, &ngspice, false)?;

        if verbose {
            println!("✓ Circuit loaded successfully");
            println!("  Parameters: {}", param_names.len());
            println!("  Constraints: {}", constraint_data.len());
            println!("  Tests: {}", tests.len());
            println!("  Targets: {}", targets.len());
        }

        Ok(Self {
            params,
            bounds,
            param_names,
            constraints: constraint_data,
            ngspice: RefCell::new(ngspice),
            tests: processed_tests,
            targets,
            temp_netlist_path,
        })
    }

    /// Get the last NgSpice output (useful for debugging)
    pub fn get_ngspice_output(&self) -> Result<Vec<String>, String> {
        let output = NGSPICE_OUTPUT
            .lock()
            .map_err(|e| format!("Failed to lock output: {}", e))?;
        Ok(output.clone())
    }

    /// Print last NgSpice output (for debugging)
    pub fn print_ngspice_output(&self) {
        if let Ok(output) = self.get_ngspice_output() {
            println!("\n=== NgSpice Output ===");
            for line in output {
                println!("{}", line);
            }
            println!("======================\n");
        }
    }

    /// Process test environments by substituting environment variables in spice_code
    fn process_test_environments(tests: &[Test], verbose: bool) -> Result<Vec<Test>, String> {
        let mut processed_tests = Vec::with_capacity(tests.len());

        for test in tests {
            let mut processed_code = test.spice_code.clone();

            // Replace environment variable placeholders with actual values
            for env in &test.environment {
                let placeholder = format!("{{{}}}", env.name);
                processed_code = processed_code.replace(&placeholder, &env.value);
            }

            if verbose && !test.environment.is_empty() {
                println!("  Test '{}' environments:", test.name);
                for env in &test.environment {
                    println!("    {} = {}", env.name, env.value);
                }
            }

            processed_tests.push(Test {
                name: test.name.clone(),
                spice_code: processed_code,
                description: test.description.clone(),
                environment: test.environment.clone(),
            });
        }

        Ok(processed_tests)
    }

    /// Build constraint data from parameters and constraint definitions
    fn build_constraints(
        parameters: &[Parameter],
        constraints: Vec<ParameterConstraint>,
    ) -> Result<Vec<ConstraintData>, String> {
        let mut constraint_data = Vec::with_capacity(constraints.len());

        for constraint in constraints {
            let target_idx = parameters
                .iter()
                .position(|p| p.name == constraint.target_param.name)
                .ok_or_else(|| {
                    format!("Target param '{}' not found", constraint.target_param.name)
                })?;

            let source_indices: Vec<usize> = constraint
                .source_params
                .iter()
                .filter_map(|sp| parameters.iter().position(|p| p.name == sp.name))
                .collect();

            if source_indices.len() != constraint.source_params.len() {
                return Err("Source parameter not found".into());
            }

            constraint_data.push(ConstraintData {
                target_idx,
                source_indices,
                relationship: constraint.relationship,
                compiled: constraint.compiled.clone(),
            });
        }

        Ok(constraint_data)
    }

    /// Parameterize netlist by injecting .param directives and replacing component values
    fn parameterize_netlist(
        netlist_lines: &[String],
        parameters: &[Parameter],
        verbose: bool,
    ) -> Result<Vec<String>, String> {
        let mut result = Vec::new();

        // Preserve title line
        if let Some(first_line) = netlist_lines.first() {
            if !first_line.trim().starts_with('.') {
                result.push(first_line.clone());
            }
        }

        // Add parameter definitions at top
        result.push("".to_string());
        result.push("* === Optimization Parameters (Auto-generated) ===".to_string());
        for param in parameters {
            let param_line = format!(".param {} = {}", param.name, param.value);
            result.push(param_line);
        }
        result.push("* === End Parameters ===".to_string());
        result.push("".to_string());

        // Build component->parameter mapping
        let component_params = Self::build_component_param_map(parameters);

        // Process netlist lines
        let start_idx = if netlist_lines
            .first()
            .map(|l| !l.trim().starts_with('.'))
            .unwrap_or(false)
        {
            1
        } else {
            0
        };

        for line in &netlist_lines[start_idx..] {
            let trimmed = line.trim();

            // Skip existing .param lines
            if trimmed.starts_with(".param") {
                continue;
            }

            // Parameterize component lines (X* or M*)
            if trimmed.starts_with('X') || trimmed.starts_with('M') {
                let comp_name = trimmed.split_whitespace().next().unwrap_or("");
                if let Some(params) = component_params.get(comp_name) {
                    result.push(Self::parameterize_component_line(line, params));
                    continue;
                }
            }

            result.push(line.clone());
        }

        Ok(result)
    }

    /// Build mapping from component names to their parameters
    fn build_component_param_map(
        parameters: &[Parameter],
    ) -> HashMap<String, Vec<(String, String)>> {
        let mut component_params: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for param in parameters {
            if let Some(underscore_pos) = param.name.rfind('_') {
                let component = param.name[..underscore_pos].to_string();
                let param_type = param.name[underscore_pos + 1..].to_string();
                component_params
                    .entry(component)
                    .or_insert_with(Vec::new)
                    .push((param_type, param.name.clone()));
            }
        }

        component_params
    }

    /// Parameterize a single component line by replacing values with {param} references
    fn parameterize_component_line(line: &str, params: &[(String, String)]) -> String {
        let mut modified = line.to_string();

        for (ptype, pname) in params {
            let pattern = format!(" {}=", ptype);
            if let Some(pos) = modified.find(&pattern) {
                let val_start = pos + pattern.len();
                let remaining = &modified[val_start..];
                let val_end = remaining
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(remaining.len());

                // Replace value with {parameter_name}
                modified = format!(
                    "{}={{{}}}{}",
                    &modified[..pos + pattern.len() - 1],
                    pname,
                    &modified[val_start + val_end..]
                );
            }
        }

        modified
    }

    /// Add analysis directives from tests to netlist
    fn add_analysis_directives(netlist: &mut Vec<String>, tests: &[Test]) {
        // Remove .end temporarily
        if let Some(end_pos) = netlist.iter().position(|l| l.trim() == ".end") {
            netlist.remove(end_pos);
        }

        // Add analysis directives (.ac, .dc, .tran, .op)
        for test in tests {
            for line in test.spice_code.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with(".ac ")
                    || trimmed.starts_with(".dc ")
                    || trimmed.starts_with(".tran ")
                    || trimmed.starts_with(".op")
                {
                    netlist.push(trimmed.to_string());
                }
            }
        }

        // Add .end back
        netlist.push(".end".to_string());
    }

    /// Write netlist to temporary file and load into NgSpice
    fn write_and_load_netlist(
        netlist: &[String],
        ngspice: &NgSpice,
        verbose: bool,
    ) -> Result<PathBuf, String> {
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("ngspice_opt_{}.spice", std::process::id()));

        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        for line in netlist {
            writeln!(file, "{}", line).map_err(|e| format!("Failed to write: {}", e))?;
        }

        // Load circuit using 'source' command (required for alterparam + reset)
        let source_cmd = format!("source {}", temp_path.display());
        ngspice
            .command(&source_cmd)
            .map_err(|e| format!("Failed to source circuit: {}", e))?;

        Ok(temp_path)
    }

    /// Update circuit parameters using alterparam + reset + run
    pub fn update_parameters(&self, params: &[f64]) -> Result<(), String> {
        let ngspice = self.ngspice.borrow();

        // Clear previous output
        NGSPICE_OUTPUT
            .lock()
            .map_err(|e| format!("Failed to lock output: {}", e))?
            .clear();

        // Use alterparam on the actual parameter names (lowercase)
        for (param_name, &value) in self.param_names.iter().zip(params.iter()) {
            let param_lower = param_name.to_lowercase();
            let cmd = format!("alterparam {} = {}", param_lower, value);
            ngspice
                .command(&cmd)
                .map_err(|e| format!("Failed to execute '{}': {}", cmd, e))?;
        }

        // Reset to clear previous simulation data
        ngspice
            .command("reset")
            .map_err(|e| format!("Failed to execute 'reset': {}", e))?;

        // Run simulation with new parameter values
        ngspice
            .command("run")
            .map_err(|e| format!("Failed to execute 'run': {}", e))?;

        Ok(())
    }

    /// Execute test measurements with proper environment isolation
    pub fn execute_measurements(&self) -> Result<(), String> {
        let ngspice = self.ngspice.borrow();

        for test in &self.tests {
            // Apply environment settings before running test
            for env in &test.environment {
                let env_cmd = Self::environment_to_ngspice_command(&env.name, &env.value);
                if !env_cmd.is_empty() {
                    ngspice.command(&env_cmd).map_err(|e| {
                        format!(
                            "Failed to set environment '{}={}' in test '{}': {} (command: '{}')",
                            env.name, env.value, test.name, e, env_cmd
                        )
                    })?;
                }
            }

            // Execute test measurements
            for line in test.spice_code.lines() {
                let trimmed = line.trim();
                // Execute measurement commands (skip analysis directives, they're in netlist)
                if !trimmed.is_empty()
                    && !trimmed.starts_with('*')
                    && trimmed != ".control"
                    && trimmed != ".endc"
                    && trimmed != "run"
                    && !trimmed.starts_with(".ac ")
                    && !trimmed.starts_with(".dc ")
                    && !trimmed.starts_with(".tran ")
                    && !trimmed.starts_with(".op")
                {
                    ngspice.command(trimmed).map_err(|e| {
                        format!(
                            "Failed to execute command '{}' in test '{}': {}",
                            trimmed, test.name, e
                        )
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Convert environment variable to appropriate NgSpice command
    fn environment_to_ngspice_command(name: &str, value: &str) -> String {
        match name.to_lowercase().as_str() {
            "temp" | "temperature" => {
                // Set temperature (in Celsius)
                format!("set temp = {}", value)
            }
            "vdd" | "vcc" | "vss" => {
                // Set power supply voltage using alterparam
                format!("alterparam {} = {}", name.to_lowercase(), value)
            }
            _ => {
                // Generic parameter setting
                format!("alterparam {} = {}", name.to_lowercase(), value)
            }
        }
    }

    /// Extract metrics from NgSpice output
    pub fn extract_metrics(&self) -> Result<HashMap<String, f64>, String> {
        let mut metrics = HashMap::new();

        let output = NGSPICE_OUTPUT
            .lock()
            .map_err(|e| format!("Failed to lock output: {}", e))?;

        for target in &self.targets {
            let meas_name = format!("{}_val", target.metric.to_lowercase());

            // Parse output for measurement (format: "stdout dc_gain_val = -4.420978e+01 at= 1.000000e+00")
            // Search for LAST occurrence to get most recent measurement
            let mut last_value: Option<f64> = None;

            for line in output.iter() {
                let trimmed = line.trim();
                if trimmed.contains(&meas_name) && trimmed.contains('=') {
                    if let Some(eq_pos) = trimmed.find('=') {
                        let after_eq = &trimmed[eq_pos + 1..];
                        let value_str = after_eq.trim().split_whitespace().next().unwrap_or("");
                        if let Ok(value) = value_str.parse::<f64>() {
                            last_value = Some(value);
                        }
                    }
                }
            }

            if let Some(value) = last_value {
                metrics.insert(target.metric.clone(), value);
            } else {
                // Use penalty value if measurement not found/failed
                let penalty_value = match target.mode {
                    TargetMode::Min => target.value * 0.1,
                    TargetMode::Max => target.value * 10.0,
                    TargetMode::Target => target.value * 2.0,
                };
                metrics.insert(target.metric.clone(), penalty_value);
            }
        }

        drop(output);
        Ok(metrics)
    }

    /// Compute cost from metrics and targets
    pub fn compute_cost_from_metrics(&self, metrics: &HashMap<String, f64>) -> f64 {
        let mut total_cost = 0.0;

        for target in &self.targets {
            let metric_value = metrics.get(&target.metric).unwrap_or(&0.0);

            let error = match target.mode {
                TargetMode::Min => {
                    if *metric_value < target.value {
                        (target.value - metric_value).abs()
                    } else {
                        0.0
                    }
                }
                TargetMode::Max => {
                    if *metric_value > target.value {
                        (metric_value - target.value).abs()
                    } else {
                        0.0
                    }
                }
                TargetMode::Target => (metric_value - target.value).abs(),
            };

            let weighted_error = error * target.weight;
            total_cost += weighted_error;
        }

        total_cost
    }

    /// Get targets (for callback access)
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Get parameter names (for callback access)
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }
}

// Implement Problem trait - simplified interface
impl Problem for CircuitProblem {
    fn cost(&self, params: &[f64]) -> Result<f64, String> {
        let rounded_params: Vec<f64> = params
            .iter()
            .map(|&p| round_to_sky130_precision(p))
            .collect();

        self.update_parameters(params)
            .map_err(|e| format!("Cost evaluation failed during parameter update: {}", e))?;

        self.execute_measurements()
            .map_err(|e| format!("Cost evaluation failed during measurements: {}", e))?;

        let metrics = self
            .extract_metrics()
            .map_err(|e| format!("Cost evaluation failed during metric extraction: {}", e))?;

        Ok(self.compute_cost_from_metrics(&metrics))
    }

    fn num_params(&self) -> usize {
        self.params.len()
    }

    fn initial_params(&self) -> &[f64] {
        &self.params
    }

    fn bounds(&self) -> &[(f64, f64)] {
        &self.bounds
    }

    fn apply_constraints(&self, params: &mut [f64]) -> Result<(), String> {
        for constraint in &self.constraints {
            let source_values: Vec<f64> = constraint
                .source_indices
                .iter()
                .map(|&idx| params[idx])
                .collect();

            let computed = if let Some(ref expr) = constraint.compiled {
                expr.evaluate(&source_values).map_err(|e| e.to_string())?
            } else {
                return Err("Constraint not compiled".into());
            };

            let target_idx = constraint.target_idx;
            let (min, max) = self.bounds[target_idx];

            match constraint.relationship {
                RelationshipType::Equals => {
                    params[target_idx] = computed.clamp(min, max);
                }
                RelationshipType::GreaterThanOrEqual => {
                    if params[target_idx] < computed {
                        params[target_idx] = computed.clamp(min, max);
                    }
                }
                RelationshipType::LessThanOrEqual => {
                    if params[target_idx] > computed {
                        params[target_idx] = computed.clamp(min, max);
                    }
                }
                RelationshipType::GreaterThan => {
                    if params[target_idx] <= computed {
                        params[target_idx] = (computed + 1e-6).clamp(min, max);
                    }
                }
                RelationshipType::LessThan => {
                    if params[target_idx] >= computed {
                        params[target_idx] = (computed - 1e-6).clamp(min, max);
                    }
                }
            }
        }

        // **ROUND ALL PARAMS AFTER CONSTRAINT APPLICATION**
        for param in params.iter_mut() {
            *param = round_to_sky130_precision(*param);
        }

        Ok(())
    }
}

impl Drop for CircuitProblem {
    fn drop(&mut self) {
        if self.temp_netlist_path.exists() {
            let _ = std::fs::remove_file(&self.temp_netlist_path);
        }
    }
}

// ============================================================================
// CALLBACK IMPLEMENTATION FOR CIRCUIT OPTIMIZATION
// ============================================================================

/// Callback for tracking and displaying circuit optimization progress
pub struct CircuitOptimizationCallback {
    verbose: bool,
    max_iterations: u32,
    iteration_count: u32,
    history: Vec<IterationResult>,
    targets: Vec<Target>,
    param_names: Vec<String>,
    // Raw pointer to problem for accessing metrics during display
    // This is safe because the problem outlives the callback
    problem: *const CircuitProblem,
}

impl CircuitOptimizationCallback {
    pub fn new(
        verbose: bool,
        max_iterations: u32,
        targets: Vec<Target>,
        param_names: Vec<String>,
        problem: &CircuitProblem,
    ) -> Self {
        Self {
            verbose,
            max_iterations,
            iteration_count: 0,
            history: Vec::new(),
            targets,
            param_names,
            problem: problem as *const _,
        }
    }

    /// Get iteration history
    pub fn history(&self) -> &[IterationResult] {
        &self.history
    }

    /// Print metrics comparison for current iteration
    fn print_iteration(&self, iteration: u32, params: &[f64], cost: f64) -> Result<(), String> {
        if !self.verbose {
            return Ok(());
        }

        println!("\nIter {:4}: Cost = {:.6e}", iteration, cost);

        // Get metrics by running simulation
        // Safety: problem pointer is valid for the lifetime of this callback
        unsafe {
            let problem = &*self.problem;
            problem.update_parameters(params)?;
            problem.execute_measurements()?;
            let metrics = problem.extract_metrics()?;

            for target in &self.targets {
                let current = metrics.get(&target.metric).unwrap_or(&0.0);
                let mode_str = match target.mode {
                    TargetMode::Min => "≤",
                    TargetMode::Max => "≥",
                    TargetMode::Target => "=",
                };
                println!(
                    "  {:<20} Target: {:>12.6e} {} Current: {:>12.6e}",
                    target.metric, target.value, mode_str, current
                );
            }
        }

        Ok(())
    }

    /// Print optimization summary
    pub fn print_summary(&self, success: bool, stop_reason: &str) {
        println!("\n{}", "=".repeat(80));
        println!("OPTIMIZATION SUMMARY");
        println!("{}", "=".repeat(80));

        println!(
            "\nStatus: {}",
            if success { "✓ SUCCESS" } else { "✗ FAILED" }
        );
        println!("Stop Reason: {}", stop_reason);
        println!("Total Iterations: {}", self.history.len());

        if let Some(final_result) = self.history.last() {
            println!("\nFinal Cost: {:.6e}", final_result.cost);
            println!("\nOptimal Parameters:");
            for (name, &value) in self.param_names.iter().zip(final_result.params.iter()) {
                println!("  {} = {:.6e}", name, value);
            }
        }

        println!("\nIteration History:");
        println!("{:<8} {:<20}", "Iter", "Cost");
        println!("{}", "-".repeat(30));
        for (i, result) in self.history.iter().enumerate() {
            println!("{:<8} {:<20.6e}", i + 1, result.cost);
        }

        println!("\n{}\n", "=".repeat(80));
    }
}

impl OptimizationCallback for CircuitOptimizationCallback {
    fn on_iteration(&mut self, iteration: u32, params: &[f64], cost: f64) -> Result<(), String> {
        Python::with_gil(|py| {
            if py.check_signals().is_err() {
                return Err("Interrupted by user (Ctrl+C)".to_string());
            }
            Ok(())
        })?;

        self.iteration_count = iteration;

        // Record iteration
        self.history.push(IterationResult {
            params: params.to_vec(),
            cost,
        });

        // Print if verbose
        if let Err(e) = self.print_iteration(iteration, params, cost) {
            eprintln!("Warning: Failed to print iteration {}: {}", iteration, e);
            // Print NgSpice output for debugging
            unsafe {
                let problem = &*self.problem;
                problem.print_ngspice_output();
            }
            return Err(e);
        }

        Ok(())
    }

    fn should_stop(&self) -> bool {
        self.iteration_count >= self.max_iterations
    }
}
