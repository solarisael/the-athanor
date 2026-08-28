fn main() {
    // The operator-visible face: athanor.exe (and the manager beside it)
    // carry the House's icon. Resource embedding is a Windows concept; other
    // targets compile this crate without it.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        println!("cargo:rerun-if-changed=assets/athanor.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/athanor.ico")
            .compile()
            .expect("embed athanor.ico as the Windows application icon");
    }
}
