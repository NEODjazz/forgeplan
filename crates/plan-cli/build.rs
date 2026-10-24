fn main() {
    let target = std::env::var("TARGET").expect("Cargo supplies the build target");
    println!("cargo:rustc-env=FORGEPLAN_BUILD_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
