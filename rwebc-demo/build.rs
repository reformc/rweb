fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set("ProductName", "rwebc-demo")
           .set("FileDescription", "只是个demo")
           .set("CompanyName", "reformc");
        res.compile().unwrap();
    }
}
