fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("src/ui/app.slint", config).unwrap();

    // The exe's own icon (Explorer, Start, shortcuts); the window's is app.slint's `icon`.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/twinfm.ico");
        res.compile().expect("embedding the exe icon");
    }
    println!("cargo:rerun-if-changed=assets/icon/twinfm.ico");
}
