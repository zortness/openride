use openride_author::imagery;
fn png(size: u32, transparent: bool) -> Vec<u8> {
    let mut raw = vec![];
    {
        let mut encoder = png::Encoder::new(&mut raw, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![
                if transparent { 0 } else { 255 };
                (size * size * 4) as usize
            ])
            .unwrap();
    }
    raw
}
#[test]
fn decodes_complete_images_and_rejects_missing_corrupt_or_unbounded_content() {
    let image = png(1024, false);
    assert_eq!(imagery::validate(&image).unwrap(), 0);
    assert!(imagery::validate(&image[..image.len() - 10]).is_err());
    let mut corrupt = image.clone();
    let n = corrupt.len();
    corrupt[n / 2] ^= 0xff;
    assert!(imagery::validate(&corrupt).is_err());
    assert!(imagery::validate(&png(32, false)).is_err());
    assert_eq!(
        imagery::validate(&png(1024, true)).unwrap_err().code,
        "missing_imagery"
    );
}
