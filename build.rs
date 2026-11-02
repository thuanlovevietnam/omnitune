fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("app.ico");
        res.set("ProductName", "OmniTune");
        res.set("FileDescription", "OmniTune - Autonomous System Assistant");
        res.set("LegalCopyright", "© 2026 thuanlovevietnam");
        let _ = res.compile();
    }
}
