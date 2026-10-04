fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "windows" {
        println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:NvOptimusEnablement");
        println!("cargo:rustc-link-arg-bin=re-ch-rzl-rust=/EXPORT:AmdPowerXpressRequestHighPerformance");
        return;
    }
    if os != "android" {
        return;
    }

    let ndk = std::env::var("ANDROID_NDK_HOME")
        .or_else(|_| std::env::var("NDK_HOME"))
        .unwrap_or_default();
    if ndk.is_empty() {
        return;
    }
    let target = std::env::var("TARGET").unwrap_or_else(|_| "aarch64-linux-android".into());
    let triple = target.split('-').next().unwrap_or("aarch64");
    let host = if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    let lib_dir = format!(
        "{ndk}/toolchains/llvm/prebuilt/{host}/sysroot/usr/lib/{triple}-linux-android/26"
    );
    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=aaudio");
    for symbol in [
        "quad_main",
        "JNI_OnLoad",
        "Java_quad_1native_QuadNative_activityOnCreate",
        "Java_quad_1native_QuadNative_activityOnResume",
        "Java_quad_1native_QuadNative_activityOnPause",
        "Java_quad_1native_QuadNative_activityOnDestroy",
        "Java_quad_1native_QuadNative_surfaceOnSurfaceCreated",
        "Java_quad_1native_QuadNative_surfaceOnSurfaceDestroyed",
        "Java_quad_1native_QuadNative_surfaceOnSurfaceChanged",
        "Java_quad_1native_QuadNative_surfaceOnTouch",
        "Java_quad_1native_QuadNative_surfaceOnKeyDown",
        "Java_quad_1native_QuadNative_surfaceOnKeyUp",
        "Java_quad_1native_QuadNative_surfaceOnCharacter",
        "Java_io_github_chcat1320_rechrzl_MainActivity_nativeSetActivity",
        "Java_io_github_chcat1320_rechrzl_MainActivity_nativePickResult",
        "Java_io_github_chcat1320_rechrzl_MainActivity_nativePlayChoice",
    ] {
        println!("cargo:rustc-link-arg=-Wl,-u,{symbol}");
    }
}
