use super::types::{Parameter, ParameterConstraint};

/// Detect cyclic dependencies in parameter constraints
///
/// Uses depth-first search to detect cycles in the constraint dependency graph.
/// Returns an error if a cycle is detected, otherwise Ok(()).
pub fn detect_cycles(
    constraints: &[ParameterConstraint],
    params: &[Parameter],
) -> Result<(), String> {
    // Build adjacency list: parameter_index -> [dependent_parameter_indices]
    let param_count = params.len();
    let mut graph: Vec<Vec<usize>> = vec![Vec::new(); param_count];

    for constraint in constraints {
        if let Some(target_idx) = constraint.find_target_index(params) {
            let source_indices = constraint.find_source_indices(params);
            for src_idx in source_indices {
                graph[src_idx].push(target_idx);
            }
        }
    }

    // DFS to detect cycles
    let mut visited = vec![false; param_count];
    let mut rec_stack = vec![false; param_count];

    fn dfs(
        node: usize,
        graph: &[Vec<usize>],
        visited: &mut [bool],
        rec_stack: &mut [bool],
        params: &[Parameter],
    ) -> Result<(), String> {
        visited[node] = true;
        rec_stack[node] = true;

        for &neighbor in &graph[node] {
            if !visited[neighbor] {
                dfs(neighbor, graph, visited, rec_stack, params)?;
            } else if rec_stack[neighbor] {
                return Err(format!(
                    "Cyclic dependency detected involving parameter '{}'",
                    params[neighbor].name
                ));
            }
        }

        rec_stack[node] = false;
        Ok(())
    }

    for i in 0..param_count {
        if !visited[i] {
            dfs(i, &graph, &mut visited, &mut rec_stack, params)?;
        }
    }

    Ok(())
}

/// Validate and compile all constraints
///
/// First checks for cyclic dependencies, then compiles all constraint expressions.
/// Returns an error if validation fails or compilation fails.
pub fn validate_constraints(
    constraints: &mut [ParameterConstraint],
    params: &[Parameter],
) -> Result<(), String> {
    // First check for cycles
    detect_cycles(constraints, params)?;

    // Extract parameter names
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();

    // Compile all constraints
    for constraint in constraints.iter_mut() {
        constraint.compile(&param_names)?;
    }

    Ok(())
}
