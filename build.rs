fn main() {
    // Embed Windows resources (icon and manifest)
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        embed_resource::compile("resources/app.rc", embed_resource::NONE);
    }
}
