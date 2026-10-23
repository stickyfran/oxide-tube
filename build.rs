fn main() {
    println!("cargo:rustc-link-lib=mpv");
    println!("cargo:rustc-link-lib=EGL");
    println!("cargo:rustc-link-lib=GL");
    println!("cargo:rustc-link-lib=wayland-client");
}
