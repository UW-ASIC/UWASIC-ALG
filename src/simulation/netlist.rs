use super::NgSpice;
use crate::core::{Parameter, Test};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Parameterize netlist by injecting .param directives and replacing component values
pub fn parameterize_netlist(
    netlist_lines: &[String],
    parameters: &[Parameter],
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();

    // Preserve title line
    if let Some(first_line) = netlist_lines.first() {
        if !first_line.trim().starts_with('.') {
            result.push(first_line.clone());
        }
    }

    // Add parameter definitions at top
    result.push("".to_string());
    result.push("* === Optimization Parameters (Auto-generated) ===".to_string());
    for param in parameters {
        result.push(format!(".param {} = {}", param.name, param.value));
    }
    result.push("* === End Parameters ===".to_string());
    result.push("".to_string());

    // Build component->parameter mapping
    let component_params = build_component_param_map(parameters);

    // Process netlist lines
    let start_idx = if netlist_lines
        .first()
        .map(|l| !l.trim().starts_with('.'))
        .unwrap_or(false)
    {
        1
    } else {
        0
    };

    for line in &netlist_lines[start_idx..] {
        let trimmed = line.trim();

        // Skip existing .param lines
        if trimmed.starts_with(".param") {
            continue;
        }

        // Parameterize component lines (X* or M*)
        if trimmed.starts_with('X') || trimmed.starts_with('M') {
            let comp_name = trimmed.split_whitespace().next().unwrap_or("");
            if let Some(params) = component_params.get(comp_name) {
                result.push(parameterize_component_line(line, params));
                continue;
            }
        }

        result.push(line.clone());
    }

    Ok(result)
}

/// Build mapping from component names to their parameters
fn build_component_param_map(
    parameters: &[Parameter],
) -> HashMap<String, Vec<(String, String)>> {
    let mut component_params: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for param in parameters {
        if let Some(underscore_pos) = param.name.rfind('_') {
            let component = param.name[..underscore_pos].to_string();
            let param_type = param.name[underscore_pos + 1..].to_string();
            component_params
                .entry(component)
                .or_insert_with(Vec::new)
                .push((param_type, param.name.clone()));
        }
    }

    component_params
}

/// Parameterize a single component line by replacing values with {param} references
fn parameterize_component_line(line: &str, params: &[(String, String)]) -> String {
    let mut modified = line.to_string();

    for (ptype, pname) in params {
        let pattern = format!(" {}=", ptype);
        if let Some(pos) = modified.find(&pattern) {
            let val_start = pos + pattern.len();
            let remaining = &modified[val_start..];
            let val_end = remaining
                .find(|c: char| c.is_whitespace())
                .unwrap_or(remaining.len());

            // Replace value with {parameter_name}
            modified = format!(
                "{}={{{}}}{}",
                &modified[..pos + pattern.len() - 1],
                pname,
                &modified[val_start + val_end..]
            );
        }
    }

    modified
}


/// Write netlist to temporary file and load into NgSpice
pub fn write_and_load_netlist(
    netlist: &[String],
    ngspice: &NgSpice,
) -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("ngspice_opt_{}.spice", std::process::id()));

    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;

    for line in netlist {
        writeln!(file, "{}", line).map_err(|e| format!("Failed to write: {}", e))?;
    }

    // Load circuit using 'source' command (required for alterparam + reset)
    let source_cmd = format!("source {}", temp_path.display());
    ngspice
        .command(&source_cmd)
        .map_err(|e| format!("Failed to source circuit: {}", e))?;

    Ok(temp_path)
}

/// Process test environments by substituting environment variables in spice_code
pub fn process_test_environments(tests: &[Test], verbose: bool) -> Result<Vec<Test>, String> {
    let mut processed_tests = Vec::with_capacity(tests.len());

    for test in tests {
        let mut processed_code = test.spice_code.clone();

        // Replace environment variable placeholders with actual values
        for env in &test.environment {
            let placeholder = format!("{{{}}}", env.name);
            processed_code = processed_code.replace(&placeholder, &env.value);
        }

        if verbose && !test.environment.is_empty() {
            println!("  Test '{}' environments:", test.name);
            for env in &test.environment {
                println!("    {} = {}", env.name, env.value);
            }
        }

        processed_tests.push(Test {
            name: test.name.clone(),
            spice_code: processed_code,
            description: test.description.clone(),
            environment: test.environment.clone(),
        });
    }

    Ok(processed_tests)
}
