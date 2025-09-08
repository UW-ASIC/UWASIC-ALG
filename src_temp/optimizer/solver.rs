use argmin::core::{Executor, CostFunction};
use argmin::solver::{
    neldermead::NelderMead,
    particleswarm::ParticleSwarm,
    simulatedannealing::SimulatedAnnealing,
    linesearch::HagerZhangLineSearch,
    gradientdescent::SteepestDescent,
    quasinewton::BFGS,
    conjugategradient::ConjugateGradient,
    trustregion::TrustRegion,
};
use argmin::solver::linesearch::MoreThuenteLineSearch;
use crate::optimizer::OptimizationProblem;
use crate::vprintln;

#[derive(Debug, Clone, PartialEq)]
pub enum SolverType {
    // Derivative-free methods
    NelderMead,        // Simplex method
    ParticleSwarm,     // Swarm intelligence
    SimulatedAnnealing, // Metaheuristic
    
    // Gradient-based methods  
    SteepestDescent,   // Basic gradient descent
    ConjugateGradient, // CG method
    BFGS,              // Quasi-Newton
    TrustRegion,       // Trust region method
    
    // Auto selection
    Auto,
}

impl SolverType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::NelderMead,
            Self::ParticleSwarm, 
            Self::SimulatedAnnealing,
            Self::SteepestDescent,
            Self::ConjugateGradient,
            Self::BFGS,
            Self::TrustRegion,
        ]
    }
    
    pub fn is_derivative_free(&self) -> bool {
        matches!(self, Self::NelderMead | Self::ParticleSwarm | Self::SimulatedAnnealing)
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            Self::NelderMead => "Robust simplex method for derivative-free optimization",
            Self::ParticleSwarm => "Global optimization using particle swarm intelligence", 
            Self::SimulatedAnnealing => "Probabilistic method that can escape local minima",
            Self::SteepestDescent => "Simple gradient-based method",
            Self::ConjugateGradient => "Efficient gradient method with conjugate directions",
            Self::BFGS => "Quasi-Newton method with approximate Hessian",
            Self::TrustRegion => "Robust method using trust region strategy",
            Self::Auto => "Automatically select best solver for the problem",
        }
    }
}

impl std::fmt::Display for SolverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::NelderMead => "Nelder-Mead",
            Self::ParticleSwarm => "Particle Swarm",
            Self::SimulatedAnnealing => "Simulated Annealing",
            Self::SteepestDescent => "Steepest Descent",
            Self::ConjugateGradient => "Conjugate Gradient",
            Self::BFGS => "BFGS",
            Self::TrustRegion => "Trust Region",
            Self::Auto => "Auto",
        };
        write!(f, "{}", name)
    }
}

impl std::str::FromStr for SolverType {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace(&['-', '_'][..], "").as_str() {
            "neldermead" | "simplex" => Ok(Self::NelderMead),
            "particleswarm" | "pso" => Ok(Self::ParticleSwarm),
            "simulatedannealing" | "sa" => Ok(Self::SimulatedAnnealing),
            "steepestdescent" | "gd" => Ok(Self::SteepestDescent),
            "conjugategradient" | "cg" => Ok(Self::ConjugateGradient),
            "bfgs" => Ok(Self::BFGS),
            "trustregion" | "tr" => Ok(Self::TrustRegion),
            "auto" => Ok(Self::Auto),
            _ => Err(format!("Unknown solver: '{}'. Available: {:?}", s, Self::all()))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolverConfig {
    pub solver_type: SolverType,
    pub max_iterations: u64,
    pub tolerance: f64,
    pub population_size: usize,      // For PSO, SA
    pub initial_temp: f64,           // For SA
    pub cooling_rate: f64,           // For SA
    pub perturbation: f64,           // For NelderMead
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            solver_type: SolverType::Auto,
            max_iterations: 1000,
            tolerance: 1e-6,
            population_size: 40,
            initial_temp: 1000.0,
            cooling_rate: 0.95,
            perturbation: 0.05,
        }
    }
}

impl SolverConfig {
    pub fn new(solver_type: SolverType) -> Self {
        Self { solver_type, ..Default::default() }
    }
    
    pub fn with_max_iterations(mut self, max_iter: u64) -> Self {
        self.max_iterations = max_iter;
        self
    }
    
    pub fn with_tolerance(mut self, tol: f64) -> Self {
        self.tolerance = tol;
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
    
    fn auto_select(&self, num_params: usize, noisy: bool) -> SolverType {
        vprintln!(self.verbose, "Auto-selecting solver for {} parameters, noisy: {}", num_params, noisy);
        
        let selected = match (num_params, noisy) {
            (1..=5, false) => SolverType::BFGS,              // Small, smooth problems
            (1..=5, true) => SolverType::NelderMead,         // Small, noisy problems  
            (6..=20, false) => SolverType::ConjugateGradient, // Medium, smooth problems
            (6..=20, true) => SolverType::ParticleSwarm,     // Medium, noisy problems
            (_, _) => SolverType::SimulatedAnnealing,        // Large or very noisy problems
        };
        
        vprintln!(self.verbose, "Selected: {} - {}", selected, selected.description());
        selected
    }
    
    pub fn optimize(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let solver_type = if config.solver_type == SolverType::Auto {
            self.auto_select(initial_params.len(), true) // Assume noisy for circuits
        } else {
            config.solver_type
        };
        
        vprintln!(self.verbose, "Running {} solver...", solver_type);
        
        match solver_type {
            SolverType::NelderMead => self.run_nelder_mead(problem, initial_params, config),
            SolverType::ParticleSwarm => self.run_particle_swarm(problem, initial_params, config),
            SolverType::SimulatedAnnealing => self.run_simulated_annealing(problem, initial_params, config),
            SolverType::SteepestDescent => self.run_steepest_descent(problem, initial_params, config),
            SolverType::ConjugateGradient => self.run_conjugate_gradient(problem, initial_params, config),
            SolverType::BFGS => self.run_bfgs(problem, initial_params, config),
            SolverType::TrustRegion => self.run_trust_region(problem, initial_params, config),
            SolverType::Auto => unreachable!(),
        }
    }
    
    fn run_nelder_mead(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let n = initial_params.len();
        let mut simplex = vec![initial_params.clone()];
        
        // Create simplex vertices
        for i in 0..n {
            let mut vertex = initial_params.clone();
            vertex[i] += config.perturbation * vertex[i].abs().max(1e-8);
            simplex.push(vertex);
        }
        
        let solver = NelderMead::new(simplex)
            .with_sd_tolerance(config.tolerance)?;
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "Completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_particle_swarm(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        // Create bounds - assume ±50% of initial values
        let bounds = initial_params.iter()
            .map(|&x| (x * 0.5, x * 1.5))
            .collect();
        
        let solver = ParticleSwarm::new(bounds, config.population_size);
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "PSO completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_simulated_annealing(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let solver = SimulatedAnnealing::new(config.initial_temp)?
            .with_reannealing_best(10)
            .with_reannealing_accepted(100);
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "SA completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_steepest_descent(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let linesearch = HagerZhangLineSearch::new();
        let solver = SteepestDescent::new(linesearch);
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "Steepest Descent completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_conjugate_gradient(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let linesearch = MoreThuenteLineSearch::new();
        let solver = ConjugateGradient::new(linesearch);
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "CG completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_bfgs(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let linesearch = HagerZhangLineSearch::new();
        let solver = BFGS::new(linesearch);
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "BFGS completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
    
    fn run_trust_region(
        &self,
        problem: OptimizationProblem,
        initial_params: Vec<f64>,
        config: SolverConfig,
    ) -> Result<(Vec<f64>, f64, u64), String> {
        let solver = TrustRegion::new();
        
        let result = Executor::new(problem, solver)
            .configure(|state| state.param(initial_params).max_iters(config.max_iterations))
            .run()?;
        
        let params = result.state.best_param.unwrap_or_default();
        let cost = result.state.best_cost;
        let iters = result.state.iter;
        
        vprintln!(self.verbose, "Trust Region completed in {} iterations, cost: {:.6e}", iters, cost);
        Ok((params, cost, iters))
    }
}

// Convenience functions
pub fn list_solvers() -> Vec<(SolverType, &'static str)> {
    SolverType::all()
        .into_iter()
        .map(|s| (s.clone(), s.description()))
        .collect()
}

pub fn recommend_solver(num_params: usize, is_noisy: bool, needs_global: bool) -> SolverType {
    match (num_params, is_noisy, needs_global) {
        (_, _, true) => SolverType::SimulatedAnnealing,     // Global optimization needed
        (1..=5, false, false) => SolverType::BFGS,          // Small smooth problems
        (1..=10, true, false) => SolverType::NelderMead,    // Small noisy problems
        (11..=50, false, false) => SolverType::ConjugateGradient, // Medium smooth problems
        (11..=50, true, false) => SolverType::ParticleSwarm, // Medium noisy problems
        (_, _, false) => SolverType::SimulatedAnnealing,    // Large problems
    }
}
