//! Converts between PNG and the device-independent bitmaps (DIBs) that Windows
//! puts on the clipboard, for apps that don't offer or read the "PNG" format.
//! Plain byte handling, so it's tested on every platform.
use anyhow::{Context, Result, ensure};
use image::{ImageFormat, RgbaImage};
use std::io::Cursor;

const FILE_HEADER_LEN: usize = 14;
const INFO_HEADER_LEN: usize = 40;
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// `dib` is the contents of the CF_DIB clipboard format: a bitmap header,
/// optional color masks or palette, then pixels.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>> {
    ensure!(dib.len() >= INFO_HEADER_LEN, "bitmap is too short");
    let header_len = u32_at(dib, 0) as usize;
    let bit_count = u16_at(dib, 14);
    let compression = u32_at(dib, 16);
    let colors_used = u32_at(dib, 32) as usize;
    // Masks follow a plain 40-byte header; larger headers include them.
    let masks = if header_len == INFO_HEADER_LEN && compression == BI_BITFIELDS {
        12
    } else {
        0
    };
    let palette = match (bit_count, colors_used) {
        (1..=8, 0) => (1 << bit_count) * 4,
        (_, used) => used * 4,
    };
    let pixels_at = FILE_HEADER_LEN + header_len + masks + palette;

    // A DIB is a .bmp file without its 14-byte file header.
    let mut file = Vec::with_capacity(FILE_HEADER_LEN + dib.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((FILE_HEADER_LEN + dib.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&(pixels_at as u32).to_le_bytes());
    file.extend_from_slice(dib);
    let mut image = image::load_from_memory_with_format(&file, ImageFormat::Bmp)
        .context("unreadable bitmap on the clipboard")?
        .to_rgba8();
    // Screenshots are 32-bit with an alpha byte of zero that means nothing.
    if bit_count == 32 && compression == BI_RGB && image.pixels().all(|p| p[3] == 0) {
        image.pixels_mut().for_each(|p| p[3] = 255);
    }
    encode_png(&image)
}

/// Builds a 32-bit bottom-up DIB. Transparent areas are drawn on white, since
/// apps that only read DIBs ignore alpha.
pub fn png_to_dib(png: &[u8]) -> Result<Vec<u8>> {
    let image = image::load_from_memory_with_format(png, ImageFormat::Png)
        .context("not a valid PNG")?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let size = width as usize * height as usize * 4;
    let mut dib = Vec::with_capacity(INFO_HEADER_LEN + size);
    dib.extend_from_slice(&(INFO_HEADER_LEN as u32).to_le_bytes());
    dib.extend_from_slice(&(width as i32).to_le_bytes());
    dib.extend_from_slice(&(height as i32).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&BI_RGB.to_le_bytes());
    dib.extend_from_slice(&(size as u32).to_le_bytes());
    dib.extend_from_slice(&2835u32.to_le_bytes()); // 72 dpi
    dib.extend_from_slice(&2835u32.to_le_bytes());
    dib.extend_from_slice(&[0; 8]);
    for row in (0..height).rev() {
        for x in 0..width {
            let [r, g, b, a] = image.get_pixel(x, row).0;
            let over_white = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
            dib.extend_from_slice(&[over_white(b), over_white(g), over_white(r), 255]);
        }
    }
    Ok(dib)
}

/// Windows clipboard memory can be longer than the data, so this cuts a PNG
/// at the end of its last chunk.
pub fn trim_png(bytes: &[u8]) -> &[u8] {
    match bytes.windows(4).rposition(|w| w == b"IEND") {
        Some(at) if at + 8 <= bytes.len() => &bytes[..at + 8],
        _ => bytes,
    }
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sample() -> RgbaImage {
        RgbaImage::from_fn(5, 3, |x, y| Rgba([x as u8 * 50, y as u8 * 80, 200, 255]))
    }

    #[test]
    fn png_survives_a_trip_through_a_dib() {
        let png = encode_png(&sample()).unwrap();
        let back = dib_to_png(&png_to_dib(&png).unwrap()).unwrap();
        let decoded = image::load_from_memory(&back).unwrap().to_rgba8();
        assert_eq!(decoded, sample());
    }

    #[test]
    fn screenshot_alpha_of_zero_becomes_opaque() {
        let png = encode_png(&sample()).unwrap();
        let mut dib = png_to_dib(&png).unwrap();
        for pixel in dib[INFO_HEADER_LEN..].chunks_mut(4) {
            pixel[3] = 0;
        }
        let decoded = image::load_from_memory(&dib_to_png(&dib).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded, sample());
    }

    #[test]
    fn transparency_is_drawn_on_white_for_dibs() {
        let clear = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0]));
        let dib = png_to_dib(&encode_png(&clear).unwrap()).unwrap();
        assert_eq!(&dib[INFO_HEADER_LEN..], &[255, 255, 255, 255]);
    }

    #[test]
    fn trailing_bytes_after_a_png_are_trimmed() {
        let png = encode_png(&sample()).unwrap();
        let mut padded = png.clone();
        padded.extend_from_slice(&[0; 13]);
        assert_eq!(trim_png(&padded), png.as_slice());
        assert_eq!(trim_png(b"not a png"), b"not a png");
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        assert!(dib_to_png(&[0; 10]).is_err());
        assert!(dib_to_png(&[0; 64]).is_err());
        assert!(png_to_dib(b"nope").is_err());
    }
}
