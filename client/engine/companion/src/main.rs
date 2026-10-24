#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(any(target_os = "linux", all(test, unix)))]
mod linux;

#[cfg(target_os = "linux")]
fn main() {
    linux::main();
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
fn main() {
    windows::main();
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn main() {
    eprintln!("This PhotoFindry companion supports macOS and a private Linux development build.");
    std::process::exit(2);
}
