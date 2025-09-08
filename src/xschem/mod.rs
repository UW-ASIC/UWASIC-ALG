mod parser;
mod writer;
mod objects;

use std::collections::HashMap;
use std::io::Result as IoResult;
use std::path::Path;
use indexmap::IndexMap;

pub use objects::*;
use parser::parse_file;
use writer::write_file;

use crate::vprintln;

#[derive(Debug, Clone)]
pub struct XSchemIO {
    components: Vec<XSchemObject>,
    verbose: bool,
    // Cache for frequently accessed components to avoid repeated searches
    component_cache: HashMap<String, usize>, // name -> index
    symbol_cache: HashMap<String, Vec<usize>>, // symbol -> indices
}

impl XSchemIO {
    pub fn new(verbose: bool) -> Self {
        Self {
            components: Vec::new(),
            verbose,
            component_cache: HashMap::new(),
            symbol_cache: HashMap::new(),
        }
    }

    /// Enable or disable verbose debugging
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
        vprintln!(self.verbose, "Verbose mode {}", if verbose { "enabled" } else { "disabled" });
    }

    /// Rebuild internal caches after components are modified
    fn rebuild_caches(&mut self) {
        self.component_cache.clear();
        self.symbol_cache.clear();
        
        for (idx, obj) in self.components.iter().enumerate() {
            if let XSchemObject::Component(comp) = obj {
                // Cache by name
                if let Some(name) = comp.properties.get("name") {
                    self.component_cache.insert(name.clone(), idx);
                }
                
                // Cache by symbol
                self.symbol_cache
                    .entry(comp.symbol_reference.clone())
                    .or_insert_with(Vec::new)
                    .push(idx);
            }
        }
    }

    /// Load schematic from file with verbose option
    pub fn load<P: AsRef<Path>>(file_path: P, verbose: bool) -> IoResult<Self> {
        let file_path_str = file_path.as_ref().to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file path"))?;
        
        let mut instance = Self::new(verbose);
        vprintln!(instance.verbose, "Loading schematic from: {}", file_path_str);
        
        let components = parse_file(file_path_str)?;
        vprintln!(instance.verbose, "Loaded {} objects from file", components.len());
        
        if instance.verbose {
            // Count different object types in a single pass
            let (component_count, wire_count, text_count, other_count) = 
                components.iter().fold((0, 0, 0, 0), |(c, w, t, o), obj| {
                    match obj {
                        XSchemObject::Component(_) => (c + 1, w, t, o),
                        XSchemObject::Wire(_) => (c, w + 1, t, o),
                        XSchemObject::Text(_) => (c, w, t + 1, o),
                        _ => (c, w, t, o + 1),
                    }
                });
            
            vprintln!(instance.verbose, "Object breakdown: {} components, {} wires, {} text objects, {} other", 
                     component_count, wire_count, text_count, other_count);
        }
        
        instance.components = components;
        instance.rebuild_caches();
        Ok(instance)
    }

    /// Save schematic to file
    pub fn save(&self, file_path: &str) -> IoResult<()> {
        vprintln!(self.verbose, "Saving schematic to: {}", file_path);
        vprintln!(self.verbose, "Total objects to save: {}", self.components.len());
        
        let result = write_file(&self.components, file_path);
        
        if let Err(e) = &result {
            vprintln!(self.verbose, "Failed to save schematic: {}", e);
        } else {
            vprintln!(self.verbose, "Successfully saved schematic");
        }
        
        result
    }

    /// Find first component with matching symbol reference (optimized with cache)
    pub fn find_component_by_symbol(&self, symbol_ref: &str) -> Option<&Component> {
        vprintln!(self.verbose, "Searching for component with symbol: {}", symbol_ref);
        
        if let Some(indices) = self.symbol_cache.get(symbol_ref) {
            if let Some(&first_idx) = indices.first() {
                if let XSchemObject::Component(comp) = &self.components[first_idx] {
                    vprintln!(self.verbose, "Found component: {} at ({}, {})", 
                             comp.symbol_reference, comp.x, comp.y);
                    return Some(comp);
                }
            }
        }
        
        vprintln!(self.verbose, "Component with symbol '{}' not found", symbol_ref);
        None
    }

    /// Find mutable component by symbol reference (optimized with cache)
    pub fn find_component_by_symbol_mut(&mut self, symbol_ref: &str) -> Option<&mut Component> {
        vprintln!(self.verbose, "Searching for mutable component with symbol: {}", symbol_ref);
        
        if let Some(indices) = self.symbol_cache.get(symbol_ref).cloned() {
            if let Some(first_idx) = indices.first().copied() {
                if let XSchemObject::Component(comp) = &mut self.components[first_idx] {
                    vprintln!(self.verbose, "Found mutable component: {} at ({}, {})", 
                             comp.symbol_reference, comp.x, comp.y);
                    return Some(comp);
                }
            }
        }
        
        None
    }

    /// Find component by name property (optimized with cache)
    pub fn find_component_by_name(&self, name: &str) -> Option<&Component> {
        vprintln!(self.verbose, "Searching for component with name: {}", name);
        
        if let Some(&idx) = self.component_cache.get(name) {
            if let XSchemObject::Component(comp) = &self.components[idx] {
                vprintln!(self.verbose, "Found component '{}' with symbol: {}", name, comp.symbol_reference);
                return Some(comp);
            }
        }
        
        vprintln!(self.verbose, "Component with name '{}' not found", name);
        None
    }

    /// Find mutable component by name property (optimized with cache)
    pub fn find_component_by_name_mut(&mut self, name: &str) -> Option<&mut Component> {
        vprintln!(self.verbose, "Searching for mutable component with name: {}", name);
        
        if let Some(idx) = self.component_cache.get(name).copied() {
            if let XSchemObject::Component(comp) = &mut self.components[idx] {
                vprintln!(self.verbose, "Found mutable component '{}' with symbol: {}", name, comp.symbol_reference);
                return Some(comp);
            }
        }
        
        None
    }


    /// Ensure required SPICE simulation components exist
    pub fn ensure_spice_setup(&mut self) -> &mut Component {
        vprintln!(self.verbose, "Ensuring SPICE simulation setup exists");
        
        const SPICE_CORNER_SYMBOL: &str = "sky130_fd_pr/corner.sym";
        const SPICE_CODE_SYMBOL: &str = "devices/code_shown.sym";

        // Check corner component using cache
        if self.find_component_by_symbol(SPICE_CORNER_SYMBOL).is_none() {
            vprintln!(self.verbose, "Corner component not found, creating new one");
            self.add_spice_corner_component();
        } else {
            vprintln!(self.verbose, "Corner component already exists");
        }

        // Check SPICE code component using cache
        if self.find_component_by_symbol(SPICE_CODE_SYMBOL).is_none() {
            vprintln!(self.verbose, "SPICE code component not found, creating new one");
            self.add_spice_code_component();
        } else {
            vprintln!(self.verbose, "SPICE code component already exists");
        }

        vprintln!(self.verbose, "SPICE setup complete, returning reference to code component");
        
        // Return mutable reference to the SPICE code component
        self.find_component_by_symbol_mut(SPICE_CODE_SYMBOL).unwrap()
    }

    fn add_spice_corner_component(&mut self) {
        let mut corner_props = IndexMap::new();
        corner_props.insert("name".to_string(), "CORNER".to_string());
        corner_props.insert("only_toplevel".to_string(), "false".to_string());
        corner_props.insert("corner".to_string(), "tt".to_string());

        let corner = Component {
            symbol_reference: "sky130_fd_pr/corner.sym".to_string(),
            x: 300.0,
            y: -100.0,
            rotation: 0,
            flip: 0,
            properties: corner_props,
        };
        
        self.add_component_internal(corner);
        vprintln!(self.verbose, "Created corner component at (300, -100)");
    }

    fn add_spice_code_component(&mut self) {
        let mut spice_props = IndexMap::new();
        spice_props.insert("name".to_string(), "s1".to_string());
        spice_props.insert("only_toplevel".to_string(), "false".to_string());
        spice_props.insert("value".to_string(), String::new());

        let spice_code = Component {
            symbol_reference: "devices/code_shown.sym".to_string(),
            x: 240.0,
            y: 120.0,
            rotation: 0,
            flip: 0,
            properties: spice_props,
        };
        
        self.add_component_internal(spice_code);
        vprintln!(self.verbose, "Created SPICE code component at (240, 120)");
    }



    /// Internal method to add component and update caches
    fn add_component_internal(&mut self, component: Component) {
        let idx = self.components.len();
        
        // Update caches before adding
        if let Some(name) = component.properties.get("name") {
            self.component_cache.insert(name.clone(), idx);
        }
        
        self.symbol_cache
            .entry(component.symbol_reference.clone())
            .or_insert_with(Vec::new)
            .push(idx);
        
        self.components.push(XSchemObject::Component(component));
    }





    /// Get all objects (for direct access if needed)
    pub fn get_all_objects(&self) -> &Vec<XSchemObject> {
        vprintln!(self.verbose, "Retrieving all {} objects", self.components.len());
        &self.components
    }

    /// Get mutable reference to all objects
    pub fn get_all_objects_mut(&mut self) -> &mut Vec<XSchemObject> {
        vprintln!(self.verbose, "Retrieving mutable reference to all {} objects", self.components.len());
        // Note: caller should call rebuild_caches() if they modify components
        &mut self.components
    }

    /// Update testbench component values generically
    /// Returns Ok(()) on success, Err(component_name) if component not found
    pub fn update_testbench_components(&mut self, component_values: &std::collections::HashMap<String, String>) -> Result<(), String> {
        vprintln!(self.verbose, "Updating {} testbench components", component_values.len());
        
        for (component_name, component_value) in component_values {            
            vprintln!(self.verbose, "  Updating component '{}' with value: {}", 
                     component_name, component_value);
            
            if let Some(component) = self.find_component_by_name_mut(component_name) {
                // Update the value property
                component.properties.insert("value".to_string(), component_value.clone());
                vprintln!(self.verbose, "    ✓ Successfully updated component '{}'", component_name);
            } else {
                let error_msg = format!("Component '{}' not found in testbench", component_name);
                vprintln!(self.verbose, "    ❌ {}", error_msg);
                return Err(error_msg);
            }
        }
        
        vprintln!(self.verbose, "✓ All {} testbench components updated successfully", component_values.len());
        Ok(())
    }

    /// Update the SPICE code in the code_shown component
    pub fn set_spice_code(&mut self, new_spice_code: &str) -> bool {
        vprintln!(self.verbose, "Setting new SPICE code");
        vprintln!(self.verbose, "New SPICE code length: {} characters", new_spice_code.len());
        
        // First ensure the SPICE setup exists
        let _spice_component = self.ensure_spice_setup();
        
        // Now find and update the code_shown component
        const SPICE_CODE_SYMBOL: &str = "devices/code_shown.sym";
        
        if let Some(component) = self.find_component_by_symbol_mut(SPICE_CODE_SYMBOL) {
            component.properties.insert("value".to_string(), new_spice_code.to_string());
            vprintln!(self.verbose, "SPICE code updated successfully");
            true
        } else {
            vprintln!(self.verbose, "Error: Could not find SPICE code component after ensuring setup");
            false
        }
    }

    /// Get the current SPICE code from the code_shown component
    pub fn get_spice_code(&self) -> Option<String> {
        vprintln!(self.verbose, "Retrieving current SPICE code");
        
        const SPICE_CODE_SYMBOL: &str = "devices/code_shown.sym";
        
        if let Some(component) = self.find_component_by_symbol(SPICE_CODE_SYMBOL) {
            if let Some(spice_code) = component.properties.get("value") {
                vprintln!(self.verbose, "Found SPICE code, length: {} characters", spice_code.len());
                Some(spice_code.clone())
            } else {
                vprintln!(self.verbose, "SPICE code component found but has no 'value' property");
                None
            }
        } else {
            vprintln!(self.verbose, "No SPICE code component found");
            None
        }
    }


}
