use super::traits::{OptimizationCallback, Problem, Solver, SolverResult};
use rand_distr::{Distribution, StandardNormal};

pub struct CMAESOptimizer {
    max_iter: u32,
    precision: f64,
    population_size: usize,
    sigma: f64,
}

impl CMAESOptimizer {
    pub fn new(max_iter: u32, precision: f64) -> Self {
        Self {
            max_iter,
            precision,
            population_size: 0,
            sigma: 0.3,
        }
    }

    pub fn with_population_size(mut self, size: usize) -> Self {
        self.population_size = size;
        self
    }

    pub fn with_sigma(mut self, sigma: f64) -> Self {
        self.sigma = sigma;
        self
    }

    #[inline]
    fn clamp_params(&self, params: &mut [f64], bounds: &[(f64, f64)]) {
        for (i, &(min, max)) in bounds.iter().enumerate() {
            params[i] = params[i].clamp(min, max);
        }
    }
}

impl Solver for CMAESOptimizer {
    fn name(&self) -> &str {
        "CMA-ES"
    }

    fn solve(
        &mut self,
        problem: &dyn Problem,
        callback: &mut dyn OptimizationCallback,
    ) -> Result<SolverResult, String> {
        let n = problem.num_params();
        let bounds = problem.bounds();
        let mut rng = rand::thread_rng();

        // Set population size if not specified
        if self.population_size == 0 {
            self.population_size = 4 + (3.0 * (n as f64).ln()).floor() as usize;
        }

        let lambda = self.population_size;
        let mu = lambda / 2;

        // Initialize mean
        let mut mean = problem.initial_params().to_vec();

        // Covariance matrix - explicitly typed as f64
        let mut C: Vec<Vec<f64>> = vec![vec![0.0_f64; n]; n];
        for i in 0..n {
            C[i][i] = 1.0;
        }

        // Step size and evolution paths
        let mut sigma = self.sigma;
        let mut ps: Vec<f64> = vec![0.0; n];
        let mut pc: Vec<f64> = vec![0.0; n];

        // Strategy parameters
        let cc = 4.0 / (n as f64 + 4.0);
        let cs = 4.0 / (n as f64 + 4.0);
        let c1 = 2.0 / ((n as f64 + 1.3).powi(2));
        let cmu = 2.0 * (mu as f64 - 2.0 + 1.0 / mu as f64) / ((n as f64 + 2.0).powi(2));
        let damps =
            1.0 + 2.0 * (0.0_f64).max((((mu - 1) as f64) / (n as f64 + 1.0)).sqrt() - 1.0) + cs;

        // Recombination weights
        let mut weights = vec![0.0; mu];
        for i in 0..mu {
            weights[i] = ((mu as f64 + 0.5).ln() - (i as f64 + 1.0).ln()).max(0.0);
        }
        let sum_weights: f64 = weights.iter().sum();
        for w in weights.iter_mut() {
            *w /= sum_weights;
        }

        let mut cost_evals = 0;
        let mut best_cost = f64::INFINITY;
        let mut best_params = mean.clone();

        for iter in 0..self.max_iter {
            // Generate and evaluate population
            let mut population = Vec::with_capacity(lambda);
            let mut costs = Vec::with_capacity(lambda);

            for _ in 0..lambda {
                // Sample from standard normal
                let z: Vec<f64> = (0..n).map(|_| StandardNormal.sample(&mut rng)).collect();

                // Transform: y = mean + sigma * C^(1/2) * z
                // Simplified approach: use diagonal approximation
                let mut offspring = mean.clone();
                for i in 0..n {
                    let mut ci_z = 0.0_f64;
                    for j in 0..n {
                        // Use diagonal and near-diagonal elements
                        let c_ij: f64 = C[i][j];
                        ci_z += c_ij.abs().sqrt() * z[j];
                    }
                    offspring[i] += sigma * ci_z;
                }

                self.clamp_params(&mut offspring, bounds);
                problem.apply_constraints(&mut offspring)?;

                let cost = problem.cost(&offspring)?;
                cost_evals += 1;

                population.push(offspring);
                costs.push(cost);

                if cost < best_cost {
                    best_cost = cost;
                    best_params = population.last().unwrap().clone();
                }
            }

            // Report best of generation
            callback.on_iteration(iter + 1, &best_params, best_cost)?;

            if callback.should_stop() {
                return Ok(SolverResult {
                    success: true,
                    cost: best_cost,
                    iterations: iter + 1,
                    message: "Stopped by callback".into(),
                    params: best_params,
                    cost_evals,
                    grad_evals: 0,
                });
            }

            if best_cost < self.precision {
                return Ok(SolverResult {
                    success: true,
                    cost: best_cost,
                    iterations: iter + 1,
                    message: "Converged".into(),
                    params: best_params,
                    cost_evals,
                    grad_evals: 0,
                });
            }

            // Sort population by fitness
            let mut indices: Vec<usize> = (0..lambda).collect();
            indices.sort_by(|&a, &b| costs[a].partial_cmp(&costs[b]).unwrap());

            // Compute new mean (recombination)
            let old_mean = mean.clone();
            mean = vec![0.0; n];
            for i in 0..mu {
                let idx = indices[i];
                for j in 0..n {
                    mean[j] += weights[i] * population[idx][j];
                }
            }

            // Update evolution paths
            let mean_shift: Vec<f64> = mean
                .iter()
                .zip(old_mean.iter())
                .map(|(m, om)| (m - om) / sigma)
                .collect();

            // Update ps
            for i in 0..n {
                ps[i] = (1.0 - cs) * ps[i] + (cs * (2.0 - cs) * mu as f64).sqrt() * mean_shift[i];
            }

            // Adapt sigma
            let ps_norm: f64 = ps.iter().map(|x| x * x).sum::<f64>().sqrt();
            let expectation_norm = (n as f64).sqrt() * (1.0 - 1.0 / (4.0 * n as f64));
            sigma *= ((cs / damps) * (ps_norm / expectation_norm - 1.0)).exp();

            // Update pc
            for i in 0..n {
                pc[i] = (1.0 - cc) * pc[i] + cc * mean_shift[i];
            }

            // Update covariance matrix (rank-one update)
            for i in 0..n {
                for j in 0..n {
                    C[i][j] = (1.0 - c1 - cmu) * C[i][j] + c1 * pc[i] * pc[j];
                }
            }
        }

        Ok(SolverResult {
            success: false,
            cost: best_cost,
            iterations: self.max_iter,
            message: "Max iterations reached".into(),
            params: best_params,
            cost_evals,
            grad_evals: 0,
        })
    }
}
