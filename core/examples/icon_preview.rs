//! Renders the floating icon over a background image, to check the glass by eye.
//! usage: cargo run --release --example icon_preview -- <background.png> <out.png>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bg = image::open(&args[1]).expect("background").to_rgba8();
    let mut out = bg.clone();
    // Small, medium and large at 175% display scale, plus a 256 px close-up.
    let mut x = 20;
    for size in [56u32, 84, 168, 256] {
        let y = bg.height() - size - 20;
        let patch = image::imageops::crop_imm(&bg, x, y, size, size).to_image();
        let icon = shiftdrop_core::icon::glass(&patch);
        image::imageops::overlay(&mut out, &icon, x as i64, y as i64);
        x += size + 24;
    }
    let strip = image::imageops::crop_imm(&out, 0, bg.height().saturating_sub(300), x, 300).to_image();
    strip.save(&args[2]).expect("save");
}
