fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("src/ui/app.slint", config).unwrap();

    // The exe's own icon (Explorer, Start, shortcuts); the window's is app.slint's `icon`. The
    // manifest asks only for the segment heap (the Phase 12 memory follow-up); before it the exe had
    // no manifest, so nothing else about how Windows treats it changes.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/twinfm.ico");
        res.set_manifest_file("assets/twinfm.exe.manifest");
        res.compile().expect("embedding the exe icon and manifest");
    }
    println!("cargo:rerun-if-changed=assets/icon/twinfm.ico");
    println!("cargo:rerun-if-changed=assets/twinfm.exe.manifest");
}
