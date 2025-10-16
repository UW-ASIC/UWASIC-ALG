use super::problem::CircuitProblem;
use super::solvers::traits::OptimizationCallback;
use crate::core::{Target, TargetMode};
use pyo3::Python;

/// Iteration result for tracking optimization progress
#[derive(Debug, Clone)]
pub struct IterationResult {
    pub params: Vec<f64>,
    pub cost: f64,
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
