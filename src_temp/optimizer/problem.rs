use argmin::core::{CostFunction, Gradient};
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::pyinterface::TestConfiguration;
use crate::{vprintln, safe_println};

#[derive(Debug, Clone)]
pub struct TargetMetric {
    pub name: String,
    pub target_value: f64,
    pub spice_code: String,
    pub weight: f64,
}

impl TargetMetric {
    pub fn new(name: &str, target_value: f64, spice_code: &str) -> Self {
        Self {
            name: name.to_string(),
            target_value,
            spice_code: spice_code.to_string(),
            weight: 1.0,
        }
    }
    
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
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

    pub fn add_property(&mut self, property_name: &str, param_index: usize) {
        self.properties.insert(property_name.to_string(), param_index);
    }
}

/// Core optimization problem with simulation capabilities
#[derive(Debug)]
pub struct OptimizationProblem {
    pub target_metrics: Vec<TargetMetric>,
    pub component_parameters: Vec<ComponentParameter>,
    pub current_dir: PathBuf,
    pub netlist_dir: PathBuf,
    pub test_configs: Arc<Vec<TestConfiguration>>,
    pub verbose: bool,
    pub iteration_count: AtomicU64,
    
    // Simulation backend
    simulator: Box<dyn SimulationBackend + Send + Sync>,
}

/// Trait for different simulation backends
pub trait SimulationBackend: std::fmt::Debug {
    fn update_parameters(&self, params: &[f64], components: &[ComponentParameter]) -> Result<(), String>;
    fn run_simulation(&self, test_configs: &[TestConfiguration], spice_codes: &[String]) -> Result<Vec<f64>, String>;
}

impl OptimizationProblem {
    pub fn new(
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        test_configs: HashMap<String, TestConfiguration>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        simulator: Box<dyn SimulationBackend + Send + Sync>,
        verbose: bool,
    ) -> Result<(Self, Vec<f64>), String> {
        let mut component_parameters = Vec::new();
        let mut initial_params = Vec::new();
        let mut param_index = 0;
        
        vprintln!(verbose, "Setting up optimization problem with {} components", component_data.len());
        
        // Build parameter mapping
        for (component_name, properties) in component_data {
            let mut comp_param = ComponentParameter::new(&component_name);
            
            // Sort properties for consistent ordering
            let mut sorted_props: Vec<_> = properties.iter().collect();
            sorted_props.sort_by_key(|(k, _)| *k);
            
            vprintln!(verbose, "  Component {}: {} parameters", component_name, sorted_props.len());
            
            for (property_name, &value) in sorted_props {
                comp_param.add_property(property_name, param_index);
                initial_params.push(value);
                
                vprintln!(verbose, "    {}[{}] = {} (index {})", 
                         component_name, property_name, value, param_index);
                param_index += 1;
            }
            
            component_parameters.push(comp_param);
        }
        
        // Create ordered test configs matching target metrics
        let ordered_test_configs: Vec<TestConfiguration> = target_metrics.iter()
            .map(|tm| {
                test_configs.values()
                    .find(|tc| tc.get_spice_code() == tm.spice_code)
                    .cloned()
                    .unwrap_or_else(|| TestConfiguration {
                        component_values: HashMap::new(),
                        spice: Some(tm.spice_code.clone()),
                    })
            })
            .collect();
        
        let problem = Self {
            target_metrics,
            component_parameters,
            current_dir,
            netlist_dir,
            test_configs: Arc::new(ordered_test_configs),
            verbose,
            iteration_count: AtomicU64::new(0),
            simulator,
        };
        
        Ok((problem, initial_params))
    }
    
    pub fn get_iteration_count(&self) -> u64 {
        self.iteration_count.load(Ordering::SeqCst)
    }
    
    pub fn reset_iteration_count(&self) {
        self.iteration_count.store(0, Ordering::SeqCst);
    }
}

impl Clone for OptimizationProblem {
    fn clone(&self) -> Self {
        // Note: This is a simplified clone that doesn't clone the simulator
        // In practice, you might want to implement a more sophisticated cloning strategy
        panic!("OptimizationProblem clone not fully implemented - simulator backend cannot be cloned easily");
    }
}

impl CostFunction for OptimizationProblem {
    type Param = Vec<f64>;
    type Output = f64;
    
    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        let iteration = self.iteration_count.fetch_add(1, Ordering::SeqCst);
        
        vprintln!(self.verbose, "\n=== Iteration {} ===", iteration + 1);
        safe_println!("Iteration {}: Computing cost function", iteration + 1);
        
        // Validate parameters
        if param.is_empty() {
            return Ok(f64::MAX);
        }
        
        // Update simulation parameters
        if let Err(e) = self.simulator.update_parameters(param, &self.component_parameters) {
            vprintln!(self.verbose, "Parameter update failed: {}", e);
            return Ok(f64::MAX);
        }
        
        // Extract SPICE codes from target metrics
        let spice_codes: Vec<String> = self.target_metrics.iter()
            .map(|tm| tm.spice_code.clone())
            .collect();
        
        // Run simulation
        match self.simulator.run_simulation(&self.test_configs, &spice_codes) {
            Ok(metric_values) => {
                // Calculate weighted cost
                let cost: f64 = self.target_metrics.iter()
                    .zip(metric_values.iter())
                    .map(|(target, &measured)| {
                        let error = measured - target.target_value;
                        let weighted_squared_error = target.weight * error.powi(2);
                        
                        vprintln!(self.verbose, "  {}: measured={:.6e}, target={:.6e}, error={:.6e}, weight={}", 
                                 target.name, measured, target.target_value, error, target.weight);
                        
                        weighted_squared_error
                    })
                    .sum();
                
                vprintln!(self.verbose, "Total cost: {:.6e}", cost);
                safe_println!("Cost: {:.6e}", cost);
                
                Ok(cost)
            },
            Err(e) => {
                vprintln!(self.verbose, "Simulation failed: {}", e);
                Ok(f64::MAX)
            }
        }
    }
}

impl Gradient for OptimizationProblem {
    type Param = Vec<f64>;
    type Gradient = Vec<f64>;
    
    fn gradient(&self, param: &Self::Param) -> Result<Self::Gradient, argmin::core::Error> {
        const EPSILON: f64 = 1e-8;
        
        vprintln!(self.verbose, "Computing numerical gradient (ε = {:.2e})", EPSILON);
        
        let base_cost = self.cost(param)?;
        let mut gradient = Vec::with_capacity(param.len());
        
        // Sequential gradient computation (parallel version can cause simulation conflicts)
        for i in 0..param.len() {
            let mut param_plus = param.clone();
            param_plus[i] += EPSILON;
            
            let cost_plus = self.cost(&param_plus)?;
            let grad_i = (cost_plus - base_cost) / EPSILON;
            gradient.push(grad_i);
            
            vprintln!(self.verbose, "  ∂f/∂x[{}]: {:.6e}", i, grad_i);
        }
        
        let magnitude: f64 = gradient.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
        vprintln!(self.verbose, "Gradient magnitude: {:.6e}", magnitude);
        
        Ok(gradient)
    }
}
