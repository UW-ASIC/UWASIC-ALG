use crate::core::*;
use crate::optimization::solvers::traits::{OptimizationCallback, Problem};
use crate::optimizer::NGSPICE_OUTPUT;
use crate::simulation::NgSpice;
use pyo3::Python;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

const VERBOSITY_FULL: bool = false;

/// Sky130 grid size: 5nm (0.005µm) precision
const SKY130_GRID_SIZE: f64 = 0.005e-6;
const SKY130_GRID_INV: f64 = 1.0 / SKY130_GRID_SIZE;

/// Format duration in seconds to human-readable string (e.g., "2m 30s", "1h 15m")
fn format_duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        let mins = (secs / 60.0).floor();
        let secs_remaining = secs % 60.0;
        format!("{}m {:.0}s", mins, secs_remaining)
    } else {
        let hours = (secs / 3600.0).floor();
        let mins_remaining = ((secs % 3600.0) / 60.0).floor();
        format!("{}h {}m", hours, mins_remaining)
    }
}

struct ConstraintData {
    relationship: RelationshipType,
    target_idx: usize,
    source_indices: Vec<usize>,
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
    params: Vec<f64>,
    bounds: Vec<(f64, f64)>,
    constraints: Vec<ConstraintData>,
    targets: Vec<Target>,

    pub ngspice: RefCell<NgSpice>,
    tests: Vec<Test>,

    param_names: Vec<String>,
    temp_netlist_path: PathBuf,
    verbose: bool,

    constraint_cache: RefCell<Option<(u64, Vec<f64>)>>,
}

impl CircuitProblem {
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
                relationship: constraint.relationship,
                target_idx,
                source_indices,
                compiled: constraint.compiled.clone(),
            });
        }

        // Merge tests with identical environments AND analysis types to reduce simulation overhead
        let processed_tests = Self::merge_tests_by_environment(&tests, verbose)?;

        // Parameterize netlist
        let mut modified_netlist = Vec::new();
        // Preserve title line
        if let Some(first_line) = netlist_lines.first() {
            if !first_line.trim().starts_with('.') {
                modified_netlist.push(first_line.clone());
            }
        }
        // Add parameter definitions at top
        modified_netlist.push("".to_string());
        modified_netlist.push("* === Optimization Parameters (Auto-generated) ===".to_string());
        for param in &parameters {
            let param_line = format!(".param {} = {}", param.name, param.value);
            modified_netlist.push(param_line);
        }
        modified_netlist.push("* === End Parameters ===".to_string());
        modified_netlist.push("".to_string());

        // Build component->parameter mapping
        let mut component_params: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for param in &parameters {
            if let Some(underscore_pos) = param.name.rfind('_') {
                let component = param.name[..underscore_pos].to_string();
                let param_type = param.name[underscore_pos + 1..].to_string();
                component_params
                    .entry(component)
                    .or_insert_with(Vec::new)
                    .push((param_type, param.name.clone()));
            }
        }

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
                    let mut modified_line = line.to_string();
                    for (ptype, pname) in params {
                        let pattern = format!(" {}=", ptype);
                        if let Some(pos) = modified_line.find(&pattern) {
                            let val_start = pos + pattern.len();
                            let remaining = &modified_line[val_start..];
                            let val_end = remaining
                                .find(|c: char| c.is_whitespace())
                                .unwrap_or(remaining.len());
                            modified_line = format!(
                                "{}={{{}}}{}",
                                &modified_line[..pos + pattern.len() - 1],
                                pname,
                                &modified_line[val_start + val_end..]
                            );
                        }
                    }
                    modified_netlist.push(modified_line);
                    continue;
                }
            }
            modified_netlist.push(line.clone());
        }

        // ensure .end is present
        if !modified_netlist.iter().any(|l| l.trim() == ".end") {
            modified_netlist.push(".end".to_string());
        }

        // Write to temporary file and load into NgSpice
        let temp_dir = std::env::temp_dir();
        let temp_netlist_path = temp_dir.join(format!("ngspice_opt_{}.spice", std::process::id()));
        let mut file = std::fs::File::create(&temp_netlist_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        for line in &modified_netlist {
            writeln!(file, "{}", line).map_err(|e| format!("Failed to write: {}", e))?;
        }
        let source_cmd = format!("source {}", temp_netlist_path.display());
        ngspice
            .command(&source_cmd)
            .map_err(|e| format!("Failed to source circuit: {}", e))?;

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
            verbose,
            constraint_cache: RefCell::new(None),
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

    /// Update circuit parameters using alterparam
    pub fn update_parameters(&self, params: &[f64]) -> Result<(), String> {
        // Clear previous output
        NGSPICE_OUTPUT
            .lock()
            .map_err(|e| format!("Failed to lock output: {}", e))?
            .clear();

        // Execute alterparam commands one by one
        let ngspice = self.ngspice.borrow();
        for (name, &value) in self.param_names.iter().zip(params.iter()) {
            let cmd = format!("alterparam {} = {}", name.to_lowercase(), value);
            ngspice
                .command(&cmd)
                .map_err(|e| format!("Failed to execute '{}': {}", cmd, e))?;
        }

        Ok(())
    }

    /// Execute test measurements
    pub fn execute_measurements(&self) -> Result<(), String> {
        let ngspice = self.ngspice.borrow();

        for test in &self.tests {
            // Apply environment settings
            for env in &test.environment {
                let env_cmd = match env.name.to_lowercase().as_str() {
                    "temp" | "temperature" => format!("set temp = {}", env.value),
                    _ => format!("alterparam {} = {}", env.name.to_lowercase(), env.value),
                };
                ngspice
                    .command(&env_cmd)
                    .map_err(|e| format!("Failed to set environment '{}': {}", env.name, e))?;
            }

            // Reset simulation state
            ngspice
                .command("reset")
                .map_err(|e| format!("Failed to reset: {}", e))?;

            // Run analysis (find and execute .ac, .dc, .tran, or .op directive)
            let analysis_line = test
                .spice_code
                .lines()
                .find(|line| {
                    let t = line.trim();
                    t.starts_with(".ac ")
                        || t.starts_with(".dc ")
                        || t.starts_with(".tran ")
                        || t.starts_with(".op")
                })
                .ok_or_else(|| format!("No analysis directive in test '{}'", test.name))?;

            ngspice
                .command(&analysis_line.trim()[1..]) // Remove leading '.'
                .map_err(|e| format!("Failed to run analysis in '{}': {}", test.name, e))?;

            // Execute measurement commands (skip directives, comments, control blocks)
            for line in test.spice_code.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty()
                    && !trimmed.starts_with('*')
                    && !trimmed.starts_with('.')
                    && trimmed != "run"
                {
                    ngspice
                        .command(trimmed)
                        .map_err(|e| format!("Failed to execute '{}': {}", trimmed, e))?;
                }
            }
        }

        Ok(())
    }

    /// Extract metrics from NgSpice output
    pub fn extract_metrics(&self) -> Result<HashMap<String, f64>, String> {
        let output = NGSPICE_OUTPUT
            .lock()
            .map_err(|e| format!("Failed to lock output: {}", e))?;

        // Parse measurement values (single pass, indexed by target)
        let mut metric_values: Vec<Option<f64>> = vec![None; self.targets.len()];

        for line in output.iter() {
            let trimmed = line.trim();
            if !trimmed.contains('=') {
                continue;
            }

            // Check each target (typically 1-5 targets, so linear scan is fine)
            for (i, target) in self.targets.iter().enumerate() {
                let metric_lower = target.metric.to_lowercase();
                if trimmed.contains(&metric_lower) && trimmed.contains("_val") {
                    if let Some(eq_pos) = trimmed.find('=') {
                        if let Some(value_str) =
                            trimmed[eq_pos + 1..].trim().split_whitespace().next()
                        {
                            if let Ok(value) = value_str.parse::<f64>() {
                                metric_values[i] = Some(value);
                            }
                        }
                    }
                }
            }
        }
        drop(output);

        // Build results map with penalties for missing metrics
        let mut metrics = HashMap::with_capacity(self.targets.len());
        for (i, target) in self.targets.iter().enumerate() {
            let value = metric_values[i].unwrap_or_else(|| {
                // Penalty for missing measurement
                match target.mode {
                    TargetMode::Min => target.value * 0.1,
                    TargetMode::Max => target.value * 10.0,
                    TargetMode::Target => target.value * 2.0,
                }
            });
            metrics.insert(target.metric.clone(), value);
        }

        Ok(metrics)
    }

    /// Get targets (for callback access)
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Get parameter names (for callback access)
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Evaluate all constraints (helper for apply_constraints)
    fn evaluate_all_constraints(&self, params: &[f64]) -> Result<Vec<f64>, String> {
        let mut results = Vec::with_capacity(self.constraints.len());
        let max_sources = self
            .constraints
            .iter()
            .map(|c| c.source_indices.len())
            .max()
            .unwrap_or(0);
        let mut source_values = Vec::with_capacity(max_sources);

        for constraint in &self.constraints {
            source_values.clear();
            source_values.extend(constraint.source_indices.iter().map(|&idx| params[idx]));

            let expr = constraint
                .compiled
                .as_ref()
                .ok_or("Constraint not compiled")?;
            let computed = expr.evaluate(&source_values).map_err(|e| e.to_string())?;
            results.push(computed);
        }

        Ok(results)
    }

    /// Merge tests with identical environments AND analysis types
    /// This properly handles that NgSpice needs separate runs for different analyses
    fn merge_tests_by_environment(tests: &[Test], verbose: bool) -> Result<Vec<Test>, String> {
        use std::collections::BTreeMap;

        // Group tests by (environment, analysis_type) signature
        let mut groups: BTreeMap<String, Vec<&Test>> = BTreeMap::new();

        for test in tests {
            // Create environment signature
            let mut env_sig: Vec<String> = test
                .environment
                .iter()
                .map(|e| format!("{}={}", e.name, e.value))
                .collect();
            env_sig.sort();
            let env_str = env_sig.join(";");

            // Extract analysis type from test
            let analysis_type = test
                .spice_code
                .lines()
                .find(|line| {
                    let t = line.trim();
                    t.starts_with(".ac ")
                        || t.starts_with(".dc ")
                        || t.starts_with(".tran ")
                        || t.starts_with(".op")
                })
                .map(|line| {
                    let t = line.trim();
                    if t.starts_with(".ac ") {
                        "ac"
                    } else if t.starts_with(".dc ") {
                        "dc"
                    } else if t.starts_with(".tran ") {
                        "tran"
                    } else {
                        "op"
                    }
                })
                .unwrap_or("none");

            // Group by environment + analysis type
            let key = format!("{}|{}", env_str, analysis_type);
            groups.entry(key).or_insert_with(Vec::new).push(test);
        }

        if verbose {
            println!(
                "✓ Test merging: {} tests → {} unique (env+analysis) groups",
                tests.len(),
                groups.len()
            );
        }

        // Process each group
        let mut merged_tests = Vec::new();
        for (key, group) in groups {
            // Process environment placeholders for all tests in group
            let mut processed_group: Vec<Test> = group
                .iter()
                .map(|test| {
                    let mut processed_code = test.spice_code.clone();
                    for env in &test.environment {
                        let placeholder = format!("{{{}}}", env.name);
                        processed_code = processed_code.replace(&placeholder, &env.value);
                    }
                    Test {
                        name: test.name.clone(),
                        spice_code: processed_code,
                        description: test.description.clone(),
                        environment: test.environment.clone(),
                    }
                })
                .collect();

            if processed_group.len() == 1 {
                // Single test, add as-is
                merged_tests.push(processed_group.into_iter().next().unwrap());
            } else {
                // Multiple tests: merge measurements (keep first analysis directive)
                let merged_name = processed_group
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join("+");

                // Take analysis from first test, combine all measurements
                let first_test = &processed_group[0];
                let analysis_line = first_test
                    .spice_code
                    .lines()
                    .find(|line| {
                        let t = line.trim();
                        t.starts_with(".ac ")
                            || t.starts_with(".dc ")
                            || t.starts_with(".tran ")
                            || t.starts_with(".op")
                    })
                    .unwrap_or("");

                let mut merged_code = String::new();
                merged_code.push_str(analysis_line);
                merged_code.push('\n');

                // Add all measurement commands from all tests
                for test in &processed_group {
                    for line in test.spice_code.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty()
                            && !trimmed.starts_with('*')
                            && !trimmed.starts_with('.')
                            && trimmed != "run"
                        {
                            merged_code.push_str(line);
                            merged_code.push('\n');
                        }
                    }
                }

                if verbose {
                    println!(
                        "  Merged {} tests: {} ({})",
                        processed_group.len(),
                        merged_name,
                        key
                    );
                }

                merged_tests.push(Test {
                    name: merged_name,
                    spice_code: merged_code,
                    description: format!(
                        "Merged from: {}",
                        processed_group
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    environment: first_test.environment.clone(),
                });
            }
        }

        Ok(merged_tests)
    }
}

// Implement Problem trait
impl Problem for CircuitProblem {
    fn cost(&self, params: &[f64]) -> Result<f64, String> {
        // Run simulation with updated parameters
        self.update_parameters(params)?;
        self.execute_measurements()?;
        let metrics = self.extract_metrics()?;

        // Compute weighted cost from all targets
        let total_cost = self
            .targets
            .iter()
            .filter_map(|target| {
                metrics.get(&target.metric).map(|&value| {
                    let error = match target.mode {
                        TargetMode::Min if value >= target.value => value - target.value,
                        TargetMode::Max if value <= target.value => target.value - value,
                        TargetMode::Target => (value - target.value).abs(),
                        _ => 0.0, // Target satisfied
                    };
                    error * target.weight
                })
            })
            .sum();

        Ok(total_cost)
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
        // Fast path: no constraints, just round to Sky130 grid
        if self.constraints.is_empty() {
            for param in params.iter_mut() {
                *param = (*param * SKY130_GRID_INV).round() * SKY130_GRID_SIZE;
            }
            return Ok(());
        }

        // Evaluate constraints (with caching)
        let constraint_results = {
            // Compute hash of source parameters only
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for (i, &param) in params.iter().enumerate() {
                if self
                    .constraints
                    .iter()
                    .any(|c| c.source_indices.contains(&i))
                {
                    std::hash::Hash::hash(&param.to_bits(), &mut hasher);
                }
            }
            let param_hash = std::hash::Hasher::finish(&hasher);

            // Check cache
            let mut cache = self.constraint_cache.borrow_mut();
            if let Some((cached_hash, cached_results)) = cache.take() {
                if cached_hash == param_hash {
                    cached_results // Cache hit - steal results
                } else {
                    // Cache miss: evaluate all constraints
                    let results = self.evaluate_all_constraints(params)?;
                    *cache = Some((param_hash, results.clone()));
                    results
                }
            } else {
                // No cache: evaluate all constraints
                let results = self.evaluate_all_constraints(params)?;
                *cache = Some((param_hash, results.clone()));
                results
            }
        };

        // Apply constraint results to parameters
        for (constraint, &computed) in self.constraints.iter().zip(constraint_results.iter()) {
            let target_idx = constraint.target_idx;
            let (min, max) = self.bounds[target_idx];
            let current = params[target_idx];

            params[target_idx] = match constraint.relationship {
                RelationshipType::Equals => computed,
                RelationshipType::GreaterThanOrEqual if current < computed => computed,
                RelationshipType::LessThanOrEqual if current > computed => computed,
                RelationshipType::GreaterThan if current <= computed => computed + 1e-6,
                RelationshipType::LessThan if current >= computed => computed - 1e-6,
                _ => current, // Constraint already satisfied
            }
            .clamp(min, max);
        }

        // Round all params to Sky130 grid
        for param in params.iter_mut() {
            *param = (*param * SKY130_GRID_INV).round() * SKY130_GRID_SIZE;
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
    start_time: std::time::Instant,
    last_iter_time: Option<std::time::Instant>,
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
            start_time: std::time::Instant::now(),
            last_iter_time: None,
        }
    }

    /// Get iteration history
    pub fn history(&self) -> &[IterationResult] {
        &self.history
    }

    /// Print metrics comparison for current iteration
    fn print_iteration(&mut self, iteration: u32, params: &[f64], cost: f64) -> Result<(), String> {
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
