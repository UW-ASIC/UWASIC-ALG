use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub enum FileType {
    Schematic,
    Symbol,
    Testbench,
    Invalid,
}

pub struct XSchemNetlist {
    file_path: PathBuf,
}

impl XSchemNetlist {
    /// Create a new XSchemNetlist instance
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self, String> {
        let path = file_path.as_ref().to_path_buf();
        let file_type = Self::detect_file_type(&path);

        match file_type {
            FileType::Invalid => Err(format!("Invalid file type: {}", path.display())),
            _ => Ok(Self { file_path: path }),
        }
    }

    /// Detect file type based on extension
    pub fn detect_file_type(file_path: &Path) -> FileType {
        let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check for testbench files first (more specific pattern)
        if filename.ends_with("_tb.sch") {
            return FileType::Testbench;
        }

        // Check for regular schematic files
        if filename.ends_with(".sch") {
            return FileType::Schematic;
        }

        // Check for symbol files
        if filename.ends_with(".sym") {
            return FileType::Symbol;
        }

        FileType::Invalid
    }

    /// Find the testbench file for this schematic
    /// Returns path to _tb.sch file if it exists
    pub fn find_testbench(&self) -> Option<PathBuf> {
        let stem = self.file_path.file_stem()?.to_str()?;
        let parent = self.file_path.parent()?;

        // Try with _tb suffix
        let tb_path = parent.join(format!("{}_tb.sch", stem));
        if tb_path.exists() {
            return Some(tb_path);
        }

        None
    }

    /// Generate netlist from the schematic file (prefers testbench if available)
    /// Returns absolute path to the generated netlist
    pub fn generate_netlist(&self, template_dir: &Path, verbose: bool) -> Result<PathBuf, String> {
        // Use testbench if available, otherwise use schematic
        let (file_to_netlist, is_testbench) = if let Some(tb_path) = self.find_testbench() {
            if verbose {
                println!("Found testbench: {}", tb_path.display());
            }
            (tb_path, true)
        } else {
            (self.file_path.clone(), false)
        };
        // Determine output directory (same as schematic)
        let schematic_dir = self
            .file_path
            .parent()
            .ok_or_else(|| "Failed to get schematic directory".to_string())?;

        // Generate netlist filename based on what we're netlisting
        let netlist_name = file_to_netlist
            .file_stem()
            .ok_or_else(|| "Failed to get file stem".to_string())?
            .to_str()
            .ok_or_else(|| "Invalid filename".to_string())?;
        let netlist_path = schematic_dir.join(format!("{}.spice", netlist_name));

        // Get current working directory
        let cwd = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        // Create xschemrc in current working directory
        let xschemrc_path = cwd.join("xschemrc");

        // Get absolute path to template directory
        let abs_template_dir = fs::canonicalize(template_dir)
            .map_err(|e| format!("Failed to get absolute path for template: {}", e))?;

        // Create xschemrc content
        let xschemrc_content = format!(
            r#"source $env(HOME)/.volare/volare/sky130/versions/0fe599b2afb6708d281543108caf8310912f54af/sky130A/libs.tech/xschem/xschemrc
set SKYWATER_MODELS "$env(HOME)/.volare/sky130A/libs.tech/ngspice"
set SKYWATER_STDCELLS "$env(HOME)/.volare/sky130A/libs.ref/sky130_fd_sc_hd/spice"
puts "PDK set SKYWATER_MODELS to: $SKYWATER_MODELS"
puts "PDK set SKYWATER_STDCELLS to: $SKYWATER_STDCELLS"
#### PROJECT CONFIGURATION
set PROJECT_NAME "template"
set PROJECT_ROOT [file normalize "[file dirname [info script]]/../../schematics"]
set dark_colorscheme 1
set gaw_viewer "gaw"
set editor "vim"
#### NETLIST CONFIGURATION
set netlist_dir {}
file mkdir $netlist_dir
set XSCHEM_NETLIST_DIR $netlist_dir
set netlist_type spice
set spice_netlist 1

append XSCHEM_LIBRARY_PATH :{}
"#,
            abs_template_dir.display(),
            abs_template_dir.display()
        );

        // Write xschemrc file
        let mut xschemrc_file = fs::File::create(&xschemrc_path)
            .map_err(|e| format!("Failed to create xschemrc: {}", e))?;
        xschemrc_file
            .write_all(xschemrc_content.as_bytes())
            .map_err(|e| format!("Failed to write xschemrc: {}", e))?;

        if verbose {
            println!("Created xschemrc at: {}", xschemrc_path.display());
            println!("Template directory: {}", abs_template_dir.display());
        }

        // Get absolute path to file we're netlisting
        let abs_file = fs::canonicalize(&file_to_netlist)
            .map_err(|e| format!("Failed to get absolute path for file: {}", e))?;

        if verbose {
            println!(
                "Netlisting {} file: {}",
                if is_testbench {
                    "testbench"
                } else {
                    "schematic"
                },
                abs_file.display()
            );
        }

        // Build xschem command
        let output = Command::new("xschem")
            .arg("--netlist")
            .arg("-q")
            .arg("-x")
            .arg(&abs_file)
            .current_dir(&cwd) // Run in directory with xschemrc
            .output()
            .map_err(|e| format!("Failed to execute xschem: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("xschem netlist generation failed: {}", stderr));
        }

        if verbose {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                println!("xschem output:\n{}", stdout);
            }
        }

        // Verify netlist was created
        if !netlist_path.exists() {
            return Err(format!("Netlist not found at: {}", netlist_path.display()));
        }

        // Return absolute path
        let abs_netlist = fs::canonicalize(&netlist_path)
            .map_err(|e| format!("Failed to get absolute netlist path: {}", e))?;

        Ok(abs_netlist)
    }

    /// Load netlist file into memory as vector of lines
    pub fn load_netlist(netlist_path: &Path) -> Result<Vec<String>, String> {
        let content = fs::read_to_string(netlist_path)
            .map_err(|e| format!("Failed to read netlist: {}", e))?;

        Ok(content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect())
    }
}
