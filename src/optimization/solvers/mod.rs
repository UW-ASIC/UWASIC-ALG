mod cma_es;
mod newton;
mod particle;
pub mod traits;

pub use traits::{Problem, Solver, SolverResult};
pub use cma_es::CMAESOptimizer;
pub use newton::NewtonOptimizer;
pub use particle::ParticleOptimizer;

pub fn select_solver(
    num_params: usize,
    bounds: &[(f64, f64)],
    has_constraints: bool,
    max_iterations: u32,
    precision: f64,
) -> (Box<dyn Solver>, String) {
    // Analyze parameter ranges
    let mut ranges = Vec::new();
    let mut total_range = 0.0;
    
    for &(min, max) in bounds {
        let range = max - min;
        total_range += range;
        ranges.push(range);
    }
    
    let avg_range = total_range / num_params as f64;
    let has_tight_bounds = avg_range < 1.0;
    
    // Calculate parameter scaling variance (coefficient of variation)
    let parameter_scale_variance = if ranges.len() > 1 {
        let mean = ranges.iter().sum::<f64>() / ranges.len() as f64;
        let variance = ranges.iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / ranges.len() as f64;
        variance.sqrt() / mean
    } else {
        0.0
    };
    
    // Decision logic - prefer gradient-free methods for circuit optimization (noisy, non-convex)
    let (solver, reason): (Box<dyn Solver>, String) = match (num_params, has_tight_bounds, parameter_scale_variance, has_constraints) {
        // Small problems (1-2 params) with very tight bounds -> Newton as last resort
        (n, true, _, _) if n <= 2 && avg_range < 0.1 => {
            (
                Box::new(NewtonOptimizer::new(max_iterations, precision)),
                format!("Auto: Tiny problem ({} params, range {:.3}) → Newton (fast for smooth functions)", n, avg_range)
            )
        },

        // Small to medium problems (1-8 params) -> PSO (best for circuit optimization)
        (n, _, _, _) if n <= 8 => {
            let pop_size = (10 + n * 3).min(30);  // Scale population: 10-30 particles
            let pso = ParticleOptimizer::new(max_iterations, precision)
                .with_population_size(pop_size);
            (
                Box::new(pso),
                format!("Auto: {} params → PSO (pop={}, robust for noisy circuits)", n, pop_size)
            )
        },

        // Large problems (9+ params) or poorly scaled -> CMA-ES
        (n, _, var, _) if n >= 9 || var > 1.5 => {
            (
                Box::new(CMAESOptimizer::new(max_iterations, precision)),
                format!("Auto: Large problem ({} params, scale var: {:.2}) → CMA-ES (adaptive)", n, var)
            )
        },

        // Default fallback -> PSO (most robust for circuits)
        (n, _, _, _) => {
            (
                Box::new(ParticleOptimizer::new(max_iterations, precision)
                    .with_population_size(20)),
                format!("Auto: {} params → PSO (default, handles noise well)", n)
            )
        }
    };
    
    (solver, reason)
}

// Tests at the bottom:

