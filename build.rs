fn generate_manifest(require_admin: bool) -> String {
    let level = if require_admin {
        "requireAdministrator"
    } else {
        "asInvoker"
    };

    format!(
        r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="{level}" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
<compatibility xmlns="urn:schemas-microsoft-com:asm.v1">
    <application>
        <!-- Windows 10 & Windows 11 -->
        <supportedOS Id="{{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}}" />
    </application>
</compatibility>
<application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
        <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
        <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
</application>
</assembly>
"#
    )
}

fn main() {
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=OMNITUNE_FORCE_ADMIN");
    println!("cargo:rerun-if-env-changed=OMNITUNE_NO_ADMIN");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let profile = std::env::var("PROFILE").unwrap_or_default();
        let is_release = profile == "release";
        let force_admin = std::env::var("OMNITUNE_FORCE_ADMIN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let no_admin = std::env::var("OMNITUNE_NO_ADMIN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let require_admin = (is_release || force_admin) && !no_admin;

        let mut res = winres::WindowsResource::new();
        res.set_icon("app.ico");
        res.set("ProductName", "OmniTune");
        res.set("FileDescription", "OmniTune - Autonomous System Assistant");
        res.set("LegalCopyright", "© 2026 thuanlovevietnam");
        res.set_manifest(&generate_manifest(require_admin));
        let _ = res.compile();
    }
}
