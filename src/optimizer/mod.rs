mod problem;
mod solver;
mod utils;
mod xschem;

pub use problem::CircuitProblem;
pub use solver::{select_solver, CMAESOptimizer, NewtonOptimizer, ParticleOptimizer};
pub use solver::{Problem, Solver, SolverResult};
pub use xschem::XSchemNetlist;

use crate::ngspice::{vecinfoall, vecvaluesall, NgSpice};
use crate::optimizer::problem::CircuitOptimizationCallback;
use crate::types::*;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::Path;
use utils::NGSPICE_OUTPUT;

#[pyclass]
pub struct Optimizer {
    #[pyo3(get, set)]
    pub circuit: String,
    #[pyo3(get, set)]
    pub template: String,
    #[pyo3(get, set)]
    pub solver: String,
    #[pyo3(get, set)]
    pub max_iterations: u32,
    #[pyo3(get, set)]
    pub precision: f64,
    #[pyo3(get, set)]
    pub verbose: bool,
}

#[pymethods]
impl Optimizer {
    #[new]
    #[pyo3(signature = (circuit="".to_string(), template=".".to_string(), solver="auto".to_string(), max_iterations=1000, precision=1e-6, verbose=false))]
    fn new(
        circuit: String,
        template: String,
        solver: String,
        max_iterations: u32,
        precision: f64,
        verbose: bool,
    ) -> Self {
        Self {
            circuit,
            template,
            solver,
            max_iterations,
            precision,
            verbose,
        }
    }

    fn optimize(
        &self,
        parameters: Vec<Py<Parameter>>,
        tests: Vec<Py<Test>>,
        targets: Vec<Py<Target>>,
        constraints: Vec<Py<ParameterConstraint>>,
        py: Python,
    ) -> PyResult<Py<OptimizationResult>> {
        if self.verbose {
            println!("\n=== OPTIMIZATION START ===");
            println!("Circuit: {}", self.circuit);
            println!("Template: {}", self.template);
        }

        // Extract native types from Python
        let params_native: Vec<Parameter> =
            parameters.iter().map(|p| p.borrow(py).clone()).collect();
        let tests_native: Vec<Test> = tests.iter().map(|t| t.borrow(py).clone()).collect();
        let targets_native: Vec<Target> = targets.iter().map(|t| t.borrow(py).clone()).collect();
        let mut constraints_native: Vec<ParameterConstraint> =
            constraints.iter().map(|c| c.borrow(py).clone()).collect();

        // Validate constraints
        crate::validate_constraints(&mut constraints_native, &params_native)
            .map_err(|e| PyValueError::new_err(format!("Validation failed: {}", e)))?;

        let has_constraints = !constraints_native.is_empty();

        // Generate netlist
        let netlist_path_str = self.generate_netlist()?;
        let netlist_path = Path::new(&netlist_path_str);
        let netlist_lines = XSchemNetlist::load_netlist(netlist_path)
            .map_err(|e| PyValueError::new_err(format!("Failed to load netlist: {}", e)))?;

        // Initialize NgSpice with callbacks
        let mut ngspice = NgSpice::new();

        // NgSpice callbacks
        extern "C" fn print_cb(msg: *mut i8, _id: i32, _data: *mut std::ffi::c_void) -> i32 {
            use std::ffi::CStr;
            if !msg.is_null() {
                unsafe {
                    if let Ok(c_str) = CStr::from_ptr(msg).to_str() {
                        if let Ok(mut output) = NGSPICE_OUTPUT.lock() {
                            output.push(c_str.to_string());
                        }
                    }
                }
            }
            0
        }
        extern "C" fn stat_cb(_msg: *mut i8, _id: i32, _data: *mut std::ffi::c_void) -> i32 {
            0
        }
        extern "C" fn exit_cb(
            _status: i32,
            _immediate: bool,
            _quit: bool,
            _id: i32,
            _data: *mut std::ffi::c_void,
        ) -> i32 {
            0
        }
        extern "C" fn data_cb(
            _data: *mut vecvaluesall,
            _num: i32,
            _id: i32,
            _user: *mut std::ffi::c_void,
        ) -> i32 {
            0
        }
        extern "C" fn init_data_cb(
            _data: *mut vecinfoall,
            _id: i32,
            _user: *mut std::ffi::c_void,
        ) -> i32 {
            0
        }
        extern "C" fn bg_thread_cb(_running: bool, _id: i32, _data: *mut std::ffi::c_void) -> i32 {
            0
        }

        ngspice
            .init(
                Some(print_cb),
                Some(stat_cb),
                Some(exit_cb),
                Some(data_cb),
                Some(init_data_cb),
                Some(bg_thread_cb),
            )
            .map_err(|e| PyValueError::new_err(format!("NgSpice init failed: {}", e)))?;

        if self.verbose {
            println!("✓ NgSpice initialized");
        }

        // Create circuit problem (simplified - no tracking inside)
        let problem = CircuitProblem::new(
            params_native.clone(),
            constraints_native,
            ngspice,
            tests_native,
            targets_native.clone(),
            netlist_lines,
            self.verbose,
        )
        .map_err(|e| PyValueError::new_err(e))?;

        // Create callback for tracking/display
        let param_names: Vec<String> = params_native.iter().map(|p| p.name.clone()).collect();
        let mut callback = CircuitOptimizationCallback::new(
            self.verbose,
            self.max_iterations,
            targets_native,
            param_names,
            &problem,
        );

        let mut solver: Box<dyn Solver> = match self.solver.as_str() {
            "newton" => Box::new(NewtonOptimizer::new(self.max_iterations, self.precision)),
            "cmaes" => Box::new(CMAESOptimizer::new(self.max_iterations, self.precision)),
            "pso" => Box::new(ParticleOptimizer::new(self.max_iterations, self.precision)),
            _ => {
                // Prepare inputs for select_solver
                let num_params = params_native.len();
                let bounds: Vec<(f64, f64)> = params_native
                    .iter()
                    .map(|p| (p.min_val, p.max_val))
                    .collect();

                let (solver, _solver_name) = select_solver(
                    num_params,
                    &bounds,
                    has_constraints,
                    self.max_iterations,
                    self.precision,
                );
                solver
            }
        };

        if self.verbose {
            println!("Solver: {}", solver.name());
        }

        // Run optimization - NOW WITH CALLBACK!
        let result = solver
            .solve(&problem, &mut callback)
            .map_err(|e| PyValueError::new_err(e))?;

        if self.verbose {
            println!("\n=== OPTIMIZATION COMPLETE ===");
            println!("Success: {}", result.success);
            println!("Cost: {:.6e}", result.cost);
            println!("Iterations: {}", result.iterations);
            println!("Cost evals: {}", result.cost_evals);
            println!("Grad evals: {}", result.grad_evals);
        }

        // Convert result back to Python
        let final_params: Vec<Parameter> = params_native
            .iter()
            .zip(result.params.iter())
            .map(|(def, &value)| Parameter {
                name: def.name.clone(),
                value,
                min_val: def.min_val,
                max_val: def.max_val,
            })
            .collect();

        Py::new(
            py,
            OptimizationResult {
                success: result.success,
                cost: result.cost,
                iterations: result.iterations,
                message: result.message,
                parameters: final_params,
            },
        )
    }
}

impl Optimizer {
    /// Generate netlist from XSchem schematic
    fn generate_netlist(&self) -> PyResult<String> {
        if self.verbose {
            println!("\n=== GENERATING NETLIST ===");
        }

        let circuit_path = Path::new(&self.template).join(&self.circuit);

        // Check if schematic file
        if circuit_path.extension().and_then(|s| s.to_str()) == Some("sch") {
            // Verify testbench file
            let filename = circuit_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| PyValueError::new_err("Invalid circuit filename"))?;

            if !filename.ends_with("_tb.sch") {
                return Err(PyValueError::new_err(format!(
                    "Circuit must be a testbench file (ending in _tb.sch), got: {}",
                    filename
                )));
            }

            let xschem = XSchemNetlist::new(&circuit_path)
                .map_err(|e| PyValueError::new_err(format!("XSchem error: {}", e)))?;

            let netlist_path = xschem
                .generate_netlist(Path::new(&self.template), self.verbose)
                .map_err(|e| PyValueError::new_err(format!("Netlist generation failed: {}", e)))?;

            if self.verbose {
                println!("✓ Netlist generated: {}", netlist_path.display());
            }

            Ok(netlist_path.to_string_lossy().to_string())
        } else {
            // Assume SPICE file
            Ok(circuit_path.to_string_lossy().to_string())
        }
    }
    fn validate_constraints(
        &self,
        parameters: Vec<Py<Parameter>>,
        constraints: Vec<Py<ParameterConstraint>>,
        py: Python,
    ) -> PyResult<()> {
        let params_native: Vec<Parameter> =
            parameters.iter().map(|p| p.borrow(py).clone()).collect();

        let mut constraints_native: Vec<ParameterConstraint> =
            constraints.iter().map(|c| c.borrow(py).clone()).collect();

        crate::validate_constraints(&mut constraints_native, &params_native)
            .map_err(|e| PyValueError::new_err(e))?;

        for (py_constraint, native_constraint) in constraints.iter().zip(constraints_native.iter())
        {
            let mut borrowed = py_constraint.borrow_mut(py);
            borrowed.compiled = native_constraint.compiled.clone();
        }

        Ok(())
    }
}
