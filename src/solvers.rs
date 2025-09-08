use argmin::core::Executor;
use argmin::solver::neldermead::NelderMead;
use crate::optimizer::OptimizationProblem;
use crate::vprintln;

#[derive(Debug, Clone, PartialEq)]
pub enum SolverType {
    NelderMead,        // Direct search, derivative-free
    Auto,              // Automatically select best solver (currently defaults to NelderMead)
}

impl SolverType {
    pub fn all_available() -> Vec<SolverType> {
        vec![
            SolverType::NelderMead,
        ]
    }
    
    pub fn derivative_free() -> Vec<SolverType> {
        vec![
            SolverType::NelderMead,
        ]
    }
    
    pub fn requires_gradients(&self) -> bool {
        false // All current solvers are derivative-free
    }
    
    pub fn supports_multidimensional(&self) -> bool {
        true // All current solvers support multidimensional optimization
    }
    
    pub fn description(&self) -> &str {
        match self {
            SolverType::NelderMead => "Nelder-Mead simplex method - robust, derivative-free, good for noisy functions",
            SolverType::Auto => "Automatically select the best solver based on problem characteristics (currently NelderMead)",
        }
    }
}

impl std::fmt::Display for SolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            SolverType::NelderMead => "Nelder-Mead",
            SolverType::Auto => "Auto-Select",
        };
        write!(f, "{}", name)
    }
}

impl std::str::FromStr for SolverType {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "neldermead" | "nelder-mead" | "nelder_mead" | "simplex" => Ok(SolverType::NelderMead),
            "auto" | "automatic" | "best" => Ok(SolverType::Auto),
            _ => Err(format!("Unknown solver type: '{}'. Available: NelderMead, Auto", s))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolverConfig {
    pub solver_type: SolverType,
    pub tolerance: f64,
    pub max_iterations: u64,
    pub perturbation_factor: f64,
    pub min_perturbation: f64,
    pub population_size: Option<usize>, // For PSO, SA
    pub initial_temperature: Option<f64>, // For SA
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            solver_type: SolverType::Auto,
            tolerance: 1e-6,
            max_iterations: 1000,
            perturbation_factor: 0.05,
            min_perturbation: 1e-8,
            population_size: Some(20),
            initial_temperature: Some(1000.0),
        }
    }
}

impl SolverConfig {
    pub fn new(solver_type: SolverType) -> Self {
        Self {
            solver_type,
            ..Default::default()
        }
    }
    
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }
    
    pub fn with_max_iterations(mut self, max_iterations: u64) -> Self {
        self.max_iterations = max_iterations;
        self
    }
    
    pub fn with_population_size(mut self, size: usize) -> Self {
        self.population_size = Some(size);
        self
    }
    
    pub fn with_initial_temperature(mut self, temp: f64) -> Self {
        self.initial_temperature = Some(temp);
        self
    }
}

pub struct SolverManager {
    verbose: bool,
}

impl SolverManager {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }
    
    pub fn auto_select_solver(
        &self, 
        num_params: usize, 
        has_noise: bool, 
        is_multimodal: bool
    ) -> SolverType {
        vprintln!(self.verbose, "🤖 Auto-selecting solver based on problem characteristics:");
        vprintln!(self.verbose, "  Parameters: {}", num_params);
        vprintln!(self.verbose, "  Noisy function: {}", has_noise);
        vprintln!(self.verbose, "  Multimodal: {}", is_multimodal);
        
        // For now, always select Nelder-Mead as it's the most reliable
        let selected = SolverType::NelderMead;
        vprintln!(self.verbose, "  → Using robust derivative-free method for circuit optimization");
        
        vprintln!(self.verbose, "  ✓ Selected: {} - {}", selected, selected.description());
        selected
    }
    
    pub fn run_optimization(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let solver_type = if config.solver_type == SolverType::Auto {
            self.auto_select_solver(initial_params.len(), true, false) // Assume noisy for circuit optimization
        } else {
            config.solver_type.clone()
        };
        
        vprintln!(self.verbose, "\n🚀 Starting optimization with {} solver", solver_type);
        vprintln!(self.verbose, "  Tolerance: {:.2e}", config.tolerance);
        vprintln!(self.verbose, "  Max iterations: {}", config.max_iterations);
        
        match solver_type {
            SolverType::NelderMead => self.run_nelder_mead(problem, initial_params, &config),
            SolverType::Auto => unreachable!("Auto should be resolved above"),
        }
    }
    
    fn run_nelder_mead(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: &SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        vprintln!(self.verbose, "🔺 Setting up Nelder-Mead solver...");
        
        let n = initial_params.len();
        let mut simplex = vec![initial_params.clone()];
        
        for i in 0..n {
            let mut vertex = initial_params.clone();
            let perturbation = config.perturbation_factor * vertex[i].abs().max(config.min_perturbation);
            vertex[i] += perturbation;
            simplex.push(vertex);
        }
        
        let solver = NelderMead::new(simplex)
            .with_sd_tolerance(config.tolerance)
            .map_err(|e| format!("Failed to create Nelder-Mead solver: {}", e))?;
        
        let result = Executor::new(problem, solver)
            .configure(|state| {
                state
                    .param(initial_params)
                    .max_iters(config.max_iterations)
            })
            .run()
            .map_err(|e| format!("Optimization failed: {}", e))?;
        
        let best_params = result.state.best_param.unwrap_or_default();
        let best_cost = result.state.best_cost;
        let iterations = result.state.iter;
        
        vprintln!(self.verbose, "✓ Nelder-Mead completed in {} iterations", iterations);
        vprintln!(self.verbose, "  Final cost: {:.6e}", best_cost);
        
        Ok((best_params, best_cost, iterations))
    }
    
}

pub fn list_available_solvers() -> Vec<(SolverType, String)> {
    SolverType::all_available()
        .into_iter()
        .map(|s| (s.clone(), s.description().to_string()))
        .collect()
}

pub fn recommend_solver_for_problem(
    num_params: usize,
    has_noise: bool,
    is_multimodal: bool,
    requires_global: bool,
) -> Vec<SolverType> {
    let manager = SolverManager::new(false);
    let primary = manager.auto_select_solver(num_params, has_noise, is_multimodal);
    
    let mut recommendations = vec![primary.clone()];
    
    // For now, only NelderMead is supported, so no additional recommendations
    
    if !recommendations.contains(&SolverType::NelderMead) {
        recommendations.push(SolverType::NelderMead); // Always include as fallback
    }
    
    recommendations
}