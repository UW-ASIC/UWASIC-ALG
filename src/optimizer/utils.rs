/// Utility functions for optimizer
use std::sync::Mutex;

/// Global storage for capturing NgSpice output
pub static NGSPICE_OUTPUT: Mutex<Vec<String>> = Mutex::new(Vec::new());
