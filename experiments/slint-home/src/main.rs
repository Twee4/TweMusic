//! A trial of TweMusic's Home screen in Slint. It shows the same layout as the egui version, with
//! sample songs and no playback, so the two toolkits can be compared side by side.

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = HomeWindow::new()?;
    window.on_play(|title| println!("play: {title}"));
    window.on_navigate(|index| println!("navigate: {index}"));
    window.run()
}
