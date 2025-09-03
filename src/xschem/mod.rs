mod parser;
mod writer;
mod objects;

use std::collections::HashMap;
use std::io::Result as IoResult;
use std::path::Path;
use indexmap::IndexMap;
use regex::Regex;

pub use objects::*;
use parser::parse_file;
use writer::write_file;

pub struct XSchemIO {
    pub components: Vec<XSchemObject>,
}

impl XSchemIO {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Load schematic from file
    pub fn load<P: AsRef<Path>>(file_path: P) -> IoResult<Self> {
        let file_path_str = file_path.as_ref().to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file path"))?;
        let components = parse_file(file_path_str)?;
        Ok(Self {
            components,
        })
    }

    /// Save schematic to file
    pub fn save(&self, file_path: &str) -> IoResult<()> {
        write_file(&self.components, file_path)
    }

    /// Find first component with matching symbol reference
    pub fn find_component_by_symbol(&self, symbol_ref: &str) -> Option<&Component> {
        self.components.iter().find_map(|comp| {
            if let XSchemObject::Component(c) = comp {
                if c.symbol_reference == symbol_ref {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Find mutable component by symbol reference
    pub fn find_component_by_symbol_mut(&mut self, symbol_ref: &str) -> Option<&mut Component> {
        self.components.iter_mut().find_map(|comp| {
            if let XSchemObject::Component(c) = comp {
                if c.symbol_reference == symbol_ref {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Find component by name property
    pub fn find_component_by_name(&self, name: &str) -> Option<&Component> {
        self.components.iter().find_map(|comp| {
            if let XSchemObject::Component(c) = comp {
                if c.properties.get("name") == Some(&name.to_string()) {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Find mutable component by name property
    pub fn find_component_by_name_mut(&mut self, name: &str) -> Option<&mut Component> {
        self.components.iter_mut().find_map(|comp| {
            if let XSchemObject::Component(c) = comp {
                if c.properties.get("name") == Some(&name.to_string()) {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Find components whose names match the regex pattern
    pub fn find_components_by_pattern(&self, pattern: &str) -> Result<Vec<&Component>, regex::Error> {
        let compiled_pattern = Regex::new(pattern)?;
        let mut matching_components = Vec::new();

        for comp in &self.components {
            if let XSchemObject::Component(c) = comp {
                if let Some(name) = c.properties.get("name") {
                    if compiled_pattern.is_match(name) {
                        matching_components.push(c);
                    }
                }
            }
        }

        Ok(matching_components)
    }

    /// Ensure required SPICE simulation components exist
    pub fn ensure_spice_setup(&mut self) -> &mut Component {
        const SPICE_CORNER_SYMBOL: &str = "sky130_fd_pr/corner.sym";
        const SPICE_CODE_SYMBOL: &str = "devices/code_shown.sym";

        // Check corner component
        if self.find_component_by_symbol(SPICE_CORNER_SYMBOL).is_none() {
            let mut corner_props = IndexMap::new();
            corner_props.insert("name".to_string(), "CORNER".to_string());
            corner_props.insert("only_toplevel".to_string(), "false".to_string());
            corner_props.insert("corner".to_string(), "tt".to_string());

            let corner = Component {
                symbol_reference: SPICE_CORNER_SYMBOL.to_string(),
                x: 300.0,
                y: -100.0,
                rotation: 0,
                flip: 0,
                properties: corner_props,
            };
            self.components.push(XSchemObject::Component(corner));
        }

        // Check SPICE code component
        if self.find_component_by_symbol(SPICE_CODE_SYMBOL).is_none() {
            let mut spice_props = IndexMap::new();
            spice_props.insert("name".to_string(), "s1".to_string());
            spice_props.insert("only_toplevel".to_string(), "false".to_string());
            spice_props.insert("value".to_string(), String::new());

            let spice_code = Component {
                symbol_reference: SPICE_CODE_SYMBOL.to_string(),
                x: 240.0,
                y: 120.0,
                rotation: 0,
                flip: 0,
                properties: spice_props,
            };
            self.components.push(XSchemObject::Component(spice_code));
        }

        // Return mutable reference to the SPICE code component
        self.find_component_by_symbol_mut(SPICE_CODE_SYMBOL).unwrap()
    }

    /// Update properties of a named component
    pub fn update_component_properties(&mut self, component_name: &str, new_properties: HashMap<String, String>) -> bool {
        if let Some(component) = self.find_component_by_name_mut(component_name) {
            component.properties.extend(new_properties);
            true
        } else {
            false
        }
    }

    /// Add a new component to the schematic
    pub fn add_component(
        &mut self,
        name: &str,
        symbol_path: &str,
        x: f64,
        y: f64,
        properties: Option<IndexMap<String, String>>,
    ) -> &Component {
        let mut comp_properties = IndexMap::new();
        comp_properties.insert("name".to_string(), name.to_string());
        
        if let Some(props) = properties {
            comp_properties.extend(props);
        }

        let component = Component {
            symbol_reference: symbol_path.to_string(),
            x,
            y,
            rotation: 0,
            flip: 0,
            properties: comp_properties,
        };

        self.components.push(XSchemObject::Component(component));

        // Return reference to the newly added component
        if let XSchemObject::Component(c) = self.components.last().unwrap() {
            c
        } else {
            unreachable!()
        }
    }

    /// Add a wire between two points
    pub fn add_wire(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let wire = Wire::new(x1, y1, x2, y2);
        self.components.push(XSchemObject::Wire(wire));
    }

    /// Add text at specified position
    pub fn add_text(&mut self, text: &str, x: f64, y: f64, size: f64) {
        let text_obj = Text::new(text, x, y, size);
        self.components.push(XSchemObject::Text(text_obj));
    }

    /// Get all components in the schematic
    pub fn get_components(&self) -> Vec<&Component> {
        self.components.iter().filter_map(|obj| {
            if let XSchemObject::Component(c) = obj {
                Some(c)
            } else {
                None
            }
        }).collect()
    }

    /// Get all wires in the schematic
    pub fn get_wires(&self) -> Vec<&Wire> {
        self.components.iter().filter_map(|obj| {
            if let XSchemObject::Wire(w) = obj {
                Some(w)
            } else {
                None
            }
        }).collect()
    }

    /// Get all objects (for direct access if needed)
    pub fn get_all_objects(&self) -> &Vec<XSchemObject> {
        &self.components
    }

    /// Get mutable reference to all objects
    pub fn get_all_objects_mut(&mut self) -> &mut Vec<XSchemObject> {
        &mut self.components
    }
}
