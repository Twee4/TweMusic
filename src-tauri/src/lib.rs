use tauri::{WebviewUrl, WebviewWindowBuilder};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("https://music.youtube.com".parse().unwrap()),
            )
            .title("TweMusic")
            .inner_size(1280.0, 820.0)
            .min_inner_size(960.0, 640.0)
            .resizable(true)
            .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri app");
}
