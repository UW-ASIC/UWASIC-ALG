use std::io::Result as IoResult;
use std::path::Path;
use indexmap::IndexMap;

use crate::xschem::objects::{
    XSchemObject, Version, Header, Component, Wire, Text, Section, Line, Rectangle,
};

pub fn parse_file<P: AsRef<Path>>(file_path: P) -> IoResult<Vec<XSchemObject>> {
    let content = std::fs::read_to_string(file_path)?;
    
    // Inlined parse_content
    let mut objects = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') || line.starts_with('*') {
            i += 1;
            continue;
        }

        // Handle multiline constructs
        if line.contains('{') && line.matches('{').count() != line.matches('}').count() {
            // Collect multiline content
            let mut content = lines[i].to_string();
            let mut brace_count = lines[i].matches('{').count() as i32 - lines[i].matches('}').count() as i32;
            let mut consumed = 1;
            
            while brace_count > 0 && i + consumed < lines.len() {
                let line = lines[i + consumed];
                content.push('\n');
                content.push_str(line);
                brace_count += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                consumed += 1;
            }
            
            // Inlined parse_line
            if let Some(obj) = {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if !parts.is_empty() {
                        match parts[0] {
                            "v" if trimmed.starts_with("v {") => {
                                // Extract version string
                                let version_str = {
                                    let pattern = "version=";
                                    if let Some(start) = trimmed.find(pattern) {
                                        let value_start = start + pattern.len();
                                        let value_end = trimmed[value_start..].find(|c: char| c.is_whitespace() || c == '}')
                                            .map(|pos| value_start + pos)
                                            .unwrap_or(trimmed.len());
                                        Some(trimmed[value_start..value_end].to_string())
                                    } else {
                                        None
                                    }
                                };
                                
                                if let Some(version_str) = version_str {
                                    // Extract license content (everything between first { and last })
                                    let license = if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                                        if start < end {
                                            trimmed[start+1..end].to_string()
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    };
                                    
                                    Some(XSchemObject::Version(Version {
                                        version: version_str,
                                        file_version: "1.2".to_string(),
                                        license,
                                    }))
                                } else {
                                    Some(XSchemObject::Header(Header { 
                                        version: "3.4.4".to_string() 
                                    }))
                                }
                            },
                            
                            "C" if trimmed.starts_with("C {") => {
                                // C {symbol} x y rotation flip {properties}
                                if let Some(symbol_end) = trimmed.find('}') {
                                    let symbol_end = symbol_end + 1;
                                    let symbol = trimmed[3..symbol_end-1].to_string(); 
                                    let remaining = &trimmed[symbol_end..];
                                    let coords: Vec<&str> = remaining.split_whitespace().collect();
                                    
                                    if coords.len() >= 4 {
                                        let properties = extract_properties(remaining);
                                        Some(XSchemObject::Component(Component {
                                            symbol_reference: symbol,
                                            x: coords[0].parse().unwrap_or(0.0),
                                            y: coords[1].parse().unwrap_or(0.0),
                                            rotation: coords[2].parse().unwrap_or(0),
                                            flip: coords[3].parse().unwrap_or(0),
                                            properties,
                                        }))
                                    } else { None }
                                } else { None }
                            },
                            
                            "N" => {
                                // N x1 y1 x2 y2 ... {properties}
                                let coords_end = trimmed.find('{').unwrap_or(trimmed.len());
                                let coords_str = &trimmed[..coords_end];
                                let coord_parts: Vec<&str> = coords_str.split_whitespace().skip(1).collect();
                                
                                let mut points = Vec::new();
                                for chunk in coord_parts.chunks(2) {
                                    if chunk.len() == 2 {
                                        points.push((
                                            chunk[0].parse().unwrap_or(0.0),
                                            chunk[1].parse().unwrap_or(0.0)
                                        ));
                                    }
                                }
                                
                                if !points.is_empty() {
                                    let (x1, y1) = points[0];
                                    let (x2, y2) = *points.last().unwrap_or(&(x1, y1));
                                    Some(XSchemObject::Wire(Wire {
                                        x1,
                                        y1,
                                        x2,
                                        y2,
                                        points,
                                        properties: extract_properties(trimmed),
                                    }))
                                } else { None }
                            },
                            
                            "T" => {
                                // T {text} x y rotation mirror hSize vSize {properties}
                                if let (Some(text_start), Some(text_end)) = (trimmed.find('{'), trimmed.find('}')) {
                                    let text = trimmed[text_start+1..text_end].to_string();
                                    let remaining = &trimmed[text_end+1..];
                                    let coords: Vec<&str> = remaining.split_whitespace().collect();
                                    
                                    if coords.len() >= 6 {
                                        let h_size = coords[4].parse().unwrap_or(0.0);
                                        let v_size = coords[5].parse().unwrap_or(0.0);
                                        
                                        Some(XSchemObject::Text(Text {
                                            text,
                                            x: coords[0].parse().unwrap_or(0.0),
                                            y: coords[1].parse().unwrap_or(0.0),
                                            rotation: coords[2].parse().unwrap_or(0),
                                            mirror: coords[3].parse().unwrap_or(0),
                                            h_size,
                                            v_size,
                                            size: v_size,
                                            properties: extract_properties(remaining),
                                        }))
                                    } else { None }
                                } else { None }
                            },
                            
                            "G" | "K" | "V" | "S" | "E" => {
                                let section_type = parts[0].to_string();
                                // Extract content between first { and last }
                                let content = if trimmed.len() > 4 {
                                    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                                        if start < end {
                                            trimmed[start+1..end].to_string()
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };
                                Some(XSchemObject::Section(Section { section_type, content }))
                            },
                            
                            "L" if parts.len() >= 6 => {
                                Some(XSchemObject::Line(Line {
                                    layer: parts[1].parse().unwrap_or(0),
                                    x1: parts[2].parse().unwrap_or(0.0),
                                    y1: parts[3].parse().unwrap_or(0.0),
                                    x2: parts[4].parse().unwrap_or(0.0),
                                    y2: parts[5].parse().unwrap_or(0.0),
                                    properties: extract_properties(trimmed),
                                }))
                            },
                            
                            "B" if parts.len() >= 6 => {
                                Some(XSchemObject::Rectangle(Rectangle {
                                    layer: parts[1].parse().unwrap_or(0),
                                    x1: parts[2].parse().unwrap_or(0.0),
                                    y1: parts[3].parse().unwrap_or(0.0),
                                    x2: parts[4].parse().unwrap_or(0.0),
                                    y2: parts[5].parse().unwrap_or(0.0),
                                    properties: extract_properties(trimmed),
                                }))
                            },
                            
                            _ => None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } {
                objects.push(obj);
            }
            i += consumed;
        } else {
            // Inlined parse_line for single line
            if let Some(obj) = {
                let trimmed = line;
                if !trimmed.is_empty() {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if !parts.is_empty() {
                        match parts[0] {
                            "v" if trimmed.starts_with("v {") => {
                                // Extract version string
                                let version_str = {
                                    let pattern = "version=";
                                    if let Some(start) = trimmed.find(pattern) {
                                        let value_start = start + pattern.len();
                                        let value_end = trimmed[value_start..].find(|c: char| c.is_whitespace() || c == '}')
                                            .map(|pos| value_start + pos)
                                            .unwrap_or(trimmed.len());
                                        Some(trimmed[value_start..value_end].to_string())
                                    } else {
                                        None
                                    }
                                };
                                
                                if let Some(version_str) = version_str {
                                    // Extract license content (everything between first { and last })
                                    let license = if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                                        if start < end {
                                            trimmed[start+1..end].to_string()
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    };
                                    
                                    Some(XSchemObject::Version(Version {
                                        version: version_str,
                                        file_version: "1.2".to_string(),
                                        license,
                                    }))
                                } else {
                                    Some(XSchemObject::Header(Header { 
                                        version: "3.4.4".to_string() 
                                    }))
                                }
                            },
                            
                            "C" if trimmed.starts_with("C {") => {
                                // C {symbol} x y rotation flip {properties}
                                if let Some(symbol_end) = trimmed.find('}') {
                                    let symbol_end = symbol_end + 1;
                                    let symbol = trimmed[3..symbol_end-1].to_string(); 
                                    let remaining = &trimmed[symbol_end..];
                                    let coords: Vec<&str> = remaining.split_whitespace().collect();
                                    
                                    if coords.len() >= 4 {
                                        let properties = extract_properties(remaining);
                                        Some(XSchemObject::Component(Component {
                                            symbol_reference: symbol,
                                            x: coords[0].parse().unwrap_or(0.0),
                                            y: coords[1].parse().unwrap_or(0.0),
                                            rotation: coords[2].parse().unwrap_or(0),
                                            flip: coords[3].parse().unwrap_or(0),
                                            properties,
                                        }))
                                    } else { None }
                                } else { None }
                            },
                            
                            "N" => {
                                // N x1 y1 x2 y2 ... {properties}
                                let coords_end = trimmed.find('{').unwrap_or(trimmed.len());
                                let coords_str = &trimmed[..coords_end];
                                let coord_parts: Vec<&str> = coords_str.split_whitespace().skip(1).collect();
                                
                                let mut points = Vec::new();
                                for chunk in coord_parts.chunks(2) {
                                    if chunk.len() == 2 {
                                        points.push((
                                            chunk[0].parse().unwrap_or(0.0),
                                            chunk[1].parse().unwrap_or(0.0)
                                        ));
                                    }
                                }
                                
                                if !points.is_empty() {
                                    let (x1, y1) = points[0];
                                    let (x2, y2) = *points.last().unwrap_or(&(x1, y1));
                                    Some(XSchemObject::Wire(Wire {
                                        x1,
                                        y1,
                                        x2,
                                        y2,
                                        points,
                                        properties: extract_properties(trimmed),
                                    }))
                                } else { None }
                            },
                            
                            "T" => {
                                // T {text} x y rotation mirror hSize vSize {properties}
                                if let (Some(text_start), Some(text_end)) = (trimmed.find('{'), trimmed.find('}')) {
                                    let text = trimmed[text_start+1..text_end].to_string();
                                    let remaining = &trimmed[text_end+1..];
                                    let coords: Vec<&str> = remaining.split_whitespace().collect();
                                    
                                    if coords.len() >= 6 {
                                        let h_size = coords[4].parse().unwrap_or(0.0);
                                        let v_size = coords[5].parse().unwrap_or(0.0);
                                        
                                        Some(XSchemObject::Text(Text {
                                            text,
                                            x: coords[0].parse().unwrap_or(0.0),
                                            y: coords[1].parse().unwrap_or(0.0),
                                            rotation: coords[2].parse().unwrap_or(0),
                                            mirror: coords[3].parse().unwrap_or(0),
                                            h_size,
                                            v_size,
                                            size: v_size,
                                            properties: extract_properties(remaining),
                                        }))
                                    } else { None }
                                } else { None }
                            },
                            
                            "G" | "K" | "V" | "S" | "E" => {
                                let section_type = parts[0].to_string();
                                // Extract content between first { and last }
                                let content = if trimmed.len() > 4 {
                                    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                                        if start < end {
                                            trimmed[start+1..end].to_string()
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };
                                Some(XSchemObject::Section(Section { section_type, content }))
                            },
                            
                            "L" if parts.len() >= 6 => {
                                Some(XSchemObject::Line(Line {
                                    layer: parts[1].parse().unwrap_or(0),
                                    x1: parts[2].parse().unwrap_or(0.0),
                                    y1: parts[3].parse().unwrap_or(0.0),
                                    x2: parts[4].parse().unwrap_or(0.0),
                                    y2: parts[5].parse().unwrap_or(0.0),
                                    properties: extract_properties(trimmed),
                                }))
                            },
                            
                            "B" if parts.len() >= 6 => {
                                Some(XSchemObject::Rectangle(Rectangle {
                                    layer: parts[1].parse().unwrap_or(0),
                                    x1: parts[2].parse().unwrap_or(0.0),
                                    y1: parts[3].parse().unwrap_or(0.0),
                                    x2: parts[4].parse().unwrap_or(0.0),
                                    y2: parts[5].parse().unwrap_or(0.0),
                                    properties: extract_properties(trimmed),
                                }))
                            },
                            
                            _ => None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } {
                objects.push(obj);
            }
            i += 1;
        }
    }

    Ok(objects)
}

fn extract_properties(line: &str) -> IndexMap<String, String> {
    let mut properties = IndexMap::new();
    
    if let (Some(start), Some(end)) = (line.rfind('{'), line.rfind('}')) {
        if start < end {
            let props_str = &line[start+1..end];
            
            // split by whitespace but handle quoted values
            let mut tokens = Vec::new();
            let mut current_token = String::new();
            let mut in_quotes = false;
            let mut quote_char = '"';
            
            for ch in props_str.chars() {
                match ch {
                    '"' | '\'' if !in_quotes => {
                        in_quotes = true;
                        quote_char = ch;
                        // Don't include the opening quote
                    }
                    ch if in_quotes && ch == quote_char => {
                        in_quotes = false;
                        // Don't include the closing quote
                    }
                    ch if ch.is_whitespace() && !in_quotes => {
                        if !current_token.is_empty() {
                            tokens.push(current_token.clone());
                            current_token.clear();
                        }
                    }
                    ch => {
                        current_token.push(ch);
                    }
                }
            }
            
            // Don't forget the last token
            if !current_token.is_empty() {
                tokens.push(current_token);
            }
            
            // Parse key=value pairs
            for token in tokens {
                if let Some(eq_pos) = token.find('=') {
                    let key = token[..eq_pos].trim().to_string();
                    let value = token[eq_pos+1..].trim().to_string();
                    properties.insert(key, value);
                }
            }
        }
    }
    
    properties
}
