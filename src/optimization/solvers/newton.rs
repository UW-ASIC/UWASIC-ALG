use super::traits::{OptimizationCallback, Problem, Solver, SolverResult};

/// Adaptive Newton optimizer with Armijo line search and learning rate adaptation
pub struct NewtonOptimizer {
    max_iter: u32,
    precision: f64,
    learning_rate: f64,
    min_learning_rate: f64,
    max_learning_rate: f64,
    armijo_c: f64,         // Armijo condition parameter
    backtrack_factor: f64, // Line search backtracking
    increase_factor: f64,  // Learning rate increase when successful
}

impl NewtonOptimizer {
    pub fn new(max_iter: u32, precision: f64) -> Self {
        Self {
            max_iter,
            precision,
            learning_rate: 0.1,
            min_learning_rate: 1e-6,
            max_learning_rate: 1.0,
            armijo_c: 1e-4,
            backtrack_factor: 0.5,
            increase_factor: 1.2,
        }
    }

    pub fn with_learning_rate(mut self, learning_rate: f64) -> Self {
        self.learning_rate = learning_rate;
        self
    }

    #[inline]
    fn clamp_params(&self, params: &mut [f64], bounds: &[(f64, f64)]) {
        for (i, &(min, max)) in bounds.iter().enumerate() {
            params[i] = params[i].clamp(min, max);
        }
    }

    /// Compute numerical gradient using central finite differences
    fn compute_gradient(
        &self,
        problem: &dyn Problem,
        params: &[f64],
        grad: &mut [f64],
        cost_evals: &mut usize,
    ) -> Result<(), String> {
        let h = 1e-6;
        let n = params.len();

        for i in 0..n {
            let mut p_plus = params.to_vec();
            let mut p_minus = params.to_vec();

            p_plus[i] += h;
            p_minus[i] -= h;

            let c_plus = problem.cost(&p_plus)?;
            *cost_evals += 1;

            let c_minus = problem.cost(&p_minus)?;
            *cost_evals += 1;

            grad[i] = (c_plus - c_minus) / (2.0 * h);
        }

        Ok(())
    }

    /// Armijo line search with backtracking
    fn line_search(
        &self,
        problem: &dyn Problem,
        params: &[f64],
        gradient: &[f64],
        current_cost: f64,
        cost_evals: &mut usize,
        bounds: &[(f64, f64)],
    ) -> Result<f64, String> {
        let mut alpha = self.learning_rate;
        let grad_norm_sq: f64 = gradient.iter().map(|&g| g * g).sum();

        // Try up to 10 backtracking steps
        for _ in 0..10 {
            let mut new_params = params.to_vec();

            // Take step
            for i in 0..params.len() {
                new_params[i] -= alpha * gradient[i];
            }

            self.clamp_params(&mut new_params, bounds);

            let new_cost = problem.cost(&new_params)?;
            *cost_evals += 1;

            // Armijo condition: sufficient decrease
            if new_cost <= current_cost - self.armijo_c * alpha * grad_norm_sq {
                return Ok(alpha);
            }

            // Backtrack
            alpha *= self.backtrack_factor;

            if alpha < self.min_learning_rate {
                break;
            }
        }

        Ok(alpha.max(self.min_learning_rate))
    }
}

impl Solver for NewtonOptimizer {
    fn name(&self) -> &str {
        "AdaptiveNewton"
    }

    fn solve(
        &mut self,
        problem: &dyn Problem,
        callback: &mut dyn OptimizationCallback,
    ) -> Result<SolverResult, String> {
        let n = problem.num_params();
        let bounds = problem.bounds();

        let mut params = problem.initial_params().to_vec();
        let mut gradient = vec![0.0; n];

        let mut cost_evals = 0;
        let mut grad_evals = 0;
        let mut prev_cost = f64::INFINITY;
        let mut consecutive_improvements = 0;

        for iter in 0..self.max_iter {
            problem.apply_constraints(&mut params)?;
            self.clamp_params(&mut params, bounds);

            let cost = problem.cost(&params)?;
            cost_evals += 1;

            callback.on_iteration(iter + 1, &params, cost)?;

            if callback.should_stop() {
                return Ok(SolverResult {
                    success: true,
                    cost,
                    iterations: iter + 1,
                    message: "Stopped by callback".into(),
                    params,
                    cost_evals,
                    grad_evals,
                });
            }

            if cost < self.precision {
                return Ok(SolverResult {
                    success: true,
                    cost,
                    iterations: iter + 1,
                    message: "Converged".into(),
                    params,
                    cost_evals,
                    grad_evals,
                });
            }

            if (prev_cost - cost).abs() < self.precision * 0.01 {
                return Ok(SolverResult {
                    success: false,
                    cost,
                    iterations: iter + 1,
                    message: "Stagnated".into(),
                    params,
                    cost_evals,
                    grad_evals,
                });
            }

            // Adapt learning rate based on progress
            if cost < prev_cost {
                consecutive_improvements += 1;
                // Increase learning rate if consistently improving
                if consecutive_improvements >= 3 {
                    self.learning_rate =
                        (self.learning_rate * self.increase_factor).min(self.max_learning_rate);
                }
            } else {
                consecutive_improvements = 0;
            }

            prev_cost = cost;

            // Compute gradient
            self.compute_gradient(problem, &params, &mut gradient, &mut cost_evals)?;
            grad_evals += 1;

            // Adaptive line search
            let step_size =
                self.line_search(problem, &params, &gradient, cost, &mut cost_evals, bounds)?;

            // Update parameters with adaptive step size
            for i in 0..n {
                params[i] -= step_size * gradient[i];
            }

            self.clamp_params(&mut params, bounds);
        }

        Ok(SolverResult {
            success: false,
            cost: prev_cost,
            iterations: self.max_iter,
            message: "Max iterations reached".into(),
            params,
            cost_evals,
            grad_evals,
        })
    }
}
