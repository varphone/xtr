
#[cfg(target_os = "windows")]
fn main() {
    let res = winresource::WindowsResource::new();
    res.compile().unwrap();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    // Do nothing on non-Windows platforms
}