use argmin::core::{CostFunction, Gradient};
use std::path::PathBuf;
use std::collections::HashMap;
use crate::xschem::{XSchemIO};
use crate::ngspice::{SpiceInterface};
use crate::utilities::{glob_files};
use crate::pyinterface::TestConfiguration;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
use crate::{vprintln, safe_println};

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

// Optimized problem structure with caching
#[derive(Debug)]
pub struct OptimizationProblem {
    pub target_metrics: Vec<TargetMetric>,
    pub component_parameters: Vec<ComponentParameter>,

    // Files involved
    current_dir: PathBuf,
    netlist_dir: PathBuf,
    
    // Caching for performance
    base_schematic: Arc<XSchemIO>, // Cached base schematic to avoid repeated loading
    spice_codes: Arc<Vec<String>>, // Cached SPICE codes
    test_configs: Arc<Vec<TestConfiguration>>, // Cached test configurations with component values
    
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
            base_schematic: Arc::clone(&self.base_schematic),
            spice_codes: Arc::clone(&self.spice_codes),
            test_configs: Arc::clone(&self.test_configs),
            verbose: self.verbose,
            iteration_count: AtomicU64::new(self.iteration_count.load(Ordering::SeqCst)),
        }
    }
}

impl OptimizationProblem {
    pub fn new(
        target_metrics: Vec<TargetMetric>, 
        component_data: Vec<(String, HashMap<String, f64>)>,
        test_configs: HashMap<String, TestConfiguration>,
        current_dir: PathBuf, 
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> Result<(Self, Vec<f64>), String> {
        let mut component_parameters = Vec::new();
        let mut initial_params = Vec::new();
        let mut param_index = 0;
        
        vprintln!(verbose, "Setting up optimization problem with {} components", component_data.len());
        
        for (component_name, properties) in component_data {
            let mut comp_param = ComponentParameter::new(&component_name);
            
            // Sort properties for consistent ordering
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
        
        let schematic_file = glob_files(current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?
            .schematic
            .ok_or("No schematic file found in current directory")?;
        let base_schematic = XSchemIO::load(&schematic_file, verbose)
            .map_err(|e| format!("Failed to load schematic: {}", e))?;
            
        // Create ordered list of test configs matching target metrics order
        let ordered_test_configs: Vec<TestConfiguration> = target_metrics.iter().map(|tm| {
            // Find the test configuration that corresponds to this target metric's SPICE code
            test_configs.values()
                .find(|tc| tc.get_spice_code() == tm.spice_code)
                .cloned()
                .unwrap_or_else(|| {
                    // Create empty test config if not found (fallback)
                    TestConfiguration {
                        component_values: HashMap::new(),
                        spice: Some(tm.spice_code.clone()),
                    }
                })
        }).collect();
            
        let problem = Self {
            target_metrics: target_metrics.clone(),
            component_parameters,
            current_dir: current_dir.clone(),
            netlist_dir: netlist_dir.clone(),
            base_schematic: Arc::new(base_schematic),
            spice_codes: Arc::new(target_metrics.iter().map(|tm| tm.spice_code.clone()).collect()),
            test_configs: Arc::new(ordered_test_configs),
            verbose,
            iteration_count: AtomicU64::new(0),
        };
        Ok((problem, initial_params))
    }

    /// Fast parameter update using cached schematic
    pub fn update_simulation(&self, parameters: &[f64]) -> Result<(), String> {
        let iteration = self.iteration_count.fetch_add(1, Ordering::SeqCst);
        
        vprintln!(self.verbose, "\n=== Iteration {} ===", iteration + 1);
        vprintln!(self.verbose, "Updating simulation with {} parameters", parameters.len());
        
        // Always show iteration number for parameter updates (not conditional on verbose)
        safe_println!("\n=== Iteration {} Parameter Updates ===", iteration + 1);
        
        // Clone the cached base schematic instead of reloading from disk
        let mut schematic = (*self.base_schematic).clone();
        schematic.set_verbose(self.verbose);
        
        // Update parameters using optimized lookup methods
        for comp_param in &self.component_parameters {
            if let Some(component) = schematic.find_component_by_name_mut(&comp_param.component_name) {
                vprintln!(self.verbose, "  ✓ Found component: {}", comp_param.component_name);
                
                for (property_name, &param_index) in &comp_param.properties {
                    if let Some(&parameter_value) = parameters.get(param_index) {
                        let old_value = component.properties.get(property_name).cloned();
                        component.properties.insert(property_name.clone(), parameter_value.to_string());
                        
                        // Always show parameter updates (not conditional on verbose)
                        match old_value {
                            Some(old) => {
                                safe_println!("  {}[{}]: {} -> {}", comp_param.component_name, property_name, old, parameter_value);
                            },
                            None => {
                                safe_println!("  {}[{}]: (new) -> {}", comp_param.component_name, property_name, parameter_value);
                            },
                        }
                    }
                }
            } else {
                vprintln!(self.verbose, "  ⚠ Component {} not found", comp_param.component_name);
            }
        }

        // Save only once per iteration
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        if let Some(schematic_file) = &files.schematic {
            vprintln!(self.verbose, "Saving updated schematic");
            schematic.save(schematic_file)
                .map_err(|e| format!("Failed to save schematic: {}", e))?;
        }

        Ok(())
    }

    pub fn run_simulation(&self) -> Result<Vec<f64>, String> {
        vprintln!(self.verbose, "Starting simulation...");
        
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        let base_schematic_file = files.schematic
            .ok_or("No schematic file found")?;
        let base_testbench_file = files.testbench
            .ok_or("No testbench file found")?;
        let base_symbol_file = files.symbol
            .ok_or("No symbol file found")?;

        if self.spice_codes.is_empty() {
            return Err("No SPICE codes provided".to_string());
        }

        vprintln!(self.verbose, "Creating {} temporary schematic/testbench pairs for parallel processing", self.spice_codes.len());

        // Create array of SpiceInterface instances with temporary files
        let spice_interfaces: Vec<(SpiceInterface, PathBuf, PathBuf, PathBuf)> = self.spice_codes
            .iter()
            .enumerate()
            .map(|(index, spice_code)| {
                // Generate unique temporary file names
                let uuid = Uuid::new_v4();
                let temp_suffix = format!("_{}", uuid.simple());
                
                // Create temporary schematic, symbol, and testbench files
                let temp_schematic = self.current_dir.join(format!("temp_schemsym{}.sch", temp_suffix));
                let temp_symbol = self.current_dir.join(format!("temp_schemsym{}.sym", temp_suffix));
                let temp_testbench = self.current_dir.join(format!("temp_testbench{}.sch", temp_suffix));
                
                // Determine expected SPICE file path
                let spice_file_name = temp_testbench.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid testbench file name")?;
                let temp_spice_file = self.current_dir.join(self.netlist_dir.join(format!("{}.spice", spice_file_name)));

                vprintln!(self.verbose, "  Instance {}: {}", index, temp_testbench.display());
                vprintln!(self.verbose, "    Schematic: {}", temp_schematic.display());
                vprintln!(self.verbose, "    Symbol: {}", temp_symbol.display());
                vprintln!(self.verbose, "    SPICE file: {}", temp_spice_file.display());

                // Copy the base schematic to temp schematic (already updated with current parameters)
                std::fs::copy(&base_schematic_file, &temp_schematic)
                    .map_err(|e| format!("Failed to copy schematic to {}: {}", temp_schematic.display(), e))?;

                // Copy the base symbol with the same name as the temp schematic
                std::fs::copy(&base_symbol_file, &temp_symbol)
                    .map_err(|e| format!("Failed to copy symbol to {}: {}", temp_symbol.display(), e))?;

                // Create temporary testbench using XSchemIO
                let mut testbench_xschem = XSchemIO::load(&base_testbench_file, self.verbose)
                    .map_err(|e| format!("Failed to load base testbench: {}", e))?;
                
                // Update testbench to reference the temporary symbol
                let base_symbol_name = std::path::Path::new(&base_symbol_file)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid base symbol file name")?;
                let temp_symbol_name = temp_symbol.file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid temp symbol file name")?;
                
                // Update all components that reference the base symbol to use the temp symbol
                for obj in testbench_xschem.get_all_objects_mut() {
                    if let crate::xschem::XSchemObject::Component(comp) = obj {
                        if comp.symbol_reference.contains(base_symbol_name) {
                            comp.symbol_reference = comp.symbol_reference.replace(base_symbol_name, temp_symbol_name);
                        }
                    }
                }
                
                // Update testbench component values from test configuration
                let test_config = &self.test_configs[index];
                if let Err(error) = testbench_xschem.update_testbench_components(&test_config.component_values) {
                    return Err(format!("Failed to update testbench components for instance {}: {}", index, error));
                }
                
                // Set the SPICE code using XSchemIO functionality
                testbench_xschem.set_spice_code(spice_code);
                
                // Save the updated testbench
                testbench_xschem.save(temp_testbench.to_str().unwrap())
                    .map_err(|e| format!("Failed to write temp testbench: {}", e))?;

                // Create SpiceInterface with shared configuration
                let spice_interface = SpiceInterface::new(
                    temp_testbench.clone(),
                    temp_spice_file,
                    self.netlist_dir.clone(),
                    self.get_shared_sky130_version(), // You'll need to implement this
                    Arc::new(AtomicBool::new(false)), // Each instance gets its own xschemrc_created flag
                    self.verbose,
                );

                Ok((spice_interface, temp_schematic, temp_symbol, temp_testbench))
            })
            .collect::<Result<Vec<_>, String>>()?;

        let total_interfaces = spice_interfaces.len();
        vprintln!(self.verbose, "✓ Created {} SpiceInterface instances", total_interfaces);

        // First phase: Generate all SPICE files sequentially (XSchem conflicts)
        vprintln!(self.verbose, "Phase 1: Generating SPICE files sequentially to avoid XSchem conflicts...");
        let mut successful_interfaces = Vec::new();
        
        for (index, (spice_interface, temp_schematic, temp_symbol, temp_testbench)) in spice_interfaces.into_iter().enumerate() {
            vprintln!(self.verbose, "  Generating SPICE for instance {}", index);
            
            match spice_interface.gen_spice_file() {
                Ok(result) => {
                    if result.is_success() {
                        vprintln!(self.verbose, "    ✓ SPICE generation succeeded for instance {}", index);
                        successful_interfaces.push((index, spice_interface, temp_schematic, temp_symbol, temp_testbench));
                    } else {
                        vprintln!(self.verbose, "    ✗ SPICE generation failed for instance {}: {:?}", 
                                 index, result.get_error());
                    }
                }
                Err(e) => {
                    vprintln!(self.verbose, "    ✗ SPICE generation error for instance {}: {}", index, e);
                }
            }
        }
        
        // Second phase: Run SPICE simulations in parallel (no conflicts)
        let simulation_results: Vec<_> = if successful_interfaces.len() > 1 {
            vprintln!(self.verbose, "Phase 2: Running {} SPICE simulations in parallel...", successful_interfaces.len());
            
            successful_interfaces.par_iter()
                .map(|(index, spice_interface, _, _, _)| {
                    vprintln!(self.verbose, "  Running simulation for instance {} in parallel", index);

                    // Run simulation
                    match spice_interface.run_spice() {
                        Ok(sim_result) => {
                            if sim_result.success {
                                vprintln!(self.verbose, "    ✓ Simulation succeeded for instance {} ({} metrics)", 
                                         index, sim_result.metrics.len());
                                Some((*index, sim_result))
                            } else {
                                vprintln!(self.verbose, "    ✗ Simulation failed for instance {}: {:?}", 
                                         index, sim_result.get_error());
                                None
                            }
                        }
                        Err(e) => {
                            vprintln!(self.verbose, "    ✗ Simulation error for instance {}: {}", index, e);
                            None
                        }
                    }
                })
                .collect()
        } else if successful_interfaces.len() == 1 {
            // Sequential execution for single instance
            vprintln!(self.verbose, "Running single SPICE simulation...");
            
            let (index, spice_interface, _, _, _) = &successful_interfaces[0];
            
            // Run simulation
            let sim_result = spice_interface.run_spice()
                .map_err(|e| format!("Simulation failed: {}", e))?;
            
            if sim_result.success {
                vec![Some((*index, sim_result))]
            } else {
                return Err(format!("Simulation failed: {:?}", sim_result.get_error()));
            }
        } else {
            // No successful SPICE generations
            return Err("No SPICE files were generated successfully".to_string());
        };

        // Clean up temporary files
        vprintln!(self.verbose, "Cleaning up temporary files...");
        for (_, spice_interface, temp_schematic, temp_symbol, temp_testbench) in &successful_interfaces {
            let _ = std::fs::remove_file(temp_schematic);
            let _ = std::fs::remove_file(temp_symbol);
            let _ = std::fs::remove_file(temp_testbench);
            // Also clean up the generated SPICE file
            let _ = std::fs::remove_file(&spice_interface.spice_file);
            vprintln!(self.verbose, "  Removed: {}", temp_schematic.display());
            vprintln!(self.verbose, "  Removed: {}", temp_symbol.display());
            vprintln!(self.verbose, "  Removed: {}", temp_testbench.display()); 
            vprintln!(self.verbose, "  Removed: {}", spice_interface.spice_file.display());
        }

        // Collect and merge metrics from all successful simulations
        let mut all_metrics = HashMap::with_capacity(self.target_metrics.len());
        let mut successful_sims = 0;

        for result_opt in simulation_results {
            if let Some((index, result)) = result_opt {
                successful_sims += 1;
                vprintln!(self.verbose, "  ✓ Instance {} contributed {} metrics", index, result.metrics.len());
                
                // Merge metrics (later instances override earlier ones if there are conflicts)
                all_metrics.extend(result.get_metrics().clone());
            }
        }

        if successful_sims == 0 {
            return Err("All simulations failed".to_string());
        }

        vprintln!(self.verbose, "✓ {} of {} simulations succeeded", successful_sims, total_interfaces);

        // Extract target metrics in order
        let result_values: Vec<f64> = self.target_metrics.iter()
            .map(|target| {
                let value = all_metrics.get(&target.target_name).copied().unwrap_or(0.0);
                vprintln!(self.verbose, "  {}: {:.6e} (target: {:.6e})", 
                         target.target_name, value, target.target_value);
                value
            })
            .collect();

        vprintln!(self.verbose, "✓ Simulation completed with {} target values", result_values.len());
        Ok(result_values)
    }

    fn get_shared_sky130_version(&self) -> String {
        "0fe599b2afb6708d281543108caf8310912f54af".to_string()
    }
}

// Optimized cost function with better error handling
impl CostFunction for OptimizationProblem {
    type Param = Vec<f64>;
    type Output = f64;
    
    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        let iteration = self.iteration_count.load(Ordering::SeqCst);
        vprintln!(self.verbose, "\n🔍 Computing cost function (iteration {})", iteration);
        
        // Early parameter validation
        if param.is_empty() {
            return Ok(f64::MAX);
        }

        // Update simulation
        if let Err(e) = self.update_simulation(param) {
            vprintln!(self.verbose, "✗ Simulation update failed: {}", e);
            return Ok(f64::MAX);
        }
        
        // Run simulation
        match self.run_simulation() {
            Ok(metric_values) => {
                // Vectorized cost calculation
                let cost: f64 = self.target_metrics.iter()
                    .zip(metric_values.iter())
                    .map(|(target, &measured)| {
                        let error = measured - target.target_value;
                        let squared_error = error.powi(2);
                        
                        vprintln!(self.verbose, "  {}: measured={:.6e}, target={:.6e}, error={:.6e}", 
                                 target.target_name, measured, target.target_value, error);
                        
                        squared_error
                    })
                    .sum();
                
                vprintln!(self.verbose, "📊 Total cost: {:.6e}", cost);
                Ok(cost)
            },
            Err(e) => {
                vprintln!(self.verbose, "✗ Simulation error: {}", e);
                Ok(f64::MAX)
            }
        }
    }
}

// Optimized gradient calculation
impl Gradient for OptimizationProblem {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;
    
    fn gradient(&self, param: &Self::Param) -> Result<Self::Gradient, argmin::core::Error> {
        const EPSILON: f64 = 1e-8;
        
        vprintln!(self.verbose, "\n🔢 Computing numerical gradient with epsilon = {:.2e}", EPSILON);
        
        let base_cost = self.cost(param)?;
        vprintln!(self.verbose, "Base cost: {:.6e}", base_cost);
        
        // Pre-allocate gradient vector
        let mut gradient = Vec::with_capacity(param.len());
        
        // Parallel gradient computation for better performance
        if param.len() > 4 {
            vprintln!(self.verbose, "Using parallel gradient computation");
            
            let gradients: Vec<f64> = (0..param.len()).into_par_iter()
                .map(|i| {
                    let mut param_plus = param.to_vec();
                    param_plus[i] += EPSILON;
                    
                    match self.cost(&param_plus) {
                        Ok(cost_plus) => (cost_plus - base_cost) / EPSILON,
                        Err(_) => 0.0,
                    }
                })
                .collect();
            
            gradient = gradients;
        } else {
            // Sequential for small parameter vectors
            for i in 0..param.len() {
                let mut param_plus = param.to_vec();
                param_plus[i] += EPSILON;
                let cost_plus = self.cost(&param_plus)?;
                
                let grad_i = (cost_plus - base_cost) / EPSILON;
                gradient.push(grad_i);
                
                vprintln!(self.verbose, "  ∂f/∂x[{}]: {:.6e}", i, grad_i);
            }
        }
        
        let gradient_magnitude: f64 = gradient.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
        vprintln!(self.verbose, "📈 Gradient magnitude: {:.6e}", gradient_magnitude);
        
        Ok(gradient)
    }
}
