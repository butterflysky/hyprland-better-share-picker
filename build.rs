use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest = out_dir.join("hyprland_toplevel_export.rs");

    let vendored = Path::new("third_party/hyprland-protocols/hyprland-toplevel-export-v1.xml");
    let root = Path::new("hyprland-toplevel-export-v1.xml");
    let xml_path = if vendored.exists() { vendored } else { root };

    let contents = r#"
// Generated via build.rs. The actual bindings are produced by wayland-scanner
// at compile time using the XML protocol from the project root.
pub mod hyprland_toplevel_export {
    use bitflags as bitflags;
    use wayland_backend as wayland_backend;
    use wayland_client as wayland_client;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("./hyprland-toplevel-export-v1.xml");
    }

    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("./hyprland-toplevel-export-v1.xml");
}
"#;

    let contents = contents.replace("./hyprland-toplevel-export-v1.xml", &xml_path.display().to_string());
    fs::write(dest, contents).expect("failed to write generated protocol module");
    println!("cargo:rerun-if-changed=hyprland-toplevel-export-v1.xml");
    println!("cargo:rerun-if-changed=third_party/hyprland-protocols/hyprland-toplevel-export-v1.xml");
}
