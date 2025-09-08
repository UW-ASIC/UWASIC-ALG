use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;
use rayon::prelude::*;

use crate::xschem::XSchemIO;
use crate::ngspice::SpiceInterface;
use crate::utilities::glob_files;
use crate::pyinterface::TestConfiguration;
use crate::optimizer::problem::{SimulationBackend, ComponentParameter};
use crate::{vprintln, safe_println};

/// NgSpice-based simulation backend
#[derive(Debug)]
pub struct NgSpiceBackend {
    current_dir: PathBuf,
    netlist_dir: PathBuf,
    base_schematic: Arc<XSchemIO>,
    sky130_version: String,
    verbose: bool,
}

impl NgSpiceBackend {
    pub fn new(
        current_dir: PathBuf,
        netlist_dir: PathBuf,
        verbose: bool,
    ) -> Result<Self, String> {
        // Load base schematic once for caching
        let files = glob_files(current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        let schematic_file = files.schematic
            .ok_or("No schematic file found in current directory")?;
            
        let base_schematic = XSchemIO::load(&schematic_file, verbose)
            .map_err(|e| format!("Failed to load schematic: {}", e))?;
        
        Ok(Self {
            current_dir,
            netlist_dir,
            base_schematic: Arc::new(base_schematic),
            sky130_version: "0fe599b2afb6708d281543108caf8310912f54af".to_string(),
            verbose,
        })
    }
    
    fn create_temp_files(&self, spice_codes: &[String]) -> Result<Vec<TempSimulationFiles>, String> {
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        let base_testbench = files.testbench.ok_or("No testbench file found")?;
        let base_symbol = files.symbol.ok_or("No symbol file found")?;
        let base_schematic = files.schematic.ok_or("No schematic file found")?;
        
        let mut temp_files = Vec::new();
        
        for (index, spice_code) in spice_codes.iter().enumerate() {
            let uuid = Uuid::new_v4();
            let suffix = format!("_{}", uuid.simple());
            
            let temp_schematic = self.current_dir.join(format!("temp_sch{}.sch", suffix));
            let temp_symbol = self.current_dir.join(format!("temp_sym{}.sym", suffix));
            let temp_testbench = self.current_dir.join(format!("temp_tb{}.sch", suffix));
            
            let spice_name = temp_testbench.file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid testbench filename")?;
            let temp_spice = self.current_dir.join(self.netlist_dir.join(format!("{}.spice", spice_name)));
            
            // Copy base files
            std::fs::copy(&base_schematic, &temp_schematic)
                .map_err(|e| format!("Failed to copy schematic: {}", e))?;
            std::fs::copy(&base_symbol, &temp_symbol)
                .map_err(|e| format!("Failed to copy symbol: {}", e))?;
            
            // Create modified testbench
            let mut testbench = XSchemIO::load(&base_testbench, self.verbose)
                .map_err(|e| format!("Failed to load testbench: {}", e))?;
            
            // Update symbol references
            let base_symbol_name = base_symbol.file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid base symbol name")?;
            let temp_symbol_name = temp_symbol.file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid temp symbol name")?;
            
            testbench.update_symbol_references(base_symbol_name, temp_symbol_name);
            testbench.set_spice_code(spice_code);
            
            testbench.save(temp_testbench.to_str().unwrap())
                .map_err(|e| format!("Failed to save temp testbench: {}", e))?;
            
            let spice_interface = SpiceInterface::new(
                temp_testbench.clone(),
                temp_spice.clone(),
                self.netlist_dir.clone(),
                self.sky130_version.clone(),
                Arc::new(AtomicBool::new(false)),
                self.verbose,
            );
            
            temp_files.push(TempSimulationFiles {
                index,
                schematic: temp_schematic,
                symbol: temp_symbol,
                testbench: temp_testbench,
                spice_file: temp_spice,
                spice_interface,
            });
        }
        
        Ok(temp_files)
    }
    
    fn cleanup_temp_files(&self, temp_files: &[TempSimulationFiles]) {
        vprintln!(self.verbose, "Cleaning up {} temporary files", temp_files.len());
        
        for temp in temp_files {
            let _ = std::fs::remove_file(&temp.schematic);
            let _ = std::fs::remove_file(&temp.symbol);
            let _ = std::fs::remove_file(&temp.testbench);
            let _ = std::fs::remove_file(&temp.spice_file);
        }
    }
}

impl SimulationBackend for NgSpiceBackend {
    fn update_parameters(&self, params: &[f64], components: &[ComponentParameter]) -> Result<(), String> {
        vprintln!(self.verbose, "Updating {} parameters across {} components", 
                 params.len(), components.len());
        
        // Clone base schematic and update parameters
        let mut schematic = (*self.base_schematic).clone();
        schematic.set_verbose(self.verbose);
        
        // Update each component
        for comp_param in components {
            if let Some(component) = schematic.find_component_by_name_mut(&comp_param.component_name) {
                for (property_name, &param_index) in &comp_param.properties {
                    if let Some(&param_value) = params.get(param_index) {
                        let old_value = component.properties.get(property_name).cloned();
                        component.properties.insert(property_name.clone(), param_value.to_string());
                        
                        // Show parameter updates
                        match old_value {
                            Some(old) => safe_println!("  {}[{}]: {} -> {}", 
                                comp_param.component_name, property_name, old, param_value),
                            None => safe_println!("  {}[{}]: (new) -> {}", 
                                comp_param.component_name, property_name, param_value),
                        }
                    }
                }
            } else {
                vprintln!(self.verbose, "Warning: Component {} not found", comp_param.component_name);
            }
        }
        
        // Save updated schematic
        let files = glob_files(self.current_dir.to_str().unwrap())
            .map_err(|e| format!("Failed to glob files: {}", e))?;
        
        if let Some(schematic_file) = &files.schematic {
            schematic.save(schematic_file)
                .map_err(|e| format!("Failed to save schematic: {}", e))?;
        }
        
        Ok(())
    }
    
    fn run_simulation(&self, test_configs: &[TestConfiguration], spice_codes: &[String]) -> Result<Vec<f64>, String> {
        vprintln!(self.verbose, "Running simulation with {} test configurations", test_configs.len());
        
        if spice_codes.is_empty() {
            return Err("No SPICE codes provided".to_string());
        }
        
        // Create temporary files for each simulation
        let temp_files = self.create_temp_files(spice_codes)?;
        
        // Phase 1: Generate SPICE files sequentially (XSchem conflicts)
        vprintln!(self.verbose, "Phase 1: Generating SPICE files...");
        let mut successful_sims = Vec::new();
        
        for (temp, test_config) in temp_files.iter().zip(test_configs.iter()) {
            // Update testbench with test configuration
            let mut testbench = XSchemIO::load(&temp.testbench, self.verbose)
                .map_err(|e| format!("Failed to reload testbench: {}", e))?;
            
            testbench.update_testbench_components(&test_config.component_values)
                .map_err(|e| format!("Failed to update testbench: {}", e))?;
            
            testbench.save(temp.testbench.to_str().unwrap())
                .map_err(|e| format!("Failed to save updated testbench: {}", e))?;
            
            // Generate SPICE file
            match temp.spice_interface.gen_spice_file() {
                Ok(result) if result.is_success() => {
                    vprintln!(self.verbose, "  ✓ SPICE generated for simulation {}", temp.index);
                    successful_sims.push(temp);
                },
                Ok(result) => {
                    vprintln!(self.verbose, "  ✗ SPICE generation failed for simulation {}: {:?}", 
                             temp.index, result.get_error());
                },
                Err(e) => {
                    vprintln!(self.verbose, "  ✗ SPICE generation error for simulation {}: {}", temp.index, e);
                }
            }
        }
        
        if successful_sims.is_empty() {
            self.cleanup_temp_files(&temp_files);
            return Err("No SPICE files generated successfully".to_string());
        }
        
        // Phase 2: Run simulations in parallel
        vprintln!(self.verbose, "Phase 2: Running {} simulations in parallel", successful_sims.len());
        
        let results: Vec<_> = if successful_sims.len() > 1 {
            successful_sims.par_iter()
                .map(|temp| {
                    match temp.spice_interface.run_spice() {
                        Ok(result) if result.success => {
                            vprintln!(self.verbose, "  ✓ Simulation {} completed", temp.index);
                            Some((temp.index, result))
                        },
                        Ok(result) => {
                            vprintln!(self.verbose, "  ✗ Simulation {} failed: {:?}", 
                                     temp.index, result.get_error());
                            None
                        },
                        Err(e) => {
                            vprintln!(self.verbose, "  ✗ Simulation {} error: {}", temp.index, e);
                            None
                        }
                    }
                })
                .collect()
        } else {
            // Single simulation
            let temp = successful_sims[0];
            let result = temp.spice_interface.run_spice()
                .map_err(|e| format!("Simulation failed: {}", e))?;
            
            if result.success {
                vec![Some((temp.index, result))]
            } else {
                self.cleanup_temp_files(&temp_files);
                return Err(format!("Simulation failed: {:?}", result.get_error()));
            }
        };
        
        // Cleanup temporary files
        self.cleanup_temp_files(&temp_files);
        
        // Collect metrics
        let mut all_metrics = HashMap::new();
        let mut success_count = 0;
        
        for result_opt in results {
            if let Some((index, result)) = result_opt {
                success_count += 1;
                all_metrics.extend(result.get_metrics().clone());
                vprintln!(self.verbose, "  ✓ Simulation {} contributed {} metrics", 
                         index, result.metrics.len());
            }
        }
        
        if success_count == 0 {
            return Err("All simulations failed".to_string());
        }
        
        vprintln!(self.verbose, "✓ {} of {} simulations succeeded", success_count, temp_files.len());
        
        // Extract metric values in order (return 0.0 for missing metrics)
        let metric_values: Vec<f64> = spice_codes.iter()
            .enumerate()
            .map(|(i, _)| {
                // Use a default metric name pattern or extract from somewhere
                let metric_name = format!("metric_{}", i);
                all_metrics.get(&metric_name).copied().unwrap_or(0.0)
            })
            .collect();
        
        Ok(metric_values)
    }
}

/// Helper struct for managing temporary simulation files
#[derive(Debug)]
struct TempSimulationFiles {
    index: usize,
    schematic: PathBuf,
    symbol: PathBuf,
    testbench: PathBuf,
    spice_file: PathBuf,
    spice_interface: SpiceInterface,
}
