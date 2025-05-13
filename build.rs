
#[cfg(all(target_os = "windows", feature = "standalone"))]
fn main() {
    let res = winresource::WindowsResource::new();
    res.compile().unwrap();
}

#[cfg(not(feature = "standalone"))]
fn main() {
    // Do nothing on non-Windows platforms
}