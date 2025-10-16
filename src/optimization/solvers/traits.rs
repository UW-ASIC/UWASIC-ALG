#[derive(Clone, Debug)]
pub struct SolverResult {
    pub success: bool,
    pub cost: f64,
    pub iterations: u32,
    pub message: String,
    pub params: Vec<f64>,
    pub cost_evals: usize,
    pub grad_evals: usize,
}

/// Callback interface for optimization progress
pub trait OptimizationCallback {
    /// Called at each iteration with current parameters and cost
    fn on_iteration(&mut self, iteration: u32, params: &[f64], cost: f64) -> Result<(), String>;

    /// Check if optimization should stop early
    fn should_stop(&self) -> bool {
        false
    }
}

/// Core problem definition - just the essentials
pub trait Problem {
    /// Evaluate cost for given parameters (runs simulation)
    fn cost(&self, params: &[f64]) -> Result<f64, String>;

    /// Number of parameters
    fn num_params(&self) -> usize;

    /// Initial parameter values
    fn initial_params(&self) -> &[f64];

    /// Parameter bounds (min, max) for each parameter
    fn bounds(&self) -> &[(f64, f64)];

    /// Apply constraints to parameters (modifies params in place)
    fn apply_constraints(&self, params: &mut [f64]) -> Result<(), String>;
}

/// Solver interface - takes problem and callback
pub trait Solver {
    fn name(&self) -> &str;

    /// Solve the optimization problem with callback for progress tracking
    fn solve(
        &mut self,
        problem: &dyn Problem,
        callback: &mut dyn OptimizationCallback,
    ) -> Result<SolverResult, String>;
}

// ============================================================================
// GUIDE: CREATING NEW OPTIMIZERS
// ============================================================================
//
// To create a new optimizer (e.g., Gradient Descent, PSO, Genetic Algorithm):
//
// 1. CREATE A STRUCT FOR YOUR OPTIMIZER
//    Store hyperparameters and internal state:
//
//    pub struct MyOptimizer {
//        max_iter: u32,
//        learning_rate: f64,
//        // ... other hyperparameters ...
//    }
//
// 2. IMPLEMENT THE SOLVER TRAIT
//    The solve() method is where your optimization algorithm lives:
//
//    impl Solver for MyOptimizer {
//        fn name(&self) -> &str {
//            "MyOptimizer"
//        }
//
//        fn solve(
//            &mut self,
//            problem: &dyn Problem,
//            callback: &mut dyn OptimizationCallback,
//        ) -> Result<SolverResult, String> {
//            // Your optimization loop here
//        }
//    }
//
// 3. OPTIMIZATION LOOP PATTERN
//    Follow this standard pattern in your solve() method:
//
//    fn solve(&mut self, problem: &dyn Problem, callback: &mut dyn OptimizationCallback)
//        -> Result<SolverResult, String>
//    {
//        // Initialize
//        let n = problem.num_params();
//        let bounds = problem.bounds();
//        let mut params = problem.initial_params().to_vec();
//        let mut cost_evals = 0;
//
//        // Main loop
//        for iter in 0..self.max_iter {
//            // Step 1: Apply constraints and bounds
//            problem.apply_constraints(&mut params)?;
//            clamp_to_bounds(&mut params, bounds);
//
//            // Step 2: Evaluate cost (THIS TRIGGERS SIMULATION)
//            let cost = problem.cost(&params)?;
//            cost_evals += 1;
//
//            // Step 3: Notify callback (displays progress, tracks history)
//            callback.on_iteration(iter + 1, &params, cost)?;
//
//            // Step 4: Check stopping conditions
//            if callback.should_stop() {
//                return Ok(SolverResult {
//                    success: true,
//                    cost,
//                    iterations: iter + 1,
//                    message: "Stopped by callback".into(),
//                    params,
//                    cost_evals,
//                    grad_evals: 0, // or your count
//                });
//            }
//
//            // Step 5: Check convergence (your criteria)
//            if cost < self.tolerance {
//                return Ok(SolverResult {
//                    success: true,
//                    cost,
//                    iterations: iter + 1,
//                    message: "Converged".into(),
//                    params,
//                    cost_evals,
//                    grad_evals: 0,
//                });
//            }
//
//            // Step 6: UPDATE PARAMETERS (your algorithm-specific logic)
//            // This is where your optimization algorithm does its work
//            update_params_using_your_algorithm(&mut params, problem, &mut cost_evals)?;
//        }
//
//        // Return final result
//        Ok(SolverResult { /* ... */ })
//    }
//
// 4. IMPORTANT NOTES:
//    - problem.cost() is EXPENSIVE - it runs a full circuit simulation
//    - Call problem.cost() as few times as possible per iteration
//    - Always call callback.on_iteration() after evaluating cost
//    - Always apply constraints before evaluating cost
//    - Track cost_evals to report how many simulations were run
//    - Use callback.should_stop() to respect max iteration limits
//
// 5. EXAMPLE OPTIMIZERS TO IMPLEMENT:
//
//    A) Gradient Descent with Line Search:
//       - Compute gradient (2*N cost calls)
//       - Use line search to find optimal step size (3-5 cost calls)
//       - Update: params -= learning_rate * gradient
//
//    B) Particle Swarm Optimization (PSO):
//       - Initialize particle swarm (population of param sets)
//       - Each iteration: evaluate all particles (population_size cost calls)
//       - Update velocities based on personal/global best
//       - Update positions: params += velocity
//
//    C) Nelder-Mead Simplex:
//       - Maintain simplex of N+1 points
//       - Evaluate all points (N+1 cost calls per iteration)
//       - Reflect/expand/contract simplex based on function values
//
//    D) Simulated Annealing:
//       - Generate random neighbor (perturb params)
//       - Evaluate neighbor (1 cost call)
//       - Accept/reject based on Metropolis criterion
//       - Decrease temperature over iterations
//
//    E) BFGS / L-BFGS:
//       - Compute gradient (2*N cost calls)
//       - Approximate Hessian using gradient history
//       - Update: params -= H^-1 * gradient
//
// 6. REGISTERING YOUR OPTIMIZER:
//    Add to mod.rs in the optimize() method:
//
//    let mut solver: Box<dyn Solver> = match self.solver.as_str() {
//        "newton" => Box::new(NewtonOptimizer::new(self.max_iter, self.precision)),
//        "myopt" => Box::new(MyOptimizer::new(self.max_iter, ...)),
//        _ => Box::new(NewtonOptimizer::new(self.max_iter, self.precision)),
//    };
//
// 7. TESTING YOUR OPTIMIZER:
//    - Start with simple problems (fewer parameters)
//    - Monitor cost_evals - keep them reasonable
//    - Test convergence on known solutions
//    - Compare against Newton optimizer as baseline
//
// ============================================================================
