//! Bounded optional imagery validation. No image interpreter is required.
use crate::{error, Result};
use std::io::Cursor;
pub const SIZE: u32 = 1024;
pub const SERVICE: &str =
    "https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPPlus/ImageServer";
pub fn validate(raw: &[u8]) -> Result<usize> {
    let pixels = decode(raw)?;
    Ok(pixels.chunks_exact(4).filter(|p| p[3] == 0).count())
}
pub fn decode(raw: &[u8]) -> Result<Vec<u8>> {
    decode_size(raw, SIZE)
}
pub fn validate_tile(raw: &[u8]) -> Result<()> {
    decode_size(raw, 130).map(|_| ())
}
fn decode_size(raw: &[u8], size: u32) -> Result<Vec<u8>> {
    let bad = |e: String| error("invalid_imagery", e);
    if raw.len() > 16 * 1024 * 1024 {
        return Err(bad("Imagery exceeds 16 MiB".into()));
    }
    let mut decoder = png::Decoder::new(Cursor::new(raw));
    decoder.set_limits(png::Limits {
        bytes: 8 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| bad(e.to_string()))?;
    if reader.info().width != size
        || reader.info().height != size
        || reader.info().bit_depth != png::BitDepth::Eight
        || reader.info().animation_control.is_some()
    {
        return Err(bad(format!("Expected a single {size} x {size} 8-bit PNG")));
    }
    let length = reader
        .output_buffer_size()
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or_else(|| bad("Unbounded image dimensions".into()))?;
    let mut output = vec![0; length];
    let info = reader
        .next_frame(&mut output)
        .map_err(|e| bad(e.to_string()))?;
    reader.finish().map_err(|e| bad(e.to_string()))?;
    let transparent = match info.color_type {
        png::ColorType::Rgba => output[..info.buffer_size()]
            .chunks_exact(4)
            .filter(|p| p[3] == 0)
            .count(),
        png::ColorType::GrayscaleAlpha => output[..info.buffer_size()]
            .chunks_exact(2)
            .filter(|p| p[1] == 0)
            .count(),
        _ => 0,
    };
    if transparent == size as usize * size as usize {
        return Err(error(
            "missing_imagery",
            "Provider returned an entirely transparent image; coverage is unavailable",
        ));
    }
    let pixels = output[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .flat_map(|p| match info.color_type {
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            _ => unreachable!("palette expanded by decoder"),
        })
        .collect();
    Ok(pixels)
}
