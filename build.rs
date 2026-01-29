fn main() {
    // Only run this if we are compiling for Windows
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icons/icon.ico");
        res.compile().unwrap();
    }
}
