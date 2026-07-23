fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/rheogram.ico");
        res.set("CompanyName", "RPG");
        res.set("FileDescription", "COM Port Data Plotter (RKM / RKMS)");
        res.set("ProductName", "COM Port Plotter");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        if let Err(e) = res.compile() {
            eprintln!("warning: winres failed: {e}");
        }
    }
}
