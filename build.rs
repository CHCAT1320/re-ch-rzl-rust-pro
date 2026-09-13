fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows") {
        return;
    }

    println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:NvOptimusEnablement");
    println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:AmdPowerXpressRequestHighPerformance");
}
