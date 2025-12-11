fn main() {
    println!("cargo:rustc-link-lib=oldnames");
    tauri_build::build()
}
