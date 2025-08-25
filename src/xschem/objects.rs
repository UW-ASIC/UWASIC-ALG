use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq)]
pub enum XSchemObject {
    Version(Version),
    Component(Component),
    Wire(Wire),
    Text(Text),
    Header(Header),
    Section(Section),
    Line(Line),
    Rectangle(Rectangle),
    Arc(Arc),
    Polygon(Polygon),
    Spice(Spice),
    Verilog(Verilog),
    VHDL(VHDL),
    TEDAx(TEDAx),
    GlobalProperties(GlobalProperties),
    EmbeddedSymbol(EmbeddedSymbol),
}

impl XSchemObject {
    pub fn format(&self) -> String {
        match self {
            XSchemObject::Version(version) => version.format(),
            XSchemObject::Header(header) => header.format(),
            XSchemObject::Section(section) => section.format(),
            XSchemObject::Line(line) => line.format(),
            XSchemObject::Rectangle(rect) => rect.format(),
            XSchemObject::Arc(arc) => arc.format(),
            XSchemObject::Polygon(poly) => poly.format(),
            XSchemObject::Wire(wire) => wire.format(),
            XSchemObject::Component(comp) => comp.format(),
            XSchemObject::Text(text) => text.format(),
            XSchemObject::Spice(spice) => spice.format(),
            XSchemObject::Verilog(verilog) => verilog.format(),
            XSchemObject::VHDL(vhdl) => vhdl.format(),
            XSchemObject::TEDAx(tedax) => tedax.format(),
            XSchemObject::GlobalProperties(global) => global.format(),
            XSchemObject::EmbeddedSymbol(embedded) => embedded.format(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub version: String,
    pub file_version: String,
    pub license: String,
}

impl Version {
    pub fn format(&self) -> String {
        let version_line = format!("xschem version={} file_version={}", self.version, self.file_version);
        if !self.license.is_empty() && self.license.trim() != version_line.trim() {
            format!("v {{{}\n}}", self.license)
        } else {
            format!("v {{{}\n}}", version_line)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Component {
    pub symbol_reference: String,
    pub x: f64,
    pub y: f64,
    pub rotation: i32,
    pub flip: i32,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Component to handle floating point comparison
impl PartialEq for Component {
    fn eq(&self, other: &Self) -> bool {
        self.symbol_reference == other.symbol_reference &&
        float_eq(self.x, other.x) &&
        float_eq(self.y, other.y) &&
        self.rotation == other.rotation &&
        self.flip == other.flip &&
        self.properties == other.properties
    }
}

impl Component {
    pub fn new(symbol_ref: &str, x: f64, y: f64) -> Self {
        Self {
            symbol_reference: symbol_ref.to_string(),
            x,
            y,
            rotation: 0,
            flip: 0,
            properties: IndexMap::new(),
        }
    }

    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("C {{{}}} {} {} {} {} {}", 
            self.symbol_reference, 
            format_number(self.x), 
            format_number(self.y), 
            self.rotation, 
            self.flip, 
            props)
    }
}

#[derive(Debug, Clone)]
pub struct Wire {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub points: Vec<(f64, f64)>,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Wire to handle floating point comparison
impl PartialEq for Wire {
    fn eq(&self, other: &Self) -> bool {
        float_eq(self.x1, other.x1) &&
        float_eq(self.y1, other.y1) &&
        float_eq(self.x2, other.x2) &&
        float_eq(self.y2, other.y2) &&
        self.points.len() == other.points.len() &&
        self.points.iter().zip(other.points.iter()).all(|((x1, y1), (x2, y2))| {
            float_eq(*x1, *x2) && float_eq(*y1, *y2)
        }) &&
        self.properties == other.properties
    }
}

impl Wire {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            points: vec![(x1, y1), (x2, y2)],
            properties: IndexMap::new(),
        }
    }

    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("N {} {} {} {} {}", 
            format_number(self.x1), 
            format_number(self.y1), 
            format_number(self.x2), 
            format_number(self.y2), 
            props)
    }
}

#[derive(Debug, Clone)]
pub struct Text {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub size: f64,
    pub rotation: i32,
    pub mirror: i32,
    pub h_size: f64,
    pub v_size: f64,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Text to handle floating point comparison
impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        float_eq(self.x, other.x) &&
        float_eq(self.y, other.y) &&
        self.text == other.text &&
        float_eq(self.size, other.size) &&
        self.rotation == other.rotation &&
        self.mirror == other.mirror &&
        float_eq(self.h_size, other.h_size) &&
        float_eq(self.v_size, other.v_size) &&
        self.properties == other.properties
    }
}

impl Text {
    pub fn new(text: &str, x: f64, y: f64, size: f64) -> Self {
        Self {
            text: text.to_string(),
            x,
            y,
            size,
            rotation: 0,
            mirror: 0,
            h_size: size,
            v_size: size,
            properties: IndexMap::new(),
        }
    }

    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("T {{{}}} {} {} {} {} {} {} {}", 
            self.text, 
            format_number(self.x), 
            format_number(self.y), 
            self.rotation, 
            self.mirror, 
            format_number(self.h_size), 
            format_number(self.v_size), 
            props)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub version: String,
}

impl Header {
    pub fn format(&self) -> String {
        format!("v {{xschem version={} file_version=1.2}}", self.version)
    }
}

#[derive(Debug, Clone)]
pub struct Section {
    pub section_type: String, // G, K, V, S, E
    pub content: String,
}

// Custom PartialEq for Section to handle whitespace normalization
impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        self.section_type == other.section_type &&
        self.content.trim() == other.content.trim()
    }
}

impl Section {
    pub fn format(&self) -> String {
        format!("{} {{{}}}", self.section_type, self.content)
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    pub layer: i32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Line to handle floating point comparison
impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer &&
        float_eq(self.x1, other.x1) &&
        float_eq(self.y1, other.y1) &&
        float_eq(self.x2, other.x2) &&
        float_eq(self.y2, other.y2) &&
        self.properties == other.properties
    }
}

impl Line {
    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("L {} {} {} {} {} {}", 
            self.layer, 
            format_number(self.x1), 
            format_number(self.y1), 
            format_number(self.x2), 
            format_number(self.y2), 
            props)
    }
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub layer: i32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Rectangle to handle floating point comparison
impl PartialEq for Rectangle {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer &&
        float_eq(self.x1, other.x1) &&
        float_eq(self.y1, other.y1) &&
        float_eq(self.x2, other.x2) &&
        float_eq(self.y2, other.y2) &&
        self.properties == other.properties
    }
}

impl Rectangle {
    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("B {} {} {} {} {} {}", 
            self.layer, 
            format_number(self.x1), 
            format_number(self.y1), 
            format_number(self.x2), 
            format_number(self.y2), 
            props)
    }
}

#[derive(Debug, Clone)]
pub struct Arc {
    pub layer: i32,
    pub center_x: f64,
    pub center_y: f64,
    pub radius: f64,
    pub start_angle: f64,
    pub sweep_angle: f64,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Arc to handle floating point comparison
impl PartialEq for Arc {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer &&
        float_eq(self.center_x, other.center_x) &&
        float_eq(self.center_y, other.center_y) &&
        float_eq(self.radius, other.radius) &&
        float_eq(self.start_angle, other.start_angle) &&
        float_eq(self.sweep_angle, other.sweep_angle) &&
        self.properties == other.properties
    }
}

impl Arc {
    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("A {} {} {} {} {} {} {}", 
            self.layer, 
            format_number(self.center_x), 
            format_number(self.center_y), 
            format_number(self.radius), 
            format_number(self.start_angle), 
            format_number(self.sweep_angle), 
            props)
    }
}

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// Custom PartialEq for Point to handle floating point comparison
impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        float_eq(self.x, other.x) && float_eq(self.y, other.y)
    }
}

#[derive(Debug, Clone)]
pub struct Polygon {
    pub layer: i32,
    pub point_count: i32,
    pub points: Vec<Point>,
    pub properties: IndexMap<String, String>,
}

// Custom PartialEq for Polygon to handle floating point comparison
impl PartialEq for Polygon {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer &&
        self.point_count == other.point_count &&
        self.points == other.points &&
        self.properties == other.properties
    }
}

impl Polygon {
    pub fn format(&self) -> String {
        let mut coords = Vec::new();
        for point in &self.points {
            coords.push(format_number(point.x));
            coords.push(format_number(point.y));
        }
        let coord_str = coords.join(" ");
        let props = format_properties(&self.properties);
        format!("P {} {} {} {}", self.layer, self.point_count, coord_str, props)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spice {
    pub content: String,
}

impl Spice {
    pub fn format(&self) -> String {
        format!("S {{{}}}", self.content)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verilog {
    pub content: String,
}

impl Verilog {
    pub fn format(&self) -> String {
        format!("V {{{}}}", self.content)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VHDL {
    pub content: String,
}

impl VHDL {
    pub fn format(&self) -> String {
        format!("G {{{}}}", self.content)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TEDAx {
    pub content: String,
}

impl TEDAx {
    pub fn format(&self) -> String {
        format!("E {{{}}}", self.content)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalProperties {
    pub properties: IndexMap<String, String>,
}

impl GlobalProperties {
    pub fn format(&self) -> String {
        let props = format_properties(&self.properties);
        format!("K {}", props)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedSymbol {
    pub symbol: Vec<XSchemObject>,
}

impl EmbeddedSymbol {
    pub fn format(&self) -> String {
        let mut embedded_content = Vec::new();
        for sub_obj in &self.symbol {
            let line = sub_obj.format();
            if !line.is_empty() {
                embedded_content.push(line);
            }
        }
        let content = embedded_content.join("\n");
        format!("[\n{}\n]", content)
    }
}

// Helper function for floating point comparison with tolerance
fn float_eq(a: f64, b: f64) -> bool {
    const EPSILON: f64 = 1e-6;
    (a - b).abs() < EPSILON
}

// Helper functions for formatting
pub fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{:.3}", value)
    }
}

pub fn format_properties(properties: &IndexMap<String, String>) -> String {
    if properties.is_empty() {
        return "{}".to_string();
    }
    
    let mut formatted_pairs = Vec::new();
    for (key, value) in properties {
        // If value contains spaces or special chars, wrap in quotes
        if value.contains(' ') || value.contains('(') || value.contains(')') || value.contains('/') {
            formatted_pairs.push(format!("{}=\"{}\"", key, value));
        } else {
            formatted_pairs.push(format!("{}={}", key, value));
        }
    }
    
    if formatted_pairs.len() > 2 {
        format!("{{\n{}\n}}", formatted_pairs.join("\n"))
    } else {
        format!("{{ {} }}", formatted_pairs.join(" "))
    }
}
