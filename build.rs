use bindgen::Builder;
use std::{env, path::PathBuf};

fn main() {
    let bindings = Builder::default()
        .header("deadbeef.h")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not defined"));
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
