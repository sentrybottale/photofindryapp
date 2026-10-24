#![cfg(unix)] // This fixture exercises the retained Unix/macOS transport only.
// Compile and exercise the isolated handover gate before CLI launch wiring.
#[path = "../src/handover.rs"]
mod handover;
