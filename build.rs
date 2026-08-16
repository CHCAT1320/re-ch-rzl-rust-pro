fn main() {
    println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:NvOptimusEnablement");
    println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:AmdPowerXpressRequestHighPerformance");
}
