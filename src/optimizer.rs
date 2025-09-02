use argmin::core::{CostFunction, Gradient};
use std::path::PathBuf;
use std::collections::HashMap;
use crate::{gen_spice_file, run_spice, XSchemIO};
use crate::{glob_files};
use glob::glob;

#[derive(Debug, Clone)]
pub struct TargetMetric {
    pub target_name: String,
    pub target_value: f64,
    pub spice_code: String,
}

impl TargetMetric {
    pub fn new(target_name: &str, target_value: f64, spice_code: &str) -> Self {
        Self {
            target_name: target_name.to_string(),
            target_value,
            spice_code: spice_code.to_string(),
        }
    }
}

// Argmin Solver Problem
#[derive(Debug, Clone)]
pub struct OptimizationProblem {
    target_metrics: Vec<TargetMetric>, // User-Defined target
    parameter_map: Vec<(String, String)>, // User-Defined Parameters in Problem

    // Files involved
    current_dir: PathBuf,
    netlist_dir: PathBuf,
}

impl OptimizationProblem {
    pub fn new(target_metrics: Vec<TargetMetric>, parameter_map: Vec<(String, String)>,
             current_dir: PathBuf, netlist_dir: PathBuf) -> Self {
        Self {
            target_metrics,
            parameter_map,
            current_dir,
            netlist_dir,
        }
    }

    pub fn update_simulation(&self, parameters: &[f64]) -> Result<(), String> {
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        if let Some(schematic_file) = &files.schematic {
            // Load and update the schematic with new parameters
            let mut schematic = XSchemIO::load(schematic_file)
                .map_err(|e| format!("Failed to load schematic: {}", e))?;

            // Update parameters in the schematic
            for (i, (param_name, component_name)) in self.parameter_map.iter().enumerate() {
                if let Some(&parameter_value) = parameters.get(i) {
                    if let Some(component) = schematic.find_component_by_name_mut(component_name) {
                        component.properties.insert(param_name.clone(), parameter_value.to_string());
                    }
                }
            }

            // Save the updated schematic
            schematic.save(schematic_file)
                .map_err(|e| format!("Failed to save schematic: {}", e))?;
        }

        Ok(())
    }

    pub fn run_simulation(&self) -> Result<Vec<f64>, String> {
        // Generate SPICE files if needed
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        if let Some(testbench_file) = &files.testbench {
            let results = gen_spice_files(
                testbench_file, 
                &self.current_dir, 
                self.netlist_dir.to_str().unwrap() // update this to take in &str of ngspice code
            ).map_err(|e| format!("SPICE generation failed: {}", e))?;
            
            if !results.success {
                return Err(format!("SPICE generation failed: {:?}", results.error));
            }
        }

        // Find and run all SPICE files in netlist directory
        let spice_pattern = format!("{}/**/*.spice", self.netlist_dir.to_str().unwrap());
        let spice_files: Vec<_> = glob(&spice_pattern)
            .map_err(|e| format!("Failed to glob SPICE files: {}", e))?
            .filter_map(|entry| entry.ok())
            .collect();

        if spice_files.is_empty() {
            return Err("No SPICE files found for simulation".to_string());
        }

        let mut all_metrics = HashMap::new();
        
        // Run simulations on all SPICE files (needs to be updated to be run in parallel)
        for spice_file in &spice_files {
            match run_spice(spice_file) {
                Ok(results) => {
                    if results.success {
                        // Merge metrics from this simulation
                        for (key, value) in results.get_metrics() {
                            all_metrics.insert(key.clone(), *value);
                        }
                    } else {
                        eprintln!("SPICE simulation failed for {}: {:?}", 
                                spice_file.display(), results.error);
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to run SPICE simulation for {}: {}", 
                                     spice_file.display(), e));
                }
            }
        }
        
        // Extract target metrics in the order they appear in target_metrics
        let mut result_values = Vec::with_capacity(self.target_metrics.len());
        for target in &self.target_metrics {
            let value = all_metrics.get(&target.target_name)
                .copied()
                .unwrap_or(0.0);
            result_values.push(value);
        }
        
        Ok(result_values)
    }
}

/// Implement CostFunction for argmin
impl CostFunction for OptimizationProblem {
    type Param = Vec<f64>;
    type Output = f64;
    
    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        // Update simulation with new parameters
        if let Err(e) = self.update_simulation(param) {
            eprintln!("Failed to update simulation: {}", e);
            return Ok(f64::MAX);
        }
        
        // Run simulation and get target metric values
        match self.run_simulation() {
            Ok(metric_values) => {
                // Calculate Sum of Squared Errors
                let cost: f64 = self.target_metrics.iter()
                    .enumerate()
                    .map(|(i, target)| {
                        let measured = metric_values.get(i).copied().unwrap_or(0.0);
                        (measured - target.target_value).powi(2)
                    })
                    .sum();
                Ok(cost)
            },
            Err(e) => {
                eprintln!("Simulation error: {}", e);
                Ok(f64::MAX) // Return high cost for failed simulations
            }
        }
    }
}

/// Implement Gradient for argmin
impl Gradient for OptimizationProblem {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;
    
    // Based on numerical differentiation
    fn gradient(&self, param: &Self::Param) -> Result<Self::Gradient, argmin::core::Error> {
        let epsilon = 1e-8;
        let base_cost = self.cost(param)?;
        let mut gradient = Vec::with_capacity(param.len());
        
        for i in 0..param.len() {
            let mut param_plus = param.clone();
            param_plus[i] += epsilon;
            let cost_plus = self.cost(&param_plus)?;
            
            gradient.push((cost_plus - base_cost) / epsilon);
        }
        
        Ok(gradient)
    }
}

