pub mod problem;
pub mod simulation;
pub mod solver;

// Re-export main types for easier access
pub use problem::{OptimizationProblem, TargetMetric, ComponentParameter, SimulationBackend};
pub use simulation::NgSpiceBackend;
pub use solver::{SolverManager, SolverConfig, SolverType, list_solvers, recommend_solver};

use std::path::PathBuf;
use argmin::core::CostFunction;
use std::collections::HashMap;
use std::time::Instant;
use crate::pyinterface::TestConfiguration;
use crate::{vprintln, safe_println};

/// Main circuit optimizer that coordinates all components
pub struct CircuitOptimizer {
    solver_manager: SolverManager,
    verbose: bool,
}

/// Optimization result containing all relevant information
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub optimized_params: HashMap<String, HashMap<String, f64>>,
    pub final_metrics: HashMap<String, f64>,
    pub final_cost: f64,
    pub iterations_completed: u64,
    pub convergence_achieved: bool,
    pub execution_time_ms: u64,
    pub solver_diagnostics: String,
    pub solver_used: String,
}

impl CircuitOptimizer {
    pub fn new(verbose: bool) -> Self {
        Self {
            solver_manager: SolverManager::new(verbose),
            verbose,
        }
    }
    
    /// Run optimization with automatic solver selection
    pub fn optimize_auto(
        &self,
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        test_configs: HashMap<String, TestConfiguration>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        max_iterations: u64,
        tolerance: f64,
    ) -> Result<OptimizationResult, String> {
        let config = SolverConfig::default()
            .with_max_iterations(max_iterations)
            .with_tolerance(tolerance);
        
        self.optimize_with_config(
            target_metrics,
            component_data,
            test_configs,
            current_dir,
            netlist_dir,
            config,
        )
    }
    
    /// Run optimization with specific solver
    pub fn optimize_with_solver(
        &self,
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        test_configs: HashMap<String, TestConfiguration>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        solver_type: SolverType,
        max_iterations: u64,
        tolerance: f64,
    ) -> Result<OptimizationResult, String> {
        let config = SolverConfig::new(solver_type)
            .with_max_iterations(max_iterations)
            .with_tolerance(tolerance);
        
        self.optimize_with_config(
            target_metrics,
            component_data,
            test_configs,
            current_dir,
            netlist_dir,
            config,
        )
    }
    
    /// Run optimization with full configuration control
    pub fn optimize_with_config(
        &self,
        target_metrics: Vec<TargetMetric>,
        component_data: Vec<(String, HashMap<String, f64>)>,
        test_configs: HashMap<String, TestConfiguration>,
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        solver_config: SolverConfig,
    ) -> Result<OptimizationResult, String> {
        let start_time = Instant::now();
        
        safe_println!("Starting circuit optimization");
        safe_println!("  Targets: {}", target_metrics.len());
        safe_println!("  Components: {}", component_data.len());
        safe_println!("  Solver: {}", solver_config.solver_type);
        safe_println!("  Max iterations: {}", solver_config.max_iterations);
        
        // Create simulation backend
        vprintln!(self.verbose, "Initializing NgSpice simulation backend...");
        let backend = Box::new(NgSpiceBackend::new(
            current_dir.clone(),
            netlist_dir.clone(),
            self.verbose,
        )?);
        
        // Create optimization problem
        vprintln!(self.verbose, "Creating optimization problem...");
        let (problem, initial_params) = OptimizationProblem::new(
            target_metrics.clone(),
            component_data.clone(),
            test_configs,
            current_dir,
            netlist_dir,
            backend,
            self.verbose,
        )?;
        
        vprintln!(self.verbose, "Initial parameters: {:?}", initial_params);
        safe_println!("  Initial parameters: {}", initial_params.len());
        
        // Validate initial cost
        vprintln!(self.verbose, "Computing initial cost...");
        let initial_cost = problem.cost(&initial_params)
            .map_err(|e| format!("Failed to compute initial cost: {}", e))?;
        
        safe_println!("  Initial cost: {:.6e}", initial_cost);
        
        if !initial_cost.is_finite() {
            return Err("Initial cost is not finite - check your setup".to_string());
        }
        
        // Run optimization using the solver manager
        vprintln!(self.verbose, "Starting optimization with solver manager...");
        let (final_params, final_cost, iterations) = self.solver_manager.optimize(
            problem,
            initial_params.clone(),
            solver_config.clone(),
        )?;
        
        let execution_time = start_time.elapsed();
        
        // Process results
        safe_println!("Optimization completed!");
        safe_println!("  Final cost: {:.6e}", final_cost);
        safe_println!("  Iterations: {}", iterations);
        safe_println!("  Time: {:.2}s", execution_time.as_secs_f64());
        
        // Convert final parameters back to component structure
        let optimized_params = self.reconstruct_component_params(
            &final_params,
            &component_data,
        );
        
        // Check convergence (simple heuristic)
        let convergence_achieved = final_cost < solver_config.tolerance || 
                                 iterations < solver_config.max_iterations;
        
        // Build result
        let result = OptimizationResult {
            optimized_params,
            final_metrics: HashMap::new(), // Could be populated from final simulation
            final_cost,
            iterations_completed: iterations,
            convergence_achieved,
            execution_time_ms: execution_time.as_millis() as u64,
            solver_diagnostics: format!("Solver: {}, Final cost: {:.6e}", 
                                       solver_config.solver_type, final_cost),
            solver_used: solver_config.solver_type.to_string(),
        };
        
        Ok(result)
    }
    
    /// Reconstruct component parameters from flat parameter vector
    fn reconstruct_component_params(
        &self,
        params: &[f64],
        component_data: &[(String, HashMap<String, f64>)],
    ) -> HashMap<String, HashMap<String, f64>> {
        let mut result = HashMap::new();
        let mut param_index = 0;
        
        for (component_name, properties) in component_data {
            let mut component_params = HashMap::new();
            
            // Sort properties for consistent ordering (same as in problem creation)
            let mut sorted_props: Vec<_> = properties.iter().collect();
            sorted_props.sort_by_key(|(k, _)| *k);
            
            for (property_name, _) in sorted_props {
                if let Some(&param_value) = params.get(param_index) {
                    component_params.insert(property_name.clone(), param_value);
                    param_index += 1;
                }
            }
            
            result.insert(component_name.clone(), component_params);
        }
        
        result
    }
}
