use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=include/ngspice.h");

    // Link to ngspice shared library
    println!("cargo:rustc-link-lib=ngspice");

    // Generate bindings
    let bindings = bindgen::Builder::default()
        .header("include/ngspice.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("ngSpice_Init")
        .allowlist_function("ngSpice_Init_Sync")
        .allowlist_function("ngSpice_Command")
        .allowlist_function("ngSpice_Circ")
        .allowlist_function("ngGet_Vec_Info")
        .allowlist_function("ngSpice_CurPlot")
        .allowlist_function("ngSpice_AllPlots")
        .allowlist_function("ngSpice_AllVecs")
        .allowlist_function("ngSpice_running")
        .allowlist_function("ngSpice_SetBkpt")
        .allowlist_type("ngcomplex_t")
        .allowlist_type("vector_info")
        .allowlist_type("vecvalues")
        .allowlist_type("vecvaluesall")
        .allowlist_type("vecinfo")
        .allowlist_type("vecinfoall")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("ngspice_bindings.rs"))
        .expect("Couldn't write bindings!");
}
