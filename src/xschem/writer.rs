use std::fs::write;
use std::io::Result as IoResult;
use std::path::Path;

use crate::xschem::objects::XSchemObject;

pub fn write_file<P: AsRef<Path>>(objects: &[XSchemObject], file_path: P) -> IoResult<()> {
    let content = {
        let mut lines = Vec::new();
        
        for obj in objects {
            let line = obj.format();
            if !line.is_empty() {
                lines.push(line);
            }
        }
        
        // Ensure file ends with a newline
        let mut content = lines.join("\n");
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        
        content
    };

    write(file_path, content)
}
