use std::{env, path::Path};

fn main() {
    // src/state.rs trusts the development state root only for an executable
    // inside the target directory of this build. Cargo does not tell a crate
    // that directory, so derive it from OUT_DIR, which cargo places at
    // <target-dir>/<profile>/build/<package>-<hash>/out. With --target the
    // answer is <target-dir>/<triple>, which holds the binaries of that build.
    //
    // enough: cargo does not promise this layout. When a toolchain changes it,
    // the state test development_root_is_the_compiled_checkout_state_dir fails.
    let out_dir = env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for a build script");
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(4)
        .expect("OUT_DIR is four directories below the target directory");

    println!(
        "cargo:rustc-env=ATHANOR_BUILD_TARGET_DIR={}",
        target_dir.display()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
