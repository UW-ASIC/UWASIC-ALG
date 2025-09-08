use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct ParsedTest {
    #[serde(flatten)]
    pub component_values: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTarget {
    pub metric: String,
    pub target_value: f64,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default = "default_constraint_type")]
    pub constraint_type: String,
}

fn default_weight() -> f64 { 1.0 }
fn default_constraint_type() -> String { "eq".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBound {
    pub component: String,
    pub parameter: String,
    pub min_value: f64,
    pub max_value: f64,
}
