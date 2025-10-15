#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/ngspice_bindings.rs"));

use std::ffi::{CStr, CString};
use std::ptr;

pub struct NgSpice {
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for NgSpice {}
unsafe impl Sync for NgSpice {}

impl NgSpice {
    pub fn new() -> Self {
        Self {
            user_data: ptr::null_mut(),
        }
    }

    pub fn init(
        &mut self,
        print_fn: SendChar,
        stat_fn: SendStat,
        exit_fn: ControlledExit,
        data_fn: SendData,
        init_data_fn: SendInitData,
        bg_thread_fn: BGThreadRunning,
    ) -> Result<(), String> {
        unsafe {
            let result = ngSpice_Init(
                print_fn,
                stat_fn,
                exit_fn,
                data_fn,
                init_data_fn,
                bg_thread_fn,
                self.user_data,
            );
            if result != 0 {
                return Err("NgSpice initialization failed".to_string());
            }
            Ok(())
        }
    }

    pub fn init_sync(
        &mut self,
        vsrc_fn: GetVSRCData,
        isrc_fn: GetISRCData,
        sync_fn: GetSyncData,
        ident: Option<&mut i32>,
    ) -> Result<(), String> {
        unsafe {
            let ident_ptr = ident.map_or(ptr::null_mut(), |i| i as *mut i32);
            let result = ngSpice_Init_Sync(vsrc_fn, isrc_fn, sync_fn, ident_ptr, self.user_data);
            if result != 0 {
                return Err("NgSpice sync initialization failed".to_string());
            }
            Ok(())
        }
    }

    pub fn command(&self, cmd: &str) -> Result<(), String> {
        unsafe {
            let c_cmd = CString::new(cmd).map_err(|e| e.to_string())?;
            let result = ngSpice_Command(c_cmd.as_ptr() as *mut i8);
            if result != 0 {
                return Err(format!("NgSpice command failed: {}", cmd));
            }
            Ok(())
        }
    }

    /// Execute multiple commands sequentially
    /// Commands are joined with newlines and executed as a single batch
    pub fn commands(&self, cmds: &[&str]) -> Result<(), String> {
        let joined = cmds.join("\n");
        self.command(&joined)
    }

    /// Alter a component value at runtime
    /// Example: alter_component("R1", "2k") or alter_component("C1", "10u")
    /// This modifies the circuit after it's been loaded and can be called repeatedly
    pub fn alter_component(&self, component: &str, value: &str) -> Result<(), String> {
        self.command(&format!("alter {} = {}", component, value))
    }

    /// Alter a specific component parameter using the @ syntax
    /// Example: alter_parameter("R1", "resistance", "2000")
    /// This is more explicit and works for specific device parameters
    pub fn alter_parameter(&self, component: &str, param: &str, value: &str) -> Result<(), String> {
        self.command(&format!("alter @{}[{}]={}", component, param, value))
    }

    pub fn load_circuit(&self, lines: &[&str]) -> Result<(), String> {
        unsafe {
            let mut c_lines: Vec<*mut i8> = Vec::with_capacity(lines.len() + 1);

            for &line in lines {
                c_lines.push(
                    CString::new(line)
                        .map_err(|e| format!("Invalid circuit line: {}", e))?
                        .into_raw(),
                );
            }
            c_lines.push(ptr::null_mut());

            let result = ngSpice_Circ(c_lines.as_mut_ptr());

            for &c_line in &c_lines[..lines.len()] {
                let _ = CString::from_raw(c_line);
            }

            if result != 0 {
                return Err("NgSpice circuit loading failed".to_string());
            }
            Ok(())
        }
    }

    pub fn get_vector_info(&self, vec_name: &str) -> Result<*mut vector_info, String> {
        unsafe {
            let c_name = CString::new(vec_name).map_err(|e| e.to_string())?;
            let result = ngGet_Vec_Info(c_name.as_ptr() as *mut i8);
            if result.is_null() {
                return Err(format!("Vector not found: {}", vec_name));
            }
            Ok(result)
        }
    }

    pub fn current_plot(&self) -> String {
        unsafe {
            let c_str = ngSpice_CurPlot();
            if c_str.is_null() {
                return String::new();
            }
            CStr::from_ptr(c_str).to_string_lossy().into_owned()
        }
    }

    pub fn all_plots(&self) -> Vec<String> {
        unsafe {
            let plots = ngSpice_AllPlots();
            if plots.is_null() {
                return Vec::new();
            }

            let mut result = Vec::new();
            let mut i = 0;
            loop {
                let plot_ptr = *plots.offset(i);
                if plot_ptr.is_null() {
                    break;
                }
                result.push(CStr::from_ptr(plot_ptr).to_string_lossy().into_owned());
                i += 1;
            }
            result
        }
    }

    pub fn all_vecs(&self, plot_name: &str) -> Result<Vec<String>, String> {
        unsafe {
            let c_name = CString::new(plot_name).map_err(|e| e.to_string())?;
            let vecs = ngSpice_AllVecs(c_name.as_ptr() as *mut i8);
            if vecs.is_null() {
                return Ok(Vec::new());
            }

            let mut result = Vec::new();
            let mut i = 0;
            loop {
                let vec_ptr = *vecs.offset(i);
                if vec_ptr.is_null() {
                    break;
                }
                result.push(CStr::from_ptr(vec_ptr).to_string_lossy().into_owned());
                i += 1;
            }
            Ok(result)
        }
    }

    pub fn is_running(&self) -> bool {
        unsafe { ngSpice_running() }
    }

    pub fn set_breakpoint(&self, time: f64) -> Result<(), String> {
        unsafe {
            if !ngSpice_SetBkpt(time) {
                return Err("Failed to set breakpoint".to_string());
            }
            Ok(())
        }
    }

    /// Get a scalar value by using 'print' command
    pub fn get_scalar_value(&self, var_name: &str) -> Result<f64, String> {
        // Use 'let' command to evaluate the expression/variable into a temp vector
        let temp_name = format!("_temp_{}", var_name.replace("_", ""));
        let print_cmd = format!("let {} = {}", temp_name, var_name);

        // Try to evaluate and store in temp vector
        if self.command(&print_cmd).is_ok() {
            // Now read the temp vector
            match self.get_vector_info(&temp_name) {
                Ok(vec_info) => {
                    let values = extract_vector_values(vec_info);
                    if values.is_empty() {
                        return Err(format!("Variable '{}' evaluated to no values", var_name));
                    }
                    Ok(values[0])
                }
                Err(e) => Err(format!(
                    "Could not read temp vector for '{}': {}",
                    var_name, e
                )),
            }
        } else {
            Err(format!("Could not evaluate variable '{}'", var_name))
        }
    }

    /// Extract a scalar value using the plot prefix
    ///
    /// Sometimes measurement results are stored in a specific plot.
    /// This allows accessing them with the plot name prefix.
    ///
    /// # Example
    /// ```rust
    /// let dc_gain = ngspice.get_meas_result_from_plot("ac1", "dc_gain_val")?;
    /// ```
    pub fn get_meas_result_from_plot(
        &self,
        plot_name: &str,
        meas_name: &str,
    ) -> Result<f64, String> {
        let full_name = format!("{}.{}", plot_name, meas_name);
        self.get_meas_result(&full_name)
    }

    /// Get a complex vector (magnitude) from simulation results
    ///
    /// For AC analysis, vectors often contain complex values.
    /// This returns the magnitude sqrt(real^2 + imag^2).
    pub fn get_vector_magnitude(&self, vec_name: &str) -> Result<Vec<f64>, String> {
        // This is already handled by extract_vector_values
        // which computes magnitude for complex data
        self.get_vector(vec_name)
    }

    /// Get real and imaginary parts separately
    ///
    /// Returns (real_values, imag_values)
    pub fn get_vector_complex(&self, vec_name: &str) -> Result<(Vec<f64>, Vec<f64>), String> {
        unsafe {
            let c_name = CString::new(vec_name).map_err(|e| e.to_string())?;
            let vec_info = ngGet_Vec_Info(c_name.as_ptr() as *mut i8);

            if vec_info.is_null() {
                return Err(format!("Vector '{}' not found", vec_name));
            }

            let info = &*vec_info;
            let length = info.v_length as usize;

            if !info.v_compdata.is_null() {
                let mut real_vals = Vec::with_capacity(length);
                let mut imag_vals = Vec::with_capacity(length);

                for i in 0..length {
                    let complex = *info.v_compdata.offset(i as isize);
                    real_vals.push(complex.cx_real);
                    imag_vals.push(complex.cx_imag);
                }

                Ok((real_vals, imag_vals))
            } else if !info.v_realdata.is_null() {
                // Real data only
                let mut real_vals = Vec::with_capacity(length);
                for i in 0..length {
                    real_vals.push(*info.v_realdata.offset(i as isize));
                }
                let imag_vals = vec![0.0; length];
                Ok((real_vals, imag_vals))
            } else {
                Err(format!("Vector '{}' has no data", vec_name))
            }
        }
    }

    /// Extract a scalar value from a measurement result
    ///
    /// After running `meas`, ngspice creates a single-value vector.
    /// This retrieves that value programmatically.
    pub fn get_meas_result(&self, meas_name: &str) -> Result<f64, String> {
        match self.get_vector_info(meas_name) {
            Ok(vec_info) => {
                let values = extract_vector_values(vec_info);
                if values.is_empty() {
                    return Err(format!("Measurement '{}' returned no values", meas_name));
                }
                Ok(values[0])
            }
            Err(_) => Err(format!("Measurement '{}' not found", meas_name)),
        }
    }

    /// Get a vector of values from simulation results
    pub fn get_vector(&self, vec_name: &str) -> Result<Vec<f64>, String> {
        match self.get_vector_info(vec_name) {
            Ok(vec_info) => {
                let values = extract_vector_values(vec_info);
                if values.is_empty() {
                    return Err(format!("Vector '{}' is empty", vec_name));
                }
                Ok(values)
            }
            Err(_) => Err(format!("Vector '{}' not found", vec_name)),
        }
    }

    /// List all available vectors in current plot (for debugging)
    pub fn list_vectors(&self) -> Vec<String> {
        let plot = self.current_plot();
        if plot.is_empty() {
            return Vec::new();
        }
        self.all_vecs(&plot).unwrap_or_default()
    }
}

impl Default for NgSpice {
    fn default() -> Self {
        Self::new()
    }
}


/// Extract all values from a vector as a Vec<f64>
pub fn extract_vector_values(vec_info: *const vector_info) -> Vec<f64> {
    unsafe {
        if vec_info.is_null() {
            return Vec::new();
        }

        let info = &*vec_info;
        let length = info.v_length as usize;
        let mut result = Vec::with_capacity(length);

        if !info.v_compdata.is_null() {
            for i in 0..length {
                let complex = *info.v_compdata.offset(i as isize);
                result.push(
                    (complex.cx_real * complex.cx_real + complex.cx_imag * complex.cx_imag).sqrt(),
                );
            }
        } else if !info.v_realdata.is_null() {
            for i in 0..length {
                result.push(*info.v_realdata.offset(i as isize));
            }
        }

        result
    }
}
