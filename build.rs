fn main() {
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=mpv");
        println!("cargo:rustc-link-lib=EGL");
        println!("cargo:rustc-link-lib=GL");
        println!("cargo:rustc-link-lib=wayland-client");
    }
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=mpv");
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        println!("cargo:rustc-link-search=native={manifest_dir}");
        println!("cargo:rustc-link-search=native={manifest_dir}/target/debug");
        println!("cargo:rustc-link-search=native={manifest_dir}/target/release");
    }
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=mpv");
    }
}
