use std::io::Result as IoResult;
use std::path::Path;
use indexmap::IndexMap;

use crate::xschem::objects::{
    XSchemObject, Version, Component, Wire, Text, Section, Line, Rectangle,
};

pub fn parse_file<P: AsRef<Path>>(file_path: P) -> IoResult<Vec<XSchemObject>> {
    let content = std::fs::read_to_string(file_path)?;
    Ok(parse_content(&content))
}

fn parse_content(content: &str) -> Vec<XSchemObject> {
    let mut objects = Vec::new();
    let mut lines = content.lines();
    
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }

        // Handle multiline constructs
        let full_line = if trimmed.contains('{') && count_braces(trimmed) != 0 {
            collect_multiline_content(trimmed, &mut lines)
        } else {
            trimmed.to_string()
        };

        if let Some(obj) = parse_line(&full_line) {
            objects.push(obj);
        }
    }

    objects
}

fn count_braces(line: &str) -> i32 {
    line.chars().fold(0, |acc, ch| match ch {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

fn collect_multiline_content(first_line: &str, lines: &mut std::str::Lines) -> String {
    let mut content = String::from(first_line);
    let mut brace_count = count_braces(first_line);
    
    while brace_count > 0 {
        if let Some(line) = lines.next() {
            content.push('\n');
            content.push_str(line);
            brace_count += count_braces(line);
        } else {
            break;
        }
    }
    
    content
}

fn parse_line(line: &str) -> Option<XSchemObject> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "v" if trimmed.starts_with("v {") => parse_version(trimmed),
        "C" if trimmed.starts_with("C {") => parse_component(trimmed),
        "N" => parse_wire(trimmed),
        "T" => parse_text(trimmed),
        "G" | "K" | "V" | "S" | "E" => parse_section(trimmed, parts[0]),
        "L" if parts.len() >= 6 => parse_line_object(&parts, trimmed),
        "B" if parts.len() >= 6 => parse_rectangle(&parts, trimmed),
        _ => None,
    }
}

fn parse_version(line: &str) -> Option<XSchemObject> {
    // Extract version string
    let version_str = extract_value_after_pattern(line, "version=")?;
    
    // Extract license content (everything between first { and last })
    let license = extract_content_between_braces(line).unwrap_or_default();
    
    Some(XSchemObject::Version(Version {
        version: version_str,
        file_version: "1.2".to_string(),
        license,
    }))
}

fn parse_component(line: &str) -> Option<XSchemObject> {
    // C {symbol} x y rotation flip {properties}
    let symbol_end = line.find('}')? + 1;
    let symbol = line[3..symbol_end-1].to_string();
    let remaining = &line[symbol_end..];
    let coords: Vec<&str> = remaining.split_whitespace().collect();
    
    if coords.len() >= 4 {
        Some(XSchemObject::Component(Component {
            symbol_reference: symbol,
            x: coords[0].parse().unwrap_or(0.0),
            y: coords[1].parse().unwrap_or(0.0),
            rotation: coords[2].parse().unwrap_or(0),
            flip: coords[3].parse().unwrap_or(0),
            properties: extract_properties(remaining),
        }))
    } else {
        None
    }
}

fn parse_wire(line: &str) -> Option<XSchemObject> {
    // N x1 y1 x2 y2 ... {properties}
    let coords_end = line.find('{').unwrap_or(line.len());
    let coords_str = &line[..coords_end];
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
            properties: extract_properties(line),
        }))
    } else {
        None
    }
}

fn parse_text(line: &str) -> Option<XSchemObject> {
    // T {text} x y rotation mirror hSize vSize {properties}
    let (text_start, text_end) = (line.find('{')?, line.find('}')?);
    let text = line[text_start+1..text_end].to_string();
    let remaining = &line[text_end+1..];
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
    } else {
        None
    }
}

fn parse_section(line: &str, section_type: &str) -> Option<XSchemObject> {
    let content = if line.len() > 4 {
        extract_content_between_braces(line).unwrap_or_default()
    } else {
        String::new()
    };
    
    Some(XSchemObject::Section(Section { 
        section_type: section_type.to_string(), 
        content 
    }))
}

fn parse_line_object(parts: &[&str], line: &str) -> Option<XSchemObject> {
    Some(XSchemObject::Line(Line {
        layer: parts[1].parse().unwrap_or(0),
        x1: parts[2].parse().unwrap_or(0.0),
        y1: parts[3].parse().unwrap_or(0.0),
        x2: parts[4].parse().unwrap_or(0.0),
        y2: parts[5].parse().unwrap_or(0.0),
        properties: extract_properties(line),
    }))
}

fn parse_rectangle(parts: &[&str], line: &str) -> Option<XSchemObject> {
    Some(XSchemObject::Rectangle(Rectangle {
        layer: parts[1].parse().unwrap_or(0),
        x1: parts[2].parse().unwrap_or(0.0),
        y1: parts[3].parse().unwrap_or(0.0),
        x2: parts[4].parse().unwrap_or(0.0),
        y2: parts[5].parse().unwrap_or(0.0),
        properties: extract_properties(line),
    }))
}

// Helper functions
fn extract_value_after_pattern(text: &str, pattern: &str) -> Option<String> {
    let start = text.find(pattern)? + pattern.len();
    let end = text[start..].find(|c: char| c.is_whitespace() || c == '}')
        .map(|pos| start + pos)
        .unwrap_or(text.len());
    Some(text[start..end].to_string())
}

fn extract_content_between_braces(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start < end {
        Some(text[start+1..end].to_string())
    } else {
        None
    }
}

fn extract_properties(line: &str) -> IndexMap<String, String> {
    let mut properties = IndexMap::new();
    
    let content = match extract_content_between_braces(line) {
        Some(content) => content,
        None => return properties,
    };
    
    let tokens = tokenize_properties(&content);
    
    // Parse key=value pairs
    for token in tokens {
        if let Some(eq_pos) = token.find('=') {
            let key = token[..eq_pos].trim();
            let value = token[eq_pos+1..].trim();
            if !key.is_empty() {
                properties.insert(key.to_string(), value.to_string());
            }
        }
    }
    
    properties
}

fn tokenize_properties(props_str: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut chars = props_str.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' => {
                // Handle quoted strings
                let quote_char = ch;
                while let Some(inner_ch) = chars.next() {
                    if inner_ch == quote_char {
                        break;
                    }
                    current_token.push(inner_ch);
                }
            }
            ch if ch.is_whitespace() => {
                if !current_token.is_empty() {
                    tokens.push(std::mem::take(&mut current_token));
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
    
    tokens
}
