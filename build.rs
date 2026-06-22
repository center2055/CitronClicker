// embed the citron icon into the exe (windows only)
fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/branding/citron.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/branding/citron.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon embed failed: {e}");
        }
    }
}
