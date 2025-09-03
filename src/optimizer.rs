use argmin::core::{CostFunction, Gradient};
use std::path::PathBuf;
use glob::glob;
use std::collections::HashMap;
use crate::xschem::{XSchemIO};
use crate::ngspice::{gen_spice_files, run_spice};
use crate::utilities::{glob_files};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

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

#[derive(Debug, Clone)]
pub struct ComponentParameter {
    pub component_name: String,
    pub properties: HashMap<String, usize>, // Maps property name to parameter index
}

impl ComponentParameter {
    pub fn new(component_name: &str) -> Self {
        Self {
            component_name: component_name.to_string(),
            properties: HashMap::new(),
        }
    }

    pub fn add_property(&mut self, property_name: &str, param_index: usize) -> &mut Self {
        self.properties.insert(property_name.to_string(), param_index);
        self
    }
}

// Argmin Solver Problem with verbose support
#[derive(Debug)]
pub struct OptimizationProblem {
    pub target_metrics: Vec<TargetMetric>,
    pub component_parameters: Vec<ComponentParameter>,

    // Files involved
    current_dir: PathBuf,
    netlist_dir: PathBuf,
    
    // Verbose flag and iteration counter
    verbose: bool,
    iteration_count: AtomicU64,
}

impl Clone for OptimizationProblem {
    fn clone(&self) -> Self {
        Self {
            target_metrics: self.target_metrics.clone(),
            component_parameters: self.component_parameters.clone(),
            current_dir: self.current_dir.clone(),
            netlist_dir: self.netlist_dir.clone(),
            verbose: self.verbose,
            iteration_count: AtomicU64::new(self.iteration_count.load(Ordering::SeqCst)),
        }
    }
}

impl OptimizationProblem {
    pub fn new(
        target_metrics: Vec<TargetMetric>, 
        component_parameters: Vec<ComponentParameter>,
        current_dir: PathBuf, 
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> Self {
        Self {
            target_metrics,
            component_parameters,
            current_dir,
            netlist_dir,
            verbose,
            iteration_count: AtomicU64::new(0),
        }
    }

    /// Create optimization problem from component data
    /// Format: Vec<(component_name, HashMap<property_name, initial_value>)>
    /// Returns the problem and initial parameter values
    pub fn with_component_data(
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> (Self, Vec<f64>) {
        let mut component_parameters = Vec::new();
        let mut initial_params = Vec::new();
        let mut param_index = 0;
        
        vprintln!(verbose, "Setting up optimization problem with {} components", component_data.len());
        
        for (component_name, properties) in component_data {
            let mut comp_param = ComponentParameter::new(&component_name);
            
            // Sort properties by key for consistent ordering
            let mut sorted_props: Vec<_> = properties.iter().collect();
            sorted_props.sort_by_key(|(k, _)| *k);
            
            vprintln!(verbose, "  Component {}: {} parameters", component_name, sorted_props.len());
            
            for (property_name, &value) in sorted_props {
                comp_param.add_property(property_name, param_index);
                initial_params.push(value);
                
                vprintln!(verbose, "    {}[{}] = {} (param index {})", 
                         component_name, property_name, value, param_index);
                
                param_index += 1;
            }
            
            component_parameters.push(comp_param);
        }
        
        let problem = Self::new(target_metrics, component_parameters, current_dir, netlist_dir, verbose);
        (problem, initial_params)
    }

    /// Create from the old parameter map format for backward compatibility
    /// This is the old format: Vec<(property_name, component_name)>
    /// Note: This doesn't provide initial values, so you need to provide them separately
    pub fn from_parameter_map(
        target_metrics: Vec<TargetMetric>, 
        parameter_map: Vec<(String, String)>, // (property_name, component_name)
        current_dir: PathBuf, 
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> Self {
        let mut component_parameters: Vec<ComponentParameter> = Vec::new();
        
        vprintln!(verbose, "Creating optimization problem from parameter map with {} entries", parameter_map.len());
        
        for (param_index, (property_name, component_name)) in parameter_map.iter().enumerate() {
            vprintln!(verbose, "  Mapping {}:{} to param index {}", component_name, property_name, param_index);
            
            // Find existing component parameter or create new one
            if let Some(comp_param) = component_parameters.iter_mut()
                .find(|cp| cp.component_name == *component_name) {
                comp_param.add_property(property_name, param_index);
            } else {
                let mut new_comp = ComponentParameter::new(component_name);
                new_comp.add_property(property_name, param_index);
                component_parameters.push(new_comp);
            }
        }
        
        Self::new(target_metrics, component_parameters, current_dir, netlist_dir, verbose)
    }

    /// Create from component data with separate initial values (alternative approach)
    pub fn from_component_data_with_initial_values(
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> (Self, Vec<f64>) {
        // This is the same as with_component_data - keeping for backward compatibility
        Self::with_component_data(target_metrics, component_data, current_dir, netlist_dir, verbose)
    }

    pub fn update_simulation(&self, parameters: &[f64]) -> Result<(), String> {
        let iteration = self.iteration_count.fetch_add(1, Ordering::SeqCst);
        
        vprintln!(self.verbose, "\n=== Iteration {} ===", iteration + 1);
        vprintln!(self.verbose, "Updating simulation with {} parameters: {:?}", parameters.len(), parameters);
        
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| {
                vprintln!(self.verbose, "Error globbing files: {}", e);
                format!("Failed to glob files: {}", e)
            })?;
        
        if let Some(schematic_file) = &files.schematic {
            vprintln!(self.verbose, "Loading schematic: {}", schematic_file);
            
            // Load and update the schematic with new parameters
            let mut schematic = XSchemIO::load(schematic_file)
                .map_err(|e| {
                    vprintln!(self.verbose, "Error loading schematic: {}", e);
                    format!("Failed to load schematic: {}", e)
                })?;

            // Update parameters in the schematic using structured approach
            for comp_param in &self.component_parameters {
                vprintln!(self.verbose, "  Searching for component: {}", comp_param.component_name);
                
                if let Some(component) = schematic.find_component_by_name_mut(&comp_param.component_name) {
                    vprintln!(self.verbose, "  ✓ Found component: {}", comp_param.component_name);
                    
                    for (property_name, &param_index) in &comp_param.properties {
                        if let Some(&parameter_value) = parameters.get(param_index) {
                            let old_value = component.properties.get(property_name).cloned();
                            component.properties.insert(property_name.clone(), parameter_value.to_string());
                            
                            match old_value {
                                Some(old) => vprintln!(self.verbose, "    {}: {} -> {}", property_name, old, parameter_value),
                                None => vprintln!(self.verbose, "    {}: (new) -> {}", property_name, parameter_value),
                            }
                        } else {
                            vprintln!(self.verbose, "    ⚠ Warning: Parameter index {} out of bounds for {} (max index: {})", 
                                     param_index, property_name, parameters.len().saturating_sub(1));
                        }
                    }
                } else {
                    vprintln!(self.verbose, "  ✗ Warning: Component {} not found in schematic", comp_param.component_name);
                }
            }

            vprintln!(self.verbose, "Saving updated schematic to: {}", schematic_file);
            schematic.save(schematic_file)
                .map_err(|e| {
                    vprintln!(self.verbose, "Error saving schematic: {}", e);
                    format!("Failed to save schematic: {}", e)
                })?;
                
            vprintln!(self.verbose, "✓ Schematic update completed successfully");
        } else {
            vprintln!(self.verbose, "⚠ Warning: No schematic file found for updating");
        }

        Ok(())
    }

    pub fn run_simulation(&self) -> Result<Vec<f64>, String> {
        vprintln!(self.verbose, "Starting simulation...");
        
        // Generate SPICE files if needed
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| {
                vprintln!(self.verbose, "Error globbing files for simulation: {}", e);
                format!("Failed to glob files: {}", e)
            })?;
        
        if let Some(testbench_file) = &files.testbench {
            vprintln!(self.verbose, "Found testbench file: {}", testbench_file);
            
            // Collect SPICE codes from target metrics
            let spice_codes: Vec<String> = self.target_metrics.iter()
                .map(|metric| {
                    vprintln!(self.verbose, "  Adding SPICE code for metric: {}", metric.target_name);
                    metric.spice_code.clone()
                })
                .collect();
            
            vprintln!(self.verbose, "Generating SPICE files with {} analysis blocks...", spice_codes.len());
            
            let results = gen_spice_files(
                testbench_file, 
                &self.current_dir, 
                self.netlist_dir.to_str().unwrap(),
                spice_codes,
                self.verbose
            ).map_err(|e| {
                vprintln!(self.verbose, "SPICE generation failed: {}", e);
                format!("SPICE generation failed: {}", e)
            })?;
            
            if !results.success {
                vprintln!(self.verbose, "SPICE generation unsuccessful: {:?}", results.error);
                return Err(format!("SPICE generation failed: {:?}", results.error));
            }
            
            vprintln!(self.verbose, "✓ SPICE generation completed successfully");
        } else {
            vprintln!(self.verbose, "⚠ No testbench file found, skipping SPICE generation");
        }

        // Find and run all SPICE files in netlist directory
        let spice_pattern = format!("{}/**/*.spice", self.netlist_dir.to_str().unwrap());
        vprintln!(self.verbose, "Looking for SPICE files with pattern: {}", spice_pattern);
        
        let spice_files: Vec<_> = glob(&spice_pattern)
            .map_err(|e| {
                vprintln!(self.verbose, "Failed to glob SPICE files: {}", e);
                format!("Failed to glob SPICE files: {}", e)
            })?
            .filter_map(|entry| entry.ok())
            .collect();

        if spice_files.is_empty() {
            vprintln!(self.verbose, "✗ No SPICE files found for simulation");
            return Err("No SPICE files found for simulation".to_string());
        }
        
        vprintln!(self.verbose, "Found {} SPICE files to simulate:", spice_files.len());
        for (i, file) in spice_files.iter().enumerate() {
            vprintln!(self.verbose, "  {}: {}", i + 1, file.display());
        }

        // Run simulations on all SPICE files (sequential for thread safety)
        vprintln!(self.verbose, "Running simulations sequentially...");
        
        let mut all_metrics = HashMap::new();
        let mut successful_sims = 0;
        let mut failed_sims = 0;
        
        for (i, spice_file) in spice_files.iter().enumerate() {
            vprintln!(self.verbose, "  Starting simulation {}: {}", i + 1, spice_file.display());
            
            match run_spice(spice_file, self.verbose) {
                Ok(results) => {
                    if results.success {
                        successful_sims += 1;
                        vprintln!(self.verbose, "  ✓ Simulation {} completed successfully with {} metrics", 
                               i + 1, results.get_metrics().len());
                        
                        for (key, value) in results.get_metrics() {
                            all_metrics.insert(key.clone(), *value);
                            vprintln!(self.verbose, "    Collected {}: {:.6e}", key, value);
                        }
                    } else {
                        failed_sims += 1;
                        let error_msg = format!("SPICE simulation failed for {}: {:?}", 
                                               spice_file.display(), results.error);
                        vprintln!(self.verbose, "  ✗ {}", error_msg);
                    }
                }
                Err(e) => {
                    failed_sims += 1;
                    let error_msg = format!("Failed to run SPICE simulation for {}: {}", 
                                           spice_file.display(), e);
                    vprintln!(self.verbose, "  ✗ {}", error_msg);
                }
            }
        }
        
        vprintln!(self.verbose, "Simulation summary: {} successful, {} failed", successful_sims, failed_sims);
        
        if successful_sims == 0 {
            let error_msg = "All simulations failed".to_string();
            vprintln!(self.verbose, "✗ {}", error_msg);
            return Err(error_msg);
        }
        
        // Extract target metrics in the order they appear in target_metrics
        vprintln!(self.verbose, "Extracting {} target metrics from simulation results...", self.target_metrics.len());
        
        let result_values: Vec<f64> = self.target_metrics.iter()
            .map(|target| {
                let value = all_metrics.get(&target.target_name).copied().unwrap_or(0.0);
                vprintln!(self.verbose, "  {}: {:.6e} (target: {:.6e})", 
                         target.target_name, value, target.target_value);
                value
            })
            .collect();
        
        vprintln!(self.verbose, "✓ Simulation completed successfully with {} target values", result_values.len());
        Ok(result_values)
    }
}

/// Implement CostFunction for argmin
impl CostFunction for OptimizationProblem {
    type Param = Vec<f64>;
    type Output = f64;
    
    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        let iteration = self.iteration_count.load(Ordering::SeqCst);
        vprintln!(self.verbose, "\n🔍 Computing cost function (iteration {})", iteration);
        
        // Update simulation with new parameters
        if let Err(e) = self.update_simulation(param) {
            vprintln!(self.verbose, "✗ Failed to update simulation: {}", e);
            return Ok(f64::MAX);
        }
        
        // Run simulation and get target metric values
        match self.run_simulation() {
            Ok(metric_values) => {
                // Calculate Sum of Squared Errors
                let mut cost = 0.0;
                vprintln!(self.verbose, "Computing cost from {} metrics:", metric_values.len());
                
                for (i, target) in self.target_metrics.iter().enumerate() {
                    let measured = metric_values.get(i).copied().unwrap_or(0.0);
                    let error = measured - target.target_value;
                    let squared_error = error.powi(2);
                    cost += squared_error;
                    
                    vprintln!(self.verbose, "  {}: measured={:.6e}, target={:.6e}, error={:.6e}, squared_error={:.6e}", 
                             target.target_name, measured, target.target_value, error, squared_error);
                }
                
                vprintln!(self.verbose, "📊 Total cost: {:.6e}", cost);
                Ok(cost)
            },
            Err(e) => {
                vprintln!(self.verbose, "✗ Simulation error: {}", e);
                vprintln!(self.verbose, "📊 Returning maximum cost due to simulation failure");
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
        
        vprintln!(self.verbose, "\n🔢 Computing numerical gradient with epsilon = {:.2e}", epsilon);
        vprintln!(self.verbose, "Parameter vector length: {}", param.len());
        
        let base_cost = self.cost(param)?;
        vprintln!(self.verbose, "Base cost: {:.6e}", base_cost);
        
        let mut gradient = Vec::with_capacity(param.len());
        
        for i in 0..param.len() {
            let mut param_plus = param.clone();
            param_plus[i] += epsilon;
            let cost_plus = self.cost(&param_plus)?;
            
            let grad_i = (cost_plus - base_cost) / epsilon;
            gradient.push(grad_i);
            
            vprintln!(self.verbose, "  ∂f/∂x[{}]: {:.6e} (cost_plus: {:.6e})", i, grad_i, cost_plus);
        }
        
        let gradient_magnitude: f64 = gradient.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
        vprintln!(self.verbose, "📈 Gradient magnitude: {:.6e}", gradient_magnitude);
        
        Ok(gradient)
    }
}
