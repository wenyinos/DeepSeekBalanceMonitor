//! Launches the desktop widget during development.
//!
//! `cargo run -p dsmon-ui --example widget_preview` opens the same window the
//! `dsmon2-widget` binary does, without going through the platform entry point.
//! With the application running it shows the readings; without it, the card
//! that says so — which is the state worth looking at when working on the panel.

fn main() -> eframe::Result<()> {
    dsmon_ui::widget::run()
}
