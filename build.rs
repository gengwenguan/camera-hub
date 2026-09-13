use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_VOICE_WORKERS");
    if env::var_os("CARGO_FEATURE_VOICE_WORKERS").is_none() {
        return;
    }

    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/camera-hub-voice");
        }
        Ok("macos") => {
            println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../lib/camera-hub-voice");
        }
        _ => {}
    }
}
