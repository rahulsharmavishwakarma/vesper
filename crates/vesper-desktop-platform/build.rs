fn main() {
    println!("cargo:rerun-if-changed=src/projects.m");
    println!("cargo:rerun-if-changed=src/macos.m");
    println!("cargo:rerun-if-changed=src/background.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/macos.m")
            .file("src/background.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("vesper_desktop_platform");
        for framework in [
            "AppKit",
            "ApplicationServices",
            "AVFoundation",
            "Carbon",
            "IOKit",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
