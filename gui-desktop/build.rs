fn main() {
    println!("cargo:rerun-if-changed=../gui-prototype");
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().window_icon_path("../crates/athanor-install/assets/athanor.ico"),
    )).expect("build Pulse desktop resources");
}
