use std::{fs, path::Path};

fn main() {
    // Tauri's generated context expects this file even when bundling is disabled.
    // Keep an existing RGBA project icon intact; create a transparent placeholder
    // when the icon is missing or is in a format Tauri cannot load.
    let icon_dir = Path::new("icons");
    let icon_path = icon_dir.join("icon.png");

    if !is_rgba_png(&icon_path) {
        fs::create_dir_all(icon_dir).expect("failed to create Tauri icons directory");
        fs::write(&icon_path, transparent_rgba_png())
            .expect("failed to create placeholder Tauri icon");
    }

    tauri_build::build()
}

fn is_rgba_png(path: &Path) -> bool {
    fs::read(path)
        .map(|png| {
            png.len() >= 26
                && png.starts_with(b"\x89PNG\r\n\x1a\n")
                && &png[12..16] == b"IHDR"
                && png[25] == 6 // PNG color type 6 is RGBA.
        })
        .unwrap_or(false)
}

// Build a valid 1x1 transparent RGBA PNG without adding a build dependency.
fn transparent_rgba_png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let ihdr = [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0];
    append_png_chunk(&mut png, b"IHDR", &ihdr);

    // Zlib stream containing one uncompressed scanline: filter byte + RGBA pixel.
    let idat = [
        0x78, 0x01, // zlib header
        0x01, 0x05, 0x00, 0xfa, 0xff, // final uncompressed DEFLATE block, 5 bytes
        0x00, 0x00, 0x00, 0x00, 0x00, // transparent pixel and filter byte
        0x00, 0x06, 0x00, 0x01, // Adler-32 checksum
    ];
    append_png_chunk(&mut png, b"IDAT", &idat);
    append_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn append_png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);

    let mut crc = !0_u32;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
