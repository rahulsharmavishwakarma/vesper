use std::process::Command;

fn main() {
    link_compiler_rt();
    tauri_build::build();
}

// ggml's Metal backend (llama-cpp-sys-2 >= 0.1.160) uses Objective-C
// `@available`, which lowers to the compiler-rt builtin
// `___isPlatformVersionAtLeast`. The thin-LTO release link does not pull in
// `libclang_rt.osx` on its own, so link it explicitly from the active toolchain.
fn link_compiler_rt() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let output = Command::new("clang")
        .arg("--print-resource-dir")
        .output()
        .expect("clang is required to locate libclang_rt.osx");
    assert!(output.status.success(), "clang --print-resource-dir failed");
    let resource_dir = String::from_utf8(output.stdout).expect("clang printed non-UTF-8 path");
    println!(
        "cargo:rustc-link-search=native={}/lib/darwin",
        resource_dir.trim()
    );
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
