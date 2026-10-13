fn main() {
    println!("cargo:rerun-if-changed=assets/windows.rc");
    println!("cargo:rerun-if-changed=assets/windows.manifest");
    println!("cargo:rerun-if-changed=assets/tervia.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var_os("CARGO_FEATURE_NATIVE").is_some()
    {
        embed_resource::compile(
            "assets/windows.rc",
            embed_resource::ParamsIncludeDirs(["assets"]),
        )
        .manifest_required()
        .expect("Embed the Windows desktop manifest and icon");
    }
}
