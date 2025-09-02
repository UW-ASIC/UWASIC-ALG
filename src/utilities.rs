use glob::glob;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SchematicFiles {
    pub schematic: Option<String>,    // *.sch (main schematic)
    pub symbol: Option<String>,       // *.sym (symbol file)
    pub testbench: Option<String>,    // *_tb.sch (testbench schematic)
}

impl SchematicFiles {
    pub fn new() -> Self {
        Self {
            schematic: None,
            symbol: None,
            testbench: None,
        }
    }
    
    pub fn is_complete(&self) -> bool {
        self.schematic.is_some() && self.symbol.is_some() && self.testbench.is_some()
    }
    
    pub fn missing_files(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.schematic.is_none() { missing.push("schematic (.sch)"); }
        if self.symbol.is_none() { missing.push("symbol (.sym)"); }
        if self.testbench.is_none() { missing.push("testbench (_tb.sch)"); }
        missing
    }
}

pub fn glob_files(directory: &str) -> Result<SchematicFiles, String> {
    let path = Path::new(directory);
    
    // Get absolute path for more reliable globbing
    let abs_path = std::fs::canonicalize(path)
        .map_err(|e| format!("Failed to canonicalize path '{}': {}", directory, e))?;
    
    let abs_path_str = abs_path.to_str()
        .ok_or_else(|| format!("Path '{}' contains invalid UTF-8", abs_path.display()))?;
    
    // Normalize path separators for glob patterns (use forward slashes)
    let normalized_path = abs_path_str.replace('\\', "/");
    
    let mut files = SchematicFiles::new();
    
    // Search for *.sch files (excluding testbench files)
    let sch_pattern = format!("{}/**/*.sch", normalized_path);
    match glob(&sch_pattern) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        // Check if it's a testbench file
                        if path_str.ends_with("_tb.sch") {
                            if files.testbench.is_none() {
                                files.testbench = Some(path_str);
                            }
                        } else {
                            // Regular schematic file
                            if files.schematic.is_none() {
                                files.schematic = Some(path_str);
                            }
                        }
                    }
                    Err(e) => eprintln!("Warning: Error reading schematic file: {}", e),
                }
            }
        }
        Err(e) => return Err(format!("Failed to glob schematic files: {}", e)),
    }
    
    // Search for *.sym files
    let sym_pattern = format!("{}/**/*.sym", normalized_path);
    match glob(&sym_pattern) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(path) => {
                        if files.symbol.is_none() {
                            files.symbol = Some(path.to_string_lossy().to_string());
                        }
                        break; // Take the first one found
                    }
                    Err(e) => eprintln!("Warning: Error reading symbol file: {}", e),
                }
            }
        }
        Err(e) => return Err(format!("Failed to glob symbol files: {}", e)),
    }
    
    Ok(files)
}
