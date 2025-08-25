mod ngspice;
mod xschem;
pub mod optimizer;
mod python_bindings;

pub use ngspice::{run_spice, gen_spice_file, SimulationResult};
pub use xschem::{XSchemIO, XSchemObject};
pub use optimizer::{OptimizationProblem, SpiceRunConfig, ComponentParameter, NgSpiceInterface};

use pyo3::prelude::*;

#[pymodule]
fn xschemoptimizer(py: Python, m: &PyModule) -> PyResult<()> {
    python_bindings::create_module(py, m)
}

