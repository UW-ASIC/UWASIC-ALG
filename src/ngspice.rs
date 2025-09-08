use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Result as IoResult;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::env;
use regex::Regex;

use crate::{vprintln, safe_println};

pub struct SpiceInterface {
    pub testbench_file: PathBuf,
    pub spice_file: PathBuf,
    pub netlist_dir: PathBuf,
    pub sky130version: String,
    pub verbose: bool,
    // Shared Variable
    pub xschemrc_created: Arc<AtomicBool>,
}

impl SpiceInterface {
    pub fn new(
        testbench_file: PathBuf,
        spice_file: PathBuf,
        netlist_dir: PathBuf,
        sky130version: String,
        xschemrc_created: Arc<AtomicBool>,
        verbose: bool,
    ) -> Self {
        Self {
            testbench_file,
            spice_file,
            netlist_dir,
            sky130version,
            xschemrc_created,
            verbose,
        }
    }

    fn gen_xschemrc(&self) -> IoResult<()> {
        let testbench_dir = self.testbench_file.parent()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Testbench file has no parent directory",
            ))?;
        
        let output_path = testbench_dir.join("xschemrc");
        
        let netlist_dir_str = self.netlist_dir.to_str()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid netlist directory path",
            ))?;

        let normalized_netlist = if netlist_dir_str.starts_with("./") {
            &netlist_dir_str[2..] // Remove the ./ prefix
        } else {
            netlist_dir_str
        };

        let content = format!("source $env(HOME)/.volare/volare/sky130/versions/{}/sky130A/libs.tech/xschem/xschemrc
        
set SKYWATER_MODELS \"$env(HOME)/.volare/sky130A/libs.tech/ngspice\"
set SKYWATER_STDCELLS \"$env(HOME)/.volare/sky130A/libs.ref/sky130_fd_sc_hd/spice\"

puts \"PDK set SKYWATER_MODELS to: $SKYWATER_MODELS\"
puts \"PDK set SKYWATER_STDCELLS to: $SKYWATER_STDCELLS\"

#### PROJECT CONFIGURATION
set PROJECT_NAME \"template\"
set dark_colorscheme 1
set gaw_viewer \"gaw\"
set editor \"vim\"

set PROJECT_ROOT [file normalize \"[file dirname [info script]]\"]
set netlist_dir [file normalize \"[file dirname [info script]]/{}\"]

#### NETLIST CONFIGURATION
file mkdir $netlist_dir
set XSCHEM_NETLIST_DIR $netlist_dir
set netlist_type spice
set spice_netlist 1

#### LIBRARY PATHS
append XSCHEM_LIBRARY_PATH :[file dirname [info script]]
", self.sky130version, normalized_netlist);

        fs::write(output_path, &content)?;
        Ok(())
    }

    pub fn gen_spice_file(&self) -> IoResult<SpiceGenerationResult> {
        let start_time = Instant::now();

        if !self.xschemrc_created.load(Ordering::SeqCst) {
            self.gen_xschemrc()?;
            self.xschemrc_created.store(true, Ordering::SeqCst);
        }

        let testbench_file_str = self.testbench_file.file_name()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput, 
                "Invalid testbench filename"
            ))?
            .to_string_lossy()
            .to_string();

        vprintln!(self.verbose, "🔧 Generating SPICE file from testbench:");
        vprintln!(self.verbose, "  Testbench: {}", testbench_file_str);
        vprintln!(self.verbose, "  Netlist dir: {}", self.netlist_dir.display());

        // Check if the testbench file exists
        if !self.testbench_file.exists() {
            let error_msg = format!("Testbench file not found: {}", self.testbench_file.display());
            vprintln!(self.verbose, "  ❌ {}", error_msg);
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, error_msg));
        }

        vprintln!(self.verbose, "  ✓ Testbench file found at: {}", self.testbench_file.display());

        let testbench_dir = self.testbench_file.parent().unwrap_or(Path::new("."));
        
        // Print the exact command being run and working directory
        vprintln!(self.verbose, "  🔧 Command: cd {} && xschem --netlist -q -x {}", 
                 testbench_dir.display(), testbench_file_str);
        vprintln!(self.verbose, "  📂 Working directory: {}", testbench_dir.display());
        vprintln!(self.verbose, "  📄 Full testbench path: {}", self.testbench_file.display());
        vprintln!(self.verbose, "  🔍 Testbench file exists: {}", self.testbench_file.exists());
        vprintln!(self.verbose, "  🔍 Working dir is absolute: {}", testbench_dir.is_absolute());
        
        let temp_dir = match env::var("HOME") {
            Ok(home) => {
                let xschem_tmp = PathBuf::from(home).join(".cache/xschem-tmp");
                fs::create_dir_all(&xschem_tmp).ok();
                xschem_tmp
            },
            Err(_) => PathBuf::from("/var/tmp")
        };

        let result = Command::new("xschem")
            .current_dir(testbench_dir)
            .args(&["--netlist", "-q", "-x", &testbench_file_str])
            .env("TMPDIR", temp_dir.to_str().unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        self.process_xschem_output(result, start_time)
    }

    fn process_xschem_output(
        &self,
        result: std::io::Result<std::process::Output>,
        start_time: Instant,
    ) -> IoResult<SpiceGenerationResult> {
        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let execution_time = start_time.elapsed().as_secs_f64();

                vprintln!(self.verbose, "  XSchem execution completed in {:.3}s", execution_time);
                vprintln!(self.verbose, "  Exit status: {}", output.status);

                let success = output.status.success();

                if self.verbose {
                    if !stdout.is_empty() {
                        safe_println!("  Stdout ({} chars):", stdout.len());
                        for (i, line) in stdout.lines().enumerate() {
                            safe_println!("    {}: {}", i + 1, line);
                        }
                    } else {
                        safe_println!("  Stdout: (empty)");
                    }

                    if !stderr.is_empty() {
                        safe_println!("  Stderr ({} chars):", stderr.len());
                        for (i, line) in stderr.lines().enumerate() {
                            safe_println!("    {}: {}", i + 1, line);
                        }
                    } else {
                        safe_println!("  Stderr: (empty)");
                    }
                }

                // Use the pre-configured spice_file path
                let output_file = if success {
                    vprintln!(self.verbose, "  Expected SPICE file: {}", self.spice_file.display());

                    if self.spice_file.exists() {
                        if let Ok(metadata) = std::fs::metadata(&self.spice_file) {
                            vprintln!(self.verbose, "  ✓ SPICE file generated successfully ({} bytes)", metadata.len());
                        } else {
                            vprintln!(self.verbose, "  ✓ SPICE file exists");
                        }
                        Some(self.spice_file.to_string_lossy().to_string())
                    } else {
                        vprintln!(self.verbose, "  ❌ SPICE file was not created");
                        None
                    }
                } else {
                    vprintln!(self.verbose, "  ❌ XSchem command failed");
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
                vprintln!(self.verbose, "  Final result: {}", if final_success { "SUCCESS" } else { "FAILURE" });

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
                vprintln!(self.verbose, "  ❌ Failed to execute xschem: {}", e);
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

    /// Run ngspice simulation on the configured SPICE file
    pub fn run_spice(&self) -> IoResult<SimulationResult> {
        let start_time = Instant::now();
        let file_path_str = self.spice_file.to_str()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput, 
                "Invalid spice file path"
            ))?;

        vprintln!(self.verbose, "⚡ Running SPICE simulation:");
        vprintln!(self.verbose, "  File: {}", file_path_str);

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

                vprintln!(self.verbose, "  NGSpice execution completed in {:.3}s", execution_time);

                if self.verbose {
                    self.print_output_summary("Stdout", &stdout, 20);
                    self.print_output_summary("Stderr", &stderr, 10);
                }

                vprintln!(self.verbose, "  Extracting metrics from output...");
                let metrics = extract_metrics(&stdout, self.verbose);
                let success = output.status.success() && !metrics.is_empty();

                vprintln!(self.verbose, "  Extracted {} metrics", metrics.len());
                if self.verbose && !metrics.is_empty() {
                    for (key, value) in &metrics {
                        safe_println!("    {}: {:.6e}", key, value);
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
                    vprintln!(self.verbose, "  ❌ {}", err);
                } else {
                    vprintln!(self.verbose, "  ✓ Simulation completed successfully");
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
                vprintln!(self.verbose, "  ❌ Failed to execute ngspice: {}", e);
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

    fn print_output_summary(&self, label: &str, content: &str, max_lines: usize) {
        if content.is_empty() {
            safe_println!("  {}: (empty)", label);
            return;
        }

        let lines: Vec<&str> = content.lines().collect();
        safe_println!("  {} ({} chars, {} lines):", label, content.len(), lines.len());

        for (i, line) in lines.iter().enumerate().take(max_lines) {
            safe_println!("    {}: {}", i + 1, line);
        }

        if lines.len() > max_lines {
            safe_println!("    ... ({} more lines)", lines.len() - max_lines);
        }
    }
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
    pub fn is_success(&self) -> bool {
        self.success
    }
    pub fn get_error(&self) -> Option<&String> {
        self.error.as_ref()
    }
}

/// Extract metrics from ngspice output using regex patterns
fn extract_metrics(output: &str, verbose: bool) -> HashMap<String, f64> {
    let mut metrics = HashMap::new();

    vprintln!(verbose, "    Parsing output with regex patterns...");

    // Pre-compiled regex patterns for better performance
    thread_local! {
        static PATTERNS: Vec<Regex> = vec![
            Regex::new(r"([A-Z_][A-Z0-9_]*)\s*:\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)").unwrap(),
            Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)").unwrap(),
        ];
    }

    let mut total_matches = 0;

    PATTERNS.with(|patterns| {
        for (pattern_idx, pattern) in patterns.iter().enumerate() {
            let matches: Vec<_> = pattern.captures_iter(output).collect();
            vprintln!(verbose, "      Pattern {}: {} matches", pattern_idx + 1, matches.len());

            for cap in matches {
                if let (Some(metric_name), Some(value_str)) = (cap.get(1), cap.get(2)) {
                    if let Ok(value) = value_str.as_str().parse::<f64>() {
                        let key = metric_name.as_str().to_uppercase();
                        metrics.insert(key.clone(), value);
                        vprintln!(verbose, "        ✓ {}: {:.6e}", key, value);
                        total_matches += 1;
                    } else {
                        vprintln!(verbose, "        ⚠ Warning: Could not parse metric value '{}' for {}",
                                value_str.as_str(), metric_name.as_str());
                    }
                }
            }
        }
    });

    vprintln!(verbose, "    ✓ Total metrics extracted: {} (from {} pattern matches)", metrics.len(), total_matches);
    metrics
}

/// Result of a simulation run
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub metrics: HashMap<String, f64>,
    pub stdout: String,
    pub stderr: String,
    pub simulator_used: String,
    pub execution_time: f64,
    pub success: bool,
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
    pub fn get_metrics(&self) -> &HashMap<String, f64> {
        &self.metrics
    }
    pub fn get_error(&self) -> Option<&String> {
        self.error.as_ref()
    }
}
