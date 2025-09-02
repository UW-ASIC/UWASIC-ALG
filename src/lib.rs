mod ngspice;
mod xschem;
pub mod optimizer;
mod utilities;

// Re-export main types and functions
pub use ngspice::{run_spice, gen_spice_file, SimulationResult, SpiceGenerationResult};
pub use xschem::{XSchemIO, XSchemObject, Component, Wire, Text};
pub use optimizer::{OptimizationProblem};
pub use utilities::{SchematicFiles, glob_files};
