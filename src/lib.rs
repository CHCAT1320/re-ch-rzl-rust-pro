#![cfg(target_os = "android")]
#![allow(dead_code)]

include!("main.rs");

#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    main();
}
