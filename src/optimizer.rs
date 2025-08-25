use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::ngspice::{run_spice, SimulationResult};

// NgSpice interface that uses the real ngspice functionality
pub struct NgSpiceInterface {
    pub timeout_secs: u64,
}

impl NgSpiceInterface {
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    pub fn run_simulation(&self, spice_content: &str, _simulator: &str) -> Result<SimulationResult, std::io::Error> {
        // Write SPICE content to a temporary file
        let temp_file = "/tmp/temp_simulation.spice";
        fs::write(temp_file, spice_content)?;
        
        // Run the actual ngspice simulation
        let result = run_spice(temp_file)?;
        
        // Clean up temporary file
        let _ = fs::remove_file(temp_file);
        
        Ok(result)
    }

    pub fn run_simulation_from_file<P: AsRef<Path>>(&self, spice_file_path: P) -> Result<SimulationResult, std::io::Error> {
        run_spice(spice_file_path)
    }
}

#[derive(Debug, Clone)]
pub struct SpiceRunConfig {
    pub expected_metrics: Vec<String>,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct ComponentParameter {
    pub component_name: String,
    pub parameter_name: String,
    pub min_value: f64,
    pub max_value: f64,
    pub current_value: f64,
}

pub struct OptimizationProblem {
    target_values: HashMap<String, f64>,
    weights: Vec<f64>,
    spice_runs: Vec<SpiceRunConfig>,
    component_parameters: Vec<ComponentParameter>,
    current_iteration: usize,
    previous_results: Vec<SimulationResult>,
}

impl OptimizationProblem {
    /// Create a new OptimizationProblem with SPICE runs and component parameters
    pub fn new(
        spice_runs: Vec<SpiceRunConfig>,
        component_parameters: Vec<ComponentParameter>,
    ) -> Self {
        let mut target_values = HashMap::new();
        let mut weights = Vec::new();
        
        // Build target values and weights from spice runs
        for (i, run_config) in spice_runs.iter().enumerate() {
            for metric in &run_config.expected_metrics {
                target_values.insert(format!("{}_{}", metric, i), 0.0); // Default target
            }
            weights.push(run_config.weight);
        }
        
        println!("=== Optimization Problem Created ===");
        println!("Parameters to optimize:");
        for param in &component_parameters {
            println!("  {:<8} {:<2}: {:<8.4} (range: {:.3} - {:.3})", 
                param.component_name, 
                param.parameter_name, 
                param.current_value,
                param.min_value,
                param.max_value
            );
        }
        println!("Expected metrics: {:?}", spice_runs.iter().map(|r| &r.expected_metrics).collect::<Vec<_>>());
        
        Self {
            target_values,
            weights,
            spice_runs,
            component_parameters,
            current_iteration: 0,
            previous_results: Vec::new(),
        }
    }
    
    /// Update target values for specific metrics
    pub fn set_target_values(&mut self, targets: HashMap<String, f64>) {
        println!("=== Setting Target Values ===");
        for (key, value) in &targets {
            println!("  {}: {:.6}", key, value);
            self.target_values.insert(key.clone(), *value);
        }
    }
    
    /// Iterate to setup the next run based on feedback from the current one
    /// This function analyzes previous results and adjusts parameters for the next iteration
    pub fn iterate(&mut self, current_results: Vec<SimulationResult>) -> Vec<f64> {
        println!("\n=== Optimization Iteration {} ===", self.current_iteration + 1);
        
        // Store old parameters for comparison
        let old_params: Vec<f64> = self.component_parameters.iter().map(|p| p.current_value).collect();
        
        self.current_iteration += 1;
        self.previous_results.extend(current_results.clone());
        
        // Display current simulation results
        println!("Simulation Results:");
        for (run_idx, result) in current_results.iter().enumerate() {
            println!("  Run {}: success={}, time={:.3}s", run_idx, result.success, result.execution_time);
            for (metric_name, value) in &result.metrics {
                if let Some(&target) = self.target_values.get(&format!("{}_{}", metric_name, run_idx)) {
                    let error_percent = if target != 0.0 {
                        (value - target) / target * 100.0
                    } else {
                        0.0
                    };
                    println!("    {}: {:.6} (target: {:.6}, error: {:.1}%)", 
                             metric_name, value, target, error_percent);
                } else {
                    println!("    {}: {:.6}", metric_name, value);
                }
            }
        }
        
        // Analyze current results and extract metrics
        let mut metric_values: HashMap<String, f64> = HashMap::new();
        for (run_idx, result) in current_results.iter().enumerate() {
            for (metric_name, value) in &result.metrics {
                let key = format!("{}_{}", metric_name, run_idx);
                metric_values.insert(key, *value);
            }
        }
        
        // Calculate parameter adjustments based on results
        let mut new_params = Vec::new();
        for i in 0..self.component_parameters.len() {
            let adjusted_value = self.calculate_parameter_adjustment(&self.component_parameters[i], &metric_values);
            new_params.push(adjusted_value);
        }
        
        // Update the parameters
        for (i, param) in self.component_parameters.iter_mut().enumerate() {
            if i < new_params.len() {
                param.current_value = new_params[i];
            }
        }
        
        // Apply learning rate decay based on iteration
        let learning_rate = 1.0 / (1.0 + 0.1 * self.current_iteration as f64);
        println!("Learning rate: {:.4}", learning_rate);
        
        // Adjust parameters with bounds checking and learning rate
        for (i, param) in self.component_parameters.iter_mut().enumerate() {
            if i < new_params.len() {
                let old_value = param.current_value;
                let suggested_value = new_params[i];
                
                // Apply learning rate
                let adjusted_value = old_value + learning_rate * (suggested_value - old_value);
                
                // Clamp to bounds
                param.current_value = adjusted_value.clamp(param.min_value, param.max_value);
                new_params[i] = param.current_value;
            }
        }
        
        // Display parameter changes
        println!("Parameter Updates:");
        for (i, param) in self.component_parameters.iter().enumerate() {
            if i < old_params.len() && i < new_params.len() {
                let change = new_params[i] - old_params[i];
                let change_sign = if change > 0.0 { "+" } else { "" };
                println!("  {:<8} {:<2}: {:<8.4} -> {:<8.4} ({}{:.6}) [{:.3}-{:.3}]", 
                    param.component_name, 
                    param.parameter_name, 
                    old_params[i],
                    new_params[i],
                    change_sign,
                    change,
                    param.min_value,
                    param.max_value
                );
            }
        }
        
        new_params
    }
    
    /// Calculate parameter adjustment based on current metrics and targets
    fn calculate_parameter_adjustment(
        &self, 
        param: &ComponentParameter, 
        metric_values: &HashMap<String, f64>
    ) -> f64 {
        let mut total_error = 0.0;
        let mut total_weight = 0.0;
        
        // Calculate weighted error across all relevant metrics
        for (run_idx, run_config) in self.spice_runs.iter().enumerate() {
            for metric_name in &run_config.expected_metrics {
                let key = format!("{}_{}", metric_name, run_idx);
                let target_key = key.clone();
                
                if let (Some(&current_value), Some(&target_value)) = 
                    (metric_values.get(&key), self.target_values.get(&target_key)) {
                    
                    let error = (current_value - target_value) / target_value.abs().max(1e-9);
                    total_error += error * run_config.weight;
                    total_weight += run_config.weight;
                }
            }
        }
        
        if total_weight > 0.0 {
            let normalized_error = total_error / total_weight;
            
            // Simple gradient-based adjustment
            let adjustment_factor = 0.1; // This could be made adaptive
            let direction = if normalized_error > 0.0 { -1.0 } else { 1.0 };
            let magnitude = normalized_error.abs() * adjustment_factor;
            
            let range = param.max_value - param.min_value;
            let adjustment = direction * magnitude * range * 0.01; // 1% of range max
            
            param.current_value + adjustment
        } else {
            // No feedback available, keep current value
            param.current_value
        }
    }
    
    /// Get current parameter vector for optimization algorithms
    pub fn get_current_parameters(&self) -> Vec<f64> {
        self.component_parameters.iter().map(|p| p.current_value).collect()
    }
    
    /// Get parameter bounds for constrained optimization
    pub fn get_parameter_bounds(&self) -> (Vec<f64>, Vec<f64>) {
        let lower_bounds: Vec<f64> = self.component_parameters.iter().map(|p| p.min_value).collect();
        let upper_bounds: Vec<f64> = self.component_parameters.iter().map(|p| p.max_value).collect();
        (lower_bounds, upper_bounds)
    }
    
    /// Get current iteration count
    pub fn get_iteration(&self) -> usize {
        self.current_iteration
    }
    
    /// Get history of previous simulation results
    pub fn get_result_history(&self) -> &[SimulationResult] {
        &self.previous_results
    }
}
