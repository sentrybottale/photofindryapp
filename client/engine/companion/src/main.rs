#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This PhotoFindry companion currently supports macOS only.");
    std::process::exit(2);
}
