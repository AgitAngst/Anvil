//! Значок программы — PNG из `assets/icon` (нарисованы из SVG рядом, см. README там же). Отсюда же:
//! ресурс exe (ico), сырые RGBA для окна и трея и PNG для уведомлений Windows. Файлы в `OUT_DIR`
//! пишутся на любой ОС: `include_bytes!` в коде от неё не зависит.

use std::path::{Path, PathBuf};

use image::ExtendedColorType;
use image::codecs::ico::{IcoEncoder, IcoFrame};

/// Размеры в ico: мелкие — упрощённый знак, от 48 — полный.
const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// RGBA файла `name`, квадрат `size`×`size`.
fn rgba(dir: &Path, name: &str, size: u32) -> Vec<u8> {
    let path = dir.join(name);
    let img = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).to_rgba8();
    assert_eq!((img.width(), img.height()), (size, size), "{name}: ждали {size}×{size}");
    img.into_raw()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon");
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("assets/icon");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    std::fs::write(out.join("window.rgba"), rgba(&dir, "window-64.png", 64)).expect("write window.rgba");
    std::fs::write(out.join("tray.rgba"), rgba(&dir, "icon-32.png", 32)).expect("write tray.rgba");
    std::fs::copy(dir.join("icon-128.png"), out.join("anvil.png")).expect("copy anvil.png");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = out.join("anvil.ico");
    let frames: Vec<IcoFrame> = ICO_SIZES
        .iter()
        .map(|&size| {
            let raw = rgba(&dir, &format!("icon-{size}.png"), size);
            IcoFrame::as_png(&raw, size, size, ExtendedColorType::Rgba8).expect("icon frame")
        })
        .collect();
    IcoEncoder::new(std::fs::File::create(&ico).expect("create icon")).encode_images(&frames).expect("write icon");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().expect("utf-8 path"));
    res.set("ProductName", "Anvil");
    res.set("FileDescription", "Anvil — launcher and forge for my programs");
    if let Err(e) = res.compile() {
        // Без rc.exe программа соберётся, просто без значка у exe.
        println!("cargo:warning=exe icon skipped: {e}");
    }
}
