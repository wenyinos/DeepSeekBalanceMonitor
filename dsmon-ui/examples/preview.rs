//! Launches the interface without going through a platform binary.
//!
//! Handy during development: `cargo run -p dsmon-ui --example preview`.

fn main() -> eframe::Result<()> {
    dsmon_ui::run()
}
