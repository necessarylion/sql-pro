fn main() {
    // Tauri puts the app icon into the program when it compiles, but it does not
    // watch the icon files. Without this line a new icon only shows after some
    // other change forces a new build.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build();
}
