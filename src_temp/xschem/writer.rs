use std::io::Result as IoResult;
use std::path::Path;
use std::io::BufWriter;
use std::io::Write;
use std::fs::File;

use crate::xschem::objects::XSchemObject;

pub fn write_file<P: AsRef<Path>>(objects: &[XSchemObject], file_path: P) -> IoResult<()> {
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);
    
    let mut is_first = true;
    for obj in objects {
        let line = obj.format();
        if !line.is_empty() {
            if !is_first {
                writer.write_all(b"\n")?;
            }
            writer.write_all(line.as_bytes())?;
            is_first = false;
        }
    }
    
    // Ensure file ends with a newline if we wrote anything
    if !is_first {
        writer.write_all(b"\n")?;
    }
    
    writer.flush()
}
