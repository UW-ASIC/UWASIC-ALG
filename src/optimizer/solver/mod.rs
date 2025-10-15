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
    
    // Decision logic
    let (solver, reason): (Box<dyn Solver>, String) = match (num_params, has_tight_bounds, parameter_scale_variance, has_constraints) {
        // Small problems (1-3 params) with tight bounds -> Adaptive Newton
        (n, true, _, _) if n <= 3 => {
            (
                Box::new(NewtonOptimizer::new(max_iterations, precision)),
                format!("Auto: Small problem ({} params) with tight bounds → Newton (fast gradient-based)", n)
            )
        },
        
        // Medium problems (4-8 params) with uniform scaling -> PSO
        (n, _, var, false) if n >= 4 && n <= 8 && var < 1.0 => {
            let mut pso = ParticleOptimizer::new(max_iterations, precision);
            pso = pso.with_population_size(15 + n * 2);  // Scale swarm with dimension
            (
                Box::new(pso),
                format!("Auto: Medium problem ({} params, uniform scaling) → PSO (efficient exploration)", n)
            )
        },
        
        // Large problems (9+ params) or poorly scaled -> CMA-ES
        (n, _, var, _) if n >= 9 || var > 1.0 => {
            (
                Box::new(CMAESOptimizer::new(max_iterations, precision)),
                format!("Auto: Large/complex problem ({} params, scale variance: {:.2}) → CMA-ES (adaptive)", n, var)
            )
        },
        
        // Constrained problems -> PSO (handles constraints naturally)
        (n, _, _, true) => {
            (
                Box::new(ParticleOptimizer::new(max_iterations, precision)
                    .with_population_size(20)),
                format!("Auto: Constrained problem ({} params) → PSO (constraint handling)", n)
            )
        },
        
        // Default fallback -> PSO (robust general-purpose)
        (n, _, _, _) => {
            (
                Box::new(ParticleOptimizer::new(max_iterations, precision)),
                format!("Auto: General problem ({} params) → PSO (robust default)", n)
            )
        }
    };
    
    (solver, reason)
}

// Tests at the bottom:

