use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest = out_dir.join("hyprland_toplevel_export.rs");

    let contents = r#"
// Generated via build.rs. The actual bindings are produced by wayland-scanner
// at compile time using the XML protocol from the project root.
pub mod hyprland_toplevel_export {
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("./hyprland-toplevel-export-v1.xml");
    }

    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("./hyprland-toplevel-export-v1.xml");
}
"#;

    fs::write(dest, contents).expect("failed to write generated protocol module");
    println!("cargo:rerun-if-changed=hyprland-toplevel-export-v1.xml");
}
