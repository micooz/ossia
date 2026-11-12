use super::*;

#[test]
fn quantized_png_preserves_palette_alpha() {
    let mut pixmap = tiny_skia::Pixmap::new(2, 1).unwrap();
    pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));
    pixmap.pixels_mut()[0] = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap();
    pixmap.pixels_mut()[1] = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
    let bytes = encode_quantized_png(&pixmap, 60).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(bytes[25], 3);
    assert!(bytes.windows(4).any(|window| window == b"tRNS"));
}
