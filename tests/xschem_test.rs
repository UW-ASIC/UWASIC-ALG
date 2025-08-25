#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use xschemoptimizer::XSchemIO;

    #[test]
    fn test_roundtrip_eg_sch() {
        let file_path = "tests/test_sample/eg.sch";
        
        // Verify the file exists
        assert!(Path::new(file_path).exists(), "eg.sch should exist in test_sample directory");

        // Step 1: Load original file
        let original = XSchemIO::load(file_path).expect("Failed to load eg.sch");
        let original_objects = original.get_all_objects();

        // Step 2: Write to temporary file
        let temp_file = "tests/test_sample/eg_temp1.sch";
        original.save(temp_file).expect("Failed to save to temporary file");

        // Step 3: Load the written file
        let reloaded = XSchemIO::load(temp_file).expect("Failed to reload from temporary file");
        let reloaded_objects = reloaded.get_all_objects();

        // Step 4: Compare objects using PartialEq
        assert_eq!(
            original_objects, reloaded_objects,
            "Round-trip failed for eg.sch - objects don't match after write/read cycle"
        );

        // Step 5: Test second round-trip to ensure stability
        let temp_file2 = "tests/test_sample/eg_temp2.sch";
        reloaded.save(temp_file2).expect("Failed to save second temporary file");
        let second_reload = XSchemIO::load(temp_file2).expect("Failed to reload second temporary file");
        
        assert_eq!(
            reloaded_objects, second_reload.get_all_objects(),
            "Second round-trip failed for eg.sch - format not stable"
        );

        // Clean up
        let _ = fs::remove_file(temp_file);
        let _ = fs::remove_file(temp_file2);

        println!("✓ eg.sch round-trip test passed successfully");
    }

    #[test]
    fn test_roundtrip_eg_tb_sch() {
        let file_path = "tests/test_sample/eg_tb.sch";
        
        assert!(Path::new(file_path).exists(), "eg_tb.sch should exist in test_sample directory");

        let original = XSchemIO::load(file_path).expect("Failed to load eg_tb.sch");
        let original_objects = original.get_all_objects();

        let temp_file = "tests/test_sample/eg_tb_temp1.sch";
        original.save(temp_file).expect("Failed to save to temporary file");

        let reloaded = XSchemIO::load(temp_file).expect("Failed to reload from temporary file");
        let reloaded_objects = reloaded.get_all_objects();

        assert_eq!(
            original_objects, reloaded_objects,
            "Round-trip failed for eg_tb.sch - objects don't match after write/read cycle"
        );

        // Test second round-trip
        let temp_file2 = "tests/test_sample/eg_tb_temp2.sch";
        reloaded.save(temp_file2).expect("Failed to save second temporary file");
        let second_reload = XSchemIO::load(temp_file2).expect("Failed to reload second temporary file");
        
        assert_eq!(
            reloaded_objects, second_reload.get_all_objects(),
            "Second round-trip failed for eg_tb.sch - format not stable"
        );

        let _ = fs::remove_file(temp_file);
        let _ = fs::remove_file(temp_file2);

        println!("✓ eg_tb.sch round-trip test passed successfully");
    }

    #[test]
    fn test_component_finding_after_roundtrip() {
        let file_path = "tests/test_sample/eg.sch";
        assert!(Path::new(file_path).exists(), "eg.sch should exist");

        // Load original and find first component
        let original = XSchemIO::load(file_path).expect("Failed to load original file");
        let original_components = original.get_components();
        
        if original_components.is_empty() {
            println!("Warning: No components found in eg.sch, skipping component finding test");
            return;
        }

        let first_component = original_components[0];
        let component_name = first_component.properties.get("name");

        // Perform round-trip
        let temp_file = "tests/test_sample/component_test_temp.sch";
        original.save(temp_file).expect("Failed to save temp file");
        let mut reloaded = XSchemIO::load(temp_file).expect("Failed to reload temp file");

        // Test finding by symbol reference
        let found_by_symbol = reloaded.find_component_by_symbol(&first_component.symbol_reference);
        assert!(found_by_symbol.is_some(), "Could not find component by symbol reference after round-trip");
        assert_eq!(found_by_symbol.unwrap(), first_component, "Found component doesn't match original");

        // Test finding by name if available
        if let Some(name) = component_name {
            let found_by_name = reloaded.find_component_by_name(name);
            assert!(found_by_name.is_some(), "Could not find component by name after round-trip");
            assert_eq!(found_by_name.unwrap(), first_component, "Found component by name doesn't match original");
        }

        // Test mutable finding
        let found_by_symbol_mut = reloaded.find_component_by_symbol_mut(&first_component.symbol_reference);
        assert!(found_by_symbol_mut.is_some(), "Could not find mutable component by symbol reference after round-trip");

        let _ = fs::remove_file(temp_file);

        println!("✓ Component finding test passed successfully");
    }

    #[test]
    fn test_wire_and_text_roundtrip() {
        let file_path = "tests/test_sample/eg.sch";
        assert!(Path::new(file_path).exists(), "eg.sch should exist");

        let original = XSchemIO::load(file_path).expect("Failed to load original file");
        
        // Check if we have wires and texts
        let original_wires = original.get_wires();
        println!("Found {} wires in original file", original_wires.len());

        // Perform round-trip
        let temp_file = "tests/test_sample/wire_text_temp.sch";
        original.save(temp_file).expect("Failed to save temp file");
        let reloaded = XSchemIO::load(temp_file).expect("Failed to reload temp file");

        let reloaded_wires = reloaded.get_wires();
        println!("Found {} wires after round-trip", reloaded_wires.len());

        assert_eq!(original_wires.len(), reloaded_wires.len(), "Wire count mismatch after round-trip");
        
        // Compare each wire
        for (orig_wire, reload_wire) in original_wires.iter().zip(reloaded_wires.iter()) {
            assert_eq!(orig_wire, reload_wire, "Wire mismatch after round-trip");
        }

        // Test the complete object comparison as well
        assert_eq!(
            original.get_all_objects(), 
            reloaded.get_all_objects(),
            "Complete object comparison failed"
        );

        let _ = fs::remove_file(temp_file);

        println!("✓ Wire and text round-trip test passed successfully");
    }

    #[test]
    fn test_spice_setup_after_roundtrip() {
        let file_path = "tests/test_sample/eg_tb.sch";  // Testbench likely has SPICE components
        assert!(Path::new(file_path).exists(), "eg_tb.sch should exist");

        let original = XSchemIO::load(file_path).expect("Failed to load original file");
        
        // Save and reload
        let temp_file = "tests/test_sample/spice_test_temp.sch";
        original.save(temp_file).expect("Failed to save temp file");
        let mut reloaded = XSchemIO::load(temp_file).expect("Failed to reload temp file");

        // Test ensure_spice_setup functionality
        let spice_component = reloaded.ensure_spice_setup();
        assert_eq!(spice_component.symbol_reference, "devices/code_shown.sym");
        
        // Save again and verify SPICE components persist
        let temp_file2 = "tests/test_sample/spice_test_temp2.sch";
        reloaded.save(temp_file2).expect("Failed to save after SPICE setup");
        let final_reload = XSchemIO::load(temp_file2).expect("Failed to final reload");

        // Verify SPICE components exist
        let corner_component = final_reload.find_component_by_symbol("sky130_fd_pr/corner.sym");
        let code_component = final_reload.find_component_by_symbol("devices/code_shown.sym");
        
        assert!(corner_component.is_some(), "Corner component should exist after round-trip");
        assert!(code_component.is_some(), "Code component should exist after round-trip");

        let _ = fs::remove_file(temp_file);
        let _ = fs::remove_file(temp_file2);

        println!("✓ SPICE setup round-trip test passed successfully");
    }
}
