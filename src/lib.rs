use pyo3::prelude::*;

mod core;
mod optimization;
mod optimizer;
mod simulation;

pub use core::*;
pub use optimization::*;
pub use optimizer::Optimizer;
pub use simulation::NgSpice;

#[pymodule]
fn uwasic_optimizer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Core types
    m.add_class::<TargetMode>()?;
    m.add_class::<RelationshipType>()?;
    m.add_class::<Environment>()?;
    m.add_class::<Parameter>()?;
    m.add_class::<Target>()?;
    m.add_class::<Test>()?;
    m.add_class::<ParameterConstraint>()?;

    // Main optimizer
    m.add_class::<Optimizer>()?;

    // Output results
    m.add_class::<OptimizationResult>()?;
    m.add_class::<CompiledExpression>()?;

    Ok(())
}
