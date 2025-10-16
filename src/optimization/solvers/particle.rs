use super::traits::{OptimizationCallback, Problem, Solver, SolverResult};
use rand::Rng;

/// Particle Swarm Optimization - often outperforms gradient-based methods
/// for noisy, non-convex problems with fewer cost evaluations
pub struct ParticleOptimizer {
    max_iter: u32,
    precision: f64,
    population_size: usize,
    inertia: f64,   // w - velocity inertia weight
    cognitive: f64, // c1 - personal best influence
    social: f64,    // c2 - global best influence
}

impl ParticleOptimizer {
    pub fn new(max_iter: u32, precision: f64) -> Self {
        Self {
            max_iter,
            precision,
            population_size: 20,
            inertia: 0.7,
            cognitive: 1.5,
            social: 1.5,
        }
    }

    /// Configure swarm size (default: 20)
    pub fn with_population_size(mut self, size: usize) -> Self {
        self.population_size = size;
        self
    }

    /// Configure PSO parameters (defaults: w=0.7, c1=1.5, c2=1.5)
    pub fn with_pso_params(mut self, inertia: f64, cognitive: f64, social: f64) -> Self {
        self.inertia = inertia;
        self.cognitive = cognitive;
        self.social = social;
        self
    }

    #[inline]
    fn clamp_params(&self, params: &mut [f64], bounds: &[(f64, f64)]) {
        for (i, &(min, max)) in bounds.iter().enumerate() {
            params[i] = params[i].clamp(min, max);
        }
    }

    /// Initialize particle positions uniformly within bounds
    fn initialize_particles(
        &self,
        n_params: usize,
        bounds: &[(f64, f64)],
        initial_params: &[f64],
    ) -> Vec<Vec<f64>> {
        let mut rng = rand::thread_rng();
        let mut particles = Vec::with_capacity(self.population_size);

        // First particle is the provided initial guess
        particles.push(initial_params.to_vec());

        // Rest are random within bounds
        for _ in 1..self.population_size {
            let mut particle = vec![0.0; n_params];
            for i in 0..n_params {
                let (min, max) = bounds[i];
                particle[i] = rng.gen_range(min..=max);
            }
            particles.push(particle);
        }

        particles
    }

    /// Initialize velocities (small random values)
    fn initialize_velocities(&self, n_params: usize, bounds: &[(f64, f64)]) -> Vec<Vec<f64>> {
        let mut rng = rand::thread_rng();
        let mut velocities = Vec::with_capacity(self.population_size);

        for _ in 0..self.population_size {
            let mut velocity = vec![0.0; n_params];
            for i in 0..n_params {
                let (min, max) = bounds[i];
                let range = max - min;
                // Initialize velocity to small fraction of parameter range
                velocity[i] = rng.gen_range(-range * 0.1..=range * 0.1);
            }
            velocities.push(velocity);
        }

        velocities
    }
}

impl Solver for ParticleOptimizer {
    fn name(&self) -> &str {
        "PSO"
    }

    fn solve(
        &mut self,
        problem: &dyn Problem,
        callback: &mut dyn OptimizationCallback,
    ) -> Result<SolverResult, String> {
        let n = problem.num_params();
        let bounds = problem.bounds();
        let mut rng = rand::thread_rng();

        // Initialize swarm
        let mut particles = self.initialize_particles(n, bounds, problem.initial_params());
        let mut velocities = self.initialize_velocities(n, bounds);
        let mut personal_best_positions = particles.clone();
        let mut personal_best_costs = vec![f64::INFINITY; self.population_size];

        let mut global_best_idx = 0;
        let mut global_best_cost = f64::INFINITY;

        let mut cost_evals = 0;
        let mut stagnation_counter = 0;
        const MAX_STAGNATION: u32 = 5;

        // Main optimization loop
        for iter in 0..self.max_iter {
            let prev_global_best = global_best_cost;

            // Evaluate all particles
            for p in 0..self.population_size {
                // Apply constraints and bounds
                problem.apply_constraints(&mut particles[p])?;
                self.clamp_params(&mut particles[p], bounds);

                // Evaluate cost (THIS RUNS SIMULATION)
                let cost = problem.cost(&particles[p])?;
                cost_evals += 1;

                // Update personal best
                if cost < personal_best_costs[p] {
                    personal_best_costs[p] = cost;
                    personal_best_positions[p].copy_from_slice(&particles[p]);
                }

                // Update global best
                if cost < global_best_cost {
                    global_best_cost = cost;
                    global_best_idx = p;
                }
            }

            // Report progress using the global best
            callback.on_iteration(iter + 1, &personal_best_positions[global_best_idx], global_best_cost)?;

            // Check for early termination
            if callback.should_stop() {
                return Ok(SolverResult {
                    success: true,
                    cost: global_best_cost,
                    iterations: iter + 1,
                    message: "Stopped by callback".into(),
                    params: personal_best_positions[global_best_idx].clone(),
                    cost_evals,
                    grad_evals: 0,
                });
            }

            // Check convergence
            if global_best_cost < self.precision {
                return Ok(SolverResult {
                    success: true,
                    cost: global_best_cost,
                    iterations: iter + 1,
                    message: "Converged".into(),
                    params: personal_best_positions[global_best_idx].clone(),
                    cost_evals,
                    grad_evals: 0,
                });
            }

            // Check for stagnation
            if (prev_global_best - global_best_cost).abs() < self.precision * 0.01 {
                stagnation_counter += 1;
                if stagnation_counter >= MAX_STAGNATION {
                    return Ok(SolverResult {
                        success: false,
                        cost: global_best_cost,
                        iterations: iter + 1,
                        message: "Stagnated".into(),
                        params: personal_best_positions[global_best_idx].clone(),
                        cost_evals,
                        grad_evals: 0,
                    });
                }
            } else {
                stagnation_counter = 0;
            }

            // Update velocities and positions for all particles
            for p in 0..self.population_size {
                for i in 0..n {
                    let r1 = rng.gen::<f64>();
                    let r2 = rng.gen::<f64>();

                    // PSO velocity update equation
                    velocities[p][i] = self.inertia * velocities[p][i]
                        + self.cognitive * r1 * (personal_best_positions[p][i] - particles[p][i])
                        + self.social * r2 * (personal_best_positions[global_best_idx][i] - particles[p][i]);

                    // Clamp velocity to fraction of search space
                    let (min, max) = bounds[i];
                    let v_max = (max - min) * 0.2;
                    velocities[p][i] = velocities[p][i].clamp(-v_max, v_max);

                    // Update position
                    particles[p][i] += velocities[p][i];
                }

                // Clamp to bounds
                self.clamp_params(&mut particles[p], bounds);
            }
        }

        // Max iterations reached
        Ok(SolverResult {
            success: false,
            cost: global_best_cost,
            iterations: self.max_iter,
            message: "Max iterations reached".into(),
            params: personal_best_positions[global_best_idx].clone(),
            cost_evals,
            grad_evals: 0,
        })
    }
}
