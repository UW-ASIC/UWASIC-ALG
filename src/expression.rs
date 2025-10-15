use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

/// Compact bytecode instruction (4 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum OpCode {
    LoadParam(u16),
    LoadConst(u16),
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Compiled expression - data-oriented layout for cache efficiency
#[pyclass]
#[derive(Clone, Debug)]
pub struct CompiledExpression {
    instructions: Vec<OpCode>,
    constants: Vec<f64>, // Constant pool
    param_count: u16,
}

impl CompiledExpression {
    /// Internal evaluation - returns static error strings
    #[inline]
    pub fn evaluate(&self, params: &[f64]) -> Result<f64, &'static str> {
        if params.len() != self.param_count as usize {
            return Err("Parameter count mismatch");
        }

        let mut stack = [0.0f64; 32]; // Fixed-size stack (no allocations)
        let mut sp = 0usize; // Stack pointer

        for &inst in &self.instructions {
            match inst {
                OpCode::LoadParam(idx) => {
                    stack[sp] = unsafe { *params.get_unchecked(idx as usize) };
                    sp += 1;
                }
                OpCode::LoadConst(idx) => {
                    stack[sp] = unsafe { *self.constants.get_unchecked(idx as usize) };
                    sp += 1;
                }
                OpCode::Add => {
                    sp -= 1;
                    stack[sp - 1] += stack[sp];
                }
                OpCode::Sub => {
                    sp -= 1;
                    stack[sp - 1] -= stack[sp];
                }
                OpCode::Mul => {
                    sp -= 1;
                    stack[sp - 1] *= stack[sp];
                }
                OpCode::Div => {
                    sp -= 1;
                    let divisor = stack[sp];
                    if divisor == 0.0 {
                        return Err("Division by zero");
                    }
                    stack[sp - 1] /= divisor;
                }
                OpCode::Pow => {
                    sp -= 1;
                    stack[sp - 1] = stack[sp - 1].powf(stack[sp]);
                }
            }
        }

        if sp != 1 {
            return Err("Invalid expression");
        }

        Ok(stack[0])
    }

    #[inline]
    pub fn is_satisfied(
        &self,
        params: &[f64],
        target: f64,
        tol: f64,
    ) -> Result<bool, &'static str> {
        self.evaluate(params).map(|v| (v - target).abs() <= tol)
    }
}

#[pymethods]
impl CompiledExpression {
    #[new]
    pub fn new(expr: String, param_names: Vec<String>) -> PyResult<Self> {
        Compiler::new(&param_names)
            .compile(&expr)
            .map_err(|e| PyValueError::new_err(format!("Expression compilation failed: {}", e)))
    }

    pub fn eval(&self, params: Vec<f64>) -> PyResult<f64> {
        self.evaluate(&params)
            .map_err(|e| PyRuntimeError::new_err(format!("Expression evaluation failed: {}", e)))
    }

    fn check(&self, params: Vec<f64>, target: f64, tolerance: f64) -> PyResult<bool> {
        self.is_satisfied(&params, target, tolerance)
            .map_err(|e| PyRuntimeError::new_err(format!("Expression check failed: {}", e)))
    }

    #[getter]
    fn param_count(&self) -> u16 {
        self.param_count
    }

    fn __repr__(&self) -> String {
        format!(
            "CompiledExpression(params={}, instructions={}, constants={})",
            self.param_count,
            self.instructions.len(),
            self.constants.len()
        )
    }
}

struct Compiler<'a> {
    params: &'a [String],
    instructions: Vec<OpCode>,
    constants: Vec<f64>,
}

impl<'a> Compiler<'a> {
    fn new(params: &'a [String]) -> Self {
        Self {
            params,
            instructions: Vec::with_capacity(32),
            constants: Vec::with_capacity(8),
        }
    }

    fn compile(mut self, expr: &str) -> Result<CompiledExpression, String> {
        if expr.is_empty() {
            return Err("Expression cannot be empty".into());
        }

        let cleaned: String = expr.chars().filter(|c| !c.is_whitespace()).collect();

        if cleaned.is_empty() {
            return Err("Expression contains only whitespace".into());
        }

        self.parse_expr(&cleaned)
            .map_err(|e| format!("Parse error: {}", e))?;

        Ok(CompiledExpression {
            instructions: self.instructions,
            constants: self.constants,
            param_count: self.params.len() as u16,
        })
    }

    fn add_const(&mut self, val: f64) -> u16 {
        // Reuse existing constants
        if let Some(idx) = self.constants.iter().position(|&v| v == val) {
            return idx as u16;
        }
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u16
    }

    fn parse_expr(&mut self, s: &str) -> Result<(), String> {
        self.parse_additive(s)
    }

    fn parse_additive(&mut self, s: &str) -> Result<(), String> {
        if let Some(pos) = find_op(s, &['+', '-']) {
            self.parse_additive(&s[..pos])?;
            self.parse_multiplicative(&s[pos + 1..])?;
            self.instructions.push(if s.as_bytes()[pos] == b'+' {
                OpCode::Add
            } else {
                OpCode::Sub
            });
        } else {
            self.parse_multiplicative(s)?;
        }
        Ok(())
    }

    fn parse_multiplicative(&mut self, s: &str) -> Result<(), String> {
        if let Some(pos) = find_op(s, &['*', '/']) {
            self.parse_multiplicative(&s[..pos])?;
            self.parse_power(&s[pos + 1..])?;
            self.instructions.push(if s.as_bytes()[pos] == b'*' {
                OpCode::Mul
            } else {
                OpCode::Div
            });
        } else {
            self.parse_power(s)?;
        }
        Ok(())
    }

    fn parse_power(&mut self, s: &str) -> Result<(), String> {
        if let Some(pos) = find_op(s, &['^']) {
            self.parse_atom(&s[..pos])?;
            self.parse_atom(&s[pos + 1..])?;
            self.instructions.push(OpCode::Pow);
        } else {
            self.parse_atom(s)?;
        }
        Ok(())
    }

    fn parse_atom(&mut self, s: &str) -> Result<(), String> {
        if s.is_empty() {
            return Err("Empty sub-expression".into());
        }

        // Handle parentheses
        if s.starts_with('(') {
            if !s.ends_with(')') {
                return Err(format!("Unmatched parentheses in '{}'", s));
            }

            let inner = &s[1..s.len() - 1];
            if !is_balanced(inner) {
                return Err(format!("Unbalanced parentheses in '{}'", s));
            }

            return self.parse_expr(inner);
        }

        // Check for invalid characters before parsing
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            return Err(format!("Invalid characters in '{}'", s));
        }

        // Try number
        if let Ok(num) = s.parse::<f64>() {
            if !num.is_finite() {
                return Err(format!("Number '{}' is not finite", s));
            }
            let idx = self.add_const(num);
            self.instructions.push(OpCode::LoadConst(idx));
            return Ok(());
        }

        // Try parameter
        if let Some(idx) = self.params.iter().position(|p| p == s) {
            self.instructions.push(OpCode::LoadParam(idx as u16));
            return Ok(());
        }

        // Provide helpful error message
        Err(format!(
            "Unknown identifier '{}'. Available parameters: [{}]",
            s,
            self.params.join(", ")
        ))
    }
}

#[inline]
fn find_op(s: &str, ops: &[char]) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0;

    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => depth -= 1,
            c if depth == 0 && ops.contains(&(c as char)) => return Some(i),
            _ => {}
        }
    }
    None
}

#[inline]
fn is_balanced(s: &str) -> bool {
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}
