use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Result as IoResult;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;
use regex::Regex;

use crate::xschem::XSchemIO;

// Runtime verbose macros
macro_rules! vprintln {
    ($verbose:expr, $($arg:tt)*) => {
        if $verbose {
            println!($($arg)*);
        }
    };
}

macro_rules! vprint {
    ($verbose:expr, $($arg:tt)*) => {
        if $verbose {
            print!($($arg)*);
        }
    };
}

fn generate_xschemrc(
    library_paths: Vec<String>, 
    netlist_dir: &str, 
    output_path: &Path,
    verbose: bool,
) -> IoResult<()> {
    vprintln!(verbose, "  📄 Generating xschemrc file at: {}", output_path.display());
    
    let mut content = String::new();
    let Some((parent_dir, _)) = netlist_dir.rsplit_once('/') else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid netlist directory format, expected 'path/to/dir'",
        ));
    };

    vprintln!(verbose, "    Parent directory: {}", parent_dir);
    vprintln!(verbose, "    Netlist directory: {}", netlist_dir);
    vprintln!(verbose, "    Library paths: {} entries", library_paths.len());

    // headers
    content.push_str("source $env(HOME)/.volare/volare/sky130/versions/12df12e2e74145e31c5a13de02f9a1e176b56e67/sky130A/libs.tech/xschem/xschemrc\n");
    content.push_str("set SKYWATER_MODELS \"$env(HOME)/.volare/sky130A/libs.tech/ngspice\"\n");
    content.push_str("set SKYWATER_STDCELLS \"$env(HOME)/.volare/sky130A/libs.ref/sky130_fd_sc_hd/spice\"\n");
    content.push_str("puts \"PDK set SKYWATER_MODELS to: $SKYWATER_MODELS\"\n");
    content.push_str("puts \"PDK set SKYWATER_STDCELLS to: $SKYWATER_STDCELLS\"\n");
    content.push_str("#### PROJECT CONFIGURATION\n");
    content.push_str("set PROJECT_NAME \"template\"\n");
    content.push_str(&format!("set PROJECT_ROOT [file normalize \"[file dirname [info script]]/{}\"]\n", parent_dir));
    content.push_str("set dark_colorscheme 1\n");
    content.push_str("set gaw_viewer \"gaw\"\n");
    content.push_str("set editor \"vim\"\n");

    // Netlist configuration section
    content.push_str("#### NETLIST CONFIGURATION\n");
    content.push_str(&format!("set netlist_dir [file normalize \"[file dirname [info script]]/{}\"]\n", netlist_dir));
    content.push_str("file mkdir $netlist_dir\n");
    content.push_str("set XSCHEM_NETLIST_DIR $netlist_dir\n");
    content.push_str("set netlist_type spice\n");
    content.push_str("set spice_netlist 1\n");
    
    // Library paths section
    content.push_str("## Library Paths (one for each library path provided)\n");
    for (i, library_path) in library_paths.iter().enumerate() {
        content.push_str(&format!("append XSCHEM_LIBRARY_PATH :[file dirname [info script]]/{}\n", library_path));
        vprintln!(verbose, "    Library path {}: {}", i + 1, library_path);
    }
    
    // Write the content to file
    fs::write(output_path, &content)?;
    
    vprintln!(verbose, "    ✓ Written {} lines, {} bytes", content.lines().count(), content.len());
    Ok(())
}

/// Generate SPICE netlist from XSchem testbench file
pub fn gen_spice_file<P1: AsRef<Path>, P2: AsRef<Path>>(
    testbench_file_path: P1, 
    current_dir: P2, 
    netlist_dir: &str,
    verbose: bool,
) -> IoResult<SpiceGenerationResult> {
    let start_time = Instant::now();
    let file_path_str = testbench_file_path.as_ref().to_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file path"))?;

    vprintln!(verbose, "🔧 Generating SPICE file from testbench:");
    vprintln!(verbose, "  Testbench: {}", file_path_str);
    vprintln!(verbose, "  Current dir: {}", current_dir.as_ref().display());
    vprintln!(verbose, "  Netlist dir: {}", netlist_dir);

    // Check if the testbench file exists relative to current_dir
    let full_testbench_path = current_dir.as_ref().join(testbench_file_path.as_ref());
    if !full_testbench_path.exists() {
        let error_msg = format!("Testbench file not found: {} (resolved to: {})", 
                               file_path_str, full_testbench_path.display());
        vprintln!(verbose, "  ❌ {}", error_msg);
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, error_msg));
    }

    vprintln!(verbose, "  ✓ Testbench file found at: {}", full_testbench_path.display());

    // Generate Xschemrc temporarily in same directory as command
    let xschemrc_path = current_dir.as_ref().join("xschemrc");
    let library_paths = vec![];
    generate_xschemrc(library_paths, netlist_dir, &xschemrc_path, verbose)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to generate xschemrc: {}", e)))?;

    // Use the already calculated full_testbench_path for xschem
    let resolved_file_path = full_testbench_path.to_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid resolved file path"))?;

    vprintln!(verbose, "  Running xschem command...");
    vprintln!(verbose, "  Command: xschem --netlist -q -x {}", resolved_file_path);

    let result = Command::new("xschem")
        .current_dir(current_dir.as_ref())
        .args(&["--netlist", "-q", "-x", resolved_file_path])  // -x: export netlist, -q: quit after operation
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    // Clean up the temporary xschemrc file
    vprintln!(verbose, "  Cleaning up temporary xschemrc file");
    let _ = std::fs::remove_file(&xschemrc_path);

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let execution_time = start_time.elapsed().as_secs_f64();
            
            vprintln!(verbose, "  XSchem execution completed in {:.3}s", execution_time);
            vprintln!(verbose, "  Exit status: {}", output.status);
            
            let success = output.status.success();
            
            if verbose {
                if !stdout.is_empty() {
                    println!("  Stdout ({} chars):", stdout.len());
                    for (i, line) in stdout.lines().enumerate().take(10) {
                        println!("    {}: {}", i + 1, line);
                    }
                    if stdout.lines().count() > 10 {
                        println!("    ... ({} more lines)", stdout.lines().count() - 10);
                    }
                }
                
                if !stderr.is_empty() {
                    println!("  Stderr ({} chars):", stderr.len());
                    for (i, line) in stderr.lines().enumerate().take(10) {
                        println!("    {}: {}", i + 1, line);
                    }
                    if stderr.lines().count() > 10 {
                        println!("    ... ({} more lines)", stderr.lines().count() - 10);
                    }
                }
            }
            
            // Determine the expected output file name (typically .spice or .sp extension)
            let output_file = if success {
                let base_name = testbench_file_path.as_ref()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("netlist");
                
                let spice_dir = current_dir
                    .as_ref()
                    .join(netlist_dir);

                // XSchem typically generates .spice files
                let spice_file = spice_dir.join(format!("{}.spice", base_name));
                
                vprintln!(verbose, "  Expected SPICE file: {}", spice_file.display());
                
                // Check if the SPICE file was actually created
                if spice_file.exists() {
                    let metadata = std::fs::metadata(&spice_file).ok();
                    if let Some(meta) = metadata {
                        vprintln!(verbose, "  ✓ SPICE file generated successfully ({} bytes)", meta.len());
                    } else {
                        vprintln!(verbose, "  ✓ SPICE file exists");
                    }
                    Some(spice_file.to_string_lossy().to_string())
                } else {
                    vprintln!(verbose, "  ❌ SPICE file was not created");
                    None
                }
            } else {
                vprintln!(verbose, "  ❌ XSchem command failed");
                None
            };

            let error = if success && output_file.is_some() { 
                None 
            } else if !success {
                Some(format!("XSchem netlist generation failed: {}", stderr))
            } else {
                Some("SPICE file was not generated (no output file found)".to_string())
            };

            let final_success = success && output_file.is_some();
            vprintln!(verbose, "  Final result: {}", if final_success { "SUCCESS" } else { "FAILURE" });

            Ok(SpiceGenerationResult {
                output_file,
                stdout,
                stderr,
                execution_time,
                success: final_success,
                error,
            })
        }
        Err(e) => {
            vprintln!(verbose, "  ❌ Failed to execute xschem: {}", e);
            Ok(SpiceGenerationResult {
                output_file: None,
                stdout: String::new(),
                stderr: String::new(),
                execution_time: start_time.elapsed().as_secs_f64(),
                success: false,
                error: Some(format!("Failed to execute xschem: {}", e)),
            })
        }
    }
}

pub fn gen_spice_files<P1: AsRef<Path>, P2: AsRef<Path>>(
    testbench_file_path: P1, 
    current_dir: P2, 
    netlist_dir: &str,
    spice_codes: Vec<String>,
    verbose: bool,
) -> IoResult<SpiceGenerationResult> {
    vprintln!(verbose, "🔧 Generating SPICE files with custom analysis codes:");
    vprintln!(verbose, "  Testbench: {}", testbench_file_path.as_ref().display());
    vprintln!(verbose, "  SPICE codes: {} blocks", spice_codes.len());

    // Load the testbench file
    let full_testbench_path = current_dir.as_ref().join(testbench_file_path.as_ref());
    
    if verbose {
        for (i, code) in spice_codes.iter().enumerate() {
            println!("    Block {}: {} lines", i + 1, code.lines().count());
        }
    }

    vprintln!(verbose, "  Loading testbench schematic: {}", full_testbench_path.display());
    
    let mut schematic = XSchemIO::load(&full_testbench_path)
        .map_err(|e| {
            vprintln!(verbose, "  ❌ Failed to load testbench: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to load testbench: {}", e))
        })?;

    vprintln!(verbose, "  ✓ Testbench loaded successfully");

    // Find the SPICE code component and update it
    vprintln!(verbose, "  Setting up SPICE analysis components...");
    let spice_code_component = schematic.ensure_spice_setup();
    
    // Combine all SPICE codes into one
    let combined_spice_code = spice_codes.join("\n\n");
    vprintln!(verbose, "  Combined SPICE code: {} lines total", combined_spice_code.lines().count());
    
    spice_code_component.properties.insert("value".to_string(), combined_spice_code);

    // Save the modified testbench
    vprintln!(verbose, "  Saving modified testbench...");
    schematic.save(full_testbench_path.to_str().unwrap())
        .map_err(|e| {
            vprintln!(verbose, "  ❌ Failed to save modified testbench: {}", e);
            std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to save modified testbench: {}", e))
        })?;

    vprintln!(verbose, "  ✓ Modified testbench saved");

    // Now generate the SPICE file with the updated testbench
    vprintln!(verbose, "  Generating SPICE netlist...");
    gen_spice_file(testbench_file_path, current_dir, netlist_dir, verbose)
}

/// Result of SPICE file generation
#[derive(Debug, Clone)]
pub struct SpiceGenerationResult {
    /// Path to the generated SPICE file, if successful
    pub output_file: Option<String>,
    /// Raw stdout from xschem
    pub stdout: String,
    /// Raw stderr from xschem
    pub stderr: String,
    /// Execution time in seconds
    pub execution_time: f64,
    /// Whether the generation succeeded
    pub success: bool,
    /// Error message if generation failed
    pub error: Option<String>,
}

impl fmt::Display for SpiceGenerationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "SPICE Generation Result")?;
        writeln!(f, "Success: {}", self.success)?;
        writeln!(f, "Execution time: {:.3}s", self.execution_time)?;
        
        if let Some(output_file) = &self.output_file {
            writeln!(f, "Output file: {}", output_file)?;
        }
        
        if let Some(error) = &self.error {
            writeln!(f, "Error: {}", error)?;
        }
        
        if f.alternate() {
            // Show stdout/stderr with {:#}
            if !self.stdout.is_empty() {
                writeln!(f, "\nStdout:\n{}", self.stdout)?;
            }
            if !self.stderr.is_empty() {
                writeln!(f, "\nStderr:\n{}", self.stderr)?;
            }
        }
        
        Ok(())
    }
}

impl SpiceGenerationResult {
    /// Check if generation was successful
    pub fn is_success(&self) -> bool {
        self.success
    }
    
    /// Get the path to the generated SPICE file
    pub fn get_output_file(&self) -> Option<&String> {
        self.output_file.as_ref()
    }
    
    /// Get error message if any
    pub fn get_error(&self) -> Option<&String> {
        self.error.as_ref()
    }
    
    /// Get execution time
    pub fn execution_time(&self) -> f64 {
        self.execution_time
    }
}

/// Run ngspice simulation on a SPICE file
pub fn run_spice<P: AsRef<Path>>(file_path: P, verbose: bool) -> IoResult<SimulationResult> {
    let start_time = Instant::now();
    let file_path_str = file_path.as_ref().to_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file path"))?;

    vprintln!(verbose, "⚡ Running SPICE simulation:");
    vprintln!(verbose, "  File: {}", file_path_str);
    vprintln!(verbose, "  Command: ngspice -b {}", file_path_str);

    let result = Command::new("ngspice")
        .args(&["-b", file_path_str])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let execution_time = start_time.elapsed().as_secs_f64();
            
            vprintln!(verbose, "  NGSpice execution completed in {:.3}s", execution_time);
            vprintln!(verbose, "  Exit status: {}", output.status);
            
            if verbose {
                if !stdout.is_empty() {
                    println!("  Stdout ({} chars):", stdout.len());
                    let lines: Vec<&str> = stdout.lines().collect();
                    for (i, line) in lines.iter().enumerate().take(20) {
                        println!("    {}: {}", i + 1, line);
                    }
                    if lines.len() > 20 {
                        println!("    ... ({} more lines)", lines.len() - 20);
                    }
                } else {
                    println!("  Stdout: (empty)");
                }
                
                if !stderr.is_empty() {
                    println!("  Stderr ({} chars):", stderr.len());
                    let lines: Vec<&str> = stderr.lines().collect();
                    for (i, line) in lines.iter().enumerate().take(10) {
                        println!("    {}: {}", i + 1, line);
                    }
                    if lines.len() > 10 {
                        println!("    ... ({} more lines)", lines.len() - 10);
                    }
                } else {
                    println!("  Stderr: (empty)");
                }
            }
            
            // Extract metrics from stdout
            vprintln!(verbose, "  Extracting metrics from output...");
            let metrics = extract_metrics(&stdout, verbose);
            let success = output.status.success() && !metrics.is_empty();

            vprintln!(verbose, "  Extracted {} metrics", metrics.len());
            if verbose && !metrics.is_empty() {
                for (key, value) in &metrics {
                    println!("    {}: {:.6e}", key, value);
                }
            }

            let error = if success { 
                None 
            } else if !output.status.success() {
                Some(format!("Simulation failed with exit code: {}", output.status))
            } else { 
                Some("Simulation completed but no metrics extracted".to_string())
            };

            if let Some(ref err) = error {
                vprintln!(verbose, "  ❌ {}", err);
            } else {
                vprintln!(verbose, "  ✓ Simulation completed successfully");
            }

            Ok(SimulationResult {
                metrics,
                stdout,
                stderr,
                simulator_used: "ngspice".to_string(),
                execution_time,
                success,
                error,
            })
        }
        Err(e) => {
            vprintln!(verbose, "  ❌ Failed to execute ngspice: {}", e);
            Ok(SimulationResult {
                metrics: HashMap::new(),
                stdout: String::new(),
                stderr: String::new(),
                simulator_used: "ngspice".to_string(),
                execution_time: start_time.elapsed().as_secs_f64(),
                success: false,
                error: Some(format!("Failed to execute ngspice: {}", e)),
            })
        }
    }
}

/// Extract metrics from ngspice output using regex patterns
fn extract_metrics(output: &str, verbose: bool) -> HashMap<String, f64> {
    let mut metrics = HashMap::new();
    
    vprintln!(verbose, "    Parsing output with regex patterns...");
    
    // Common ngspice output patterns
    let patterns = vec![
        // Pattern for key-value pairs like "POWER: 1.23e-3"
        Regex::new(r"([A-Z_][A-Z0-9_]*)\s*:\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)")
            .unwrap(),
        // Pattern for measurement results
        Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)")
            .unwrap(),
    ];

    let mut total_matches = 0;
    for (pattern_idx, pattern) in patterns.iter().enumerate() {
        let matches: Vec<_> = pattern.captures_iter(output).collect();
        vprintln!(verbose, "      Pattern {}: {} matches", pattern_idx + 1, matches.len());
        
        for cap in matches {
            if let (Some(metric_name), Some(value_str)) = (cap.get(1), cap.get(2)) {
                match value_str.as_str().parse::<f64>() {
                    Ok(value) => {
                        let key = metric_name.as_str().to_uppercase();
                        metrics.insert(key.clone(), value);
                        vprintln!(verbose, "        ✓ {}: {:.6e}", key, value);
                        total_matches += 1;
                    }
                    Err(_) => {
                        vprintln!(verbose, "        ⚠ Warning: Could not parse metric value '{}' for {}", 
                                value_str.as_str(), metric_name.as_str());
                    }
                }
            }
        }
    }

    vprintln!(verbose, "    ✓ Total metrics extracted: {} (from {} pattern matches)", metrics.len(), total_matches);
    metrics
}

/// Result of a simulation run
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// Parsed metrics from the simulation
    pub metrics: HashMap<String, f64>,
    /// Raw stdout from the simulator
    pub stdout: String,
    /// Raw stderr from the simulator  
    pub stderr: String,
    /// Simulator that was used
    pub simulator_used: String,
    /// Execution time in seconds
    pub execution_time: f64,
    /// Whether the simulation succeeded
    pub success: bool,
    /// Error message if simulation failed
    pub error: Option<String>,
}

impl fmt::Display for SimulationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Simulation Result ({})", self.simulator_used)?;
        writeln!(f, "Success: {}", self.success)?;
        writeln!(f, "Execution time: {:.3}s", self.execution_time)?;
        
        if let Some(error) = &self.error {
            writeln!(f, "Error: {}", error)?;
        }
        
        if !self.metrics.is_empty() {
            writeln!(f, "Metrics:")?;
            for (key, value) in &self.metrics {
                writeln!(f, "  {}: {:.6e}", key, value)?;
            }
        }
        
        if f.alternate() {
            // Show stdout/stderr with {:#}
            if !self.stdout.is_empty() {
                writeln!(f, "\nStdout:\n{}", self.stdout)?;
            }
            if !self.stderr.is_empty() {
                writeln!(f, "\nStderr:\n{}", self.stderr)?;
            }
        }
        
        Ok(())
    }
}

impl SimulationResult {
    /// Get reference to the extracted metrics
    pub fn get_metrics(&self) -> &HashMap<String, f64> {
        &self.metrics
    }
    
    /// Get a specific metric by name
    pub fn get_metric(&self, name: &str) -> Option<f64> {
        self.metrics.get(&name.to_uppercase()).copied()
    }
    
    /// Check if simulation was successful
    pub fn is_success(&self) -> bool {
        self.success
    }
    
    /// Get error message if any
    pub fn get_error(&self) -> Option<&String> {
        self.error.as_ref()
    }
    
    /// Get execution time
    pub fn execution_time(&self) -> f64 {
        self.execution_time
    }
}
