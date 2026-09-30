fn main() {
    for name in ["LOBO_VERSION", "LOBO_COMMIT", "LOBO_DATE"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    tauri_build::build()
}
