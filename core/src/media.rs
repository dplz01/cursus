//! Image rules every app shares: profile pictures (SimpleX's data-URL
//! budget), upright photos for sending, decode limits.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use image::metadata::Orientation;
use image::ImageDecoder;

/// Official clients reject profile images longer than this (data-URL chars).
pub const MAX_PROFILE_IMAGE_CHARS: usize = 12_500;

/// Longest side of a re-encoded outgoing photo (official clients use ~1600px).
pub const OUTGOING_IMAGE_MAX_SIDE: u32 = 1920;

/// EXIF orientation of an image file, when it has one.
pub fn exif_orientation(path: &Path) -> Option<Orientation> {
    let mut decoder = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    decoder.orientation().ok()
}

/// Decode budget for photos. A tiny file can claim gigantic dimensions
/// ("decompression bomb"); 128 MiB covers ~40 MP RGB photos.
pub fn photo_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits
}

/// Decode with [`photo_limits`] and apply the EXIF orientation.
pub fn open_upright(path: &Path) -> Result<image::DynamicImage, String> {
    let mut reader = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| format!("open image: {e}"))?;
    reader.limits(photo_limits());
    let mut decoder = reader.into_decoder().map_err(|e| format!("decode image: {e}"))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|e| format!("decode image: {e}"))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Official clients compress photos to this size (`MAX_IMAGE_SIZE`), which
/// is also what the other side downloads without a tap.
pub const MAX_OUTGOING_IMAGE_BYTES: usize = 261_120;

/// Longest inline preview (data-URL chars) of an image message.
pub const MAX_PREVIEW_CHARS: usize = 14_000;

fn data_url_len(bytes: usize) -> usize {
    "data:image/jpg;base64,".len() + bytes.div_ceil(3) * 4
}

fn data_url(bytes: &[u8], png: bool) -> String {
    let kind = if png { "png" } else { "jpg" };
    format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// PNG for images with transparency, else JPEG at `quality`.
fn encode(img: &image::DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    if img.color().has_alpha() {
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
    } else {
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
        image::DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(enc)
    }
    .map_err(|e| format!("encode image: {e}"))?;
    Ok(bytes)
}

/// `img` encoded so that `fits`: no larger than `max_side`, then at lower
/// quality and three quarters the size each round, down to `min_side`.
fn shrink_to_fit(
    img: &image::DynamicImage,
    max_side: u32,
    min_side: u32,
    fits: impl Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, String> {
    let qualities: &[u8] = if img.color().has_alpha() { &[0] } else { &[85, 70, 55, 40, 25] };
    let mut side = max_side;
    loop {
        let scaled = if img.width().max(img.height()) > side {
            std::borrow::Cow::Owned(img.resize(side, side, image::imageops::FilterType::Triangle))
        } else {
            std::borrow::Cow::Borrowed(img)
        };
        for &quality in qualities {
            let bytes = encode(&scaled, quality)?;
            if fits(&bytes) {
                return Ok(bytes);
            }
        }
        if side <= min_side {
            return Err("image can't be made small enough".into());
        }
        side = (side * 3 / 4).max(min_side);
    }
}

/// A photo ready to send.
pub struct OutgoingImage {
    /// The file to send: a re-encoded copy, or the original for GIFs.
    pub path: PathBuf,
    /// Small inline preview (data-URL) shown before the file is downloaded.
    pub preview: String,
}

/// Prepare a photo the way the official clients send it: upright, at most
/// [`OUTGOING_IMAGE_MAX_SIDE`] and [`MAX_OUTGOING_IMAGE_BYTES`], and without
/// its metadata (EXIF can hold where and when it was taken), plus the inline
/// preview. GIFs keep their file (they may be animated). The copy goes under
/// `out_dir`; fails when the file can't be decoded as an image.
pub fn prepare_outgoing_image(path: &Path, out_dir: &Path) -> Result<OutgoingImage, String> {
    let img = open_upright(path)?;
    let png = img.color().has_alpha();
    let preview = shrink_to_fit(&img, 256, 32, |b| data_url_len(b.len()) <= MAX_PREVIEW_CHARS)?;
    let preview = data_url(&preview, png);
    let is_gif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    if is_gif {
        return Ok(OutgoingImage { path: path.to_path_buf(), preview });
    }
    let bytes = shrink_to_fit(&img, OUTGOING_IMAGE_MAX_SIDE, 256, |b| b.len() <= MAX_OUTGOING_IMAGE_BYTES)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let out = out_dir.join(format!("{stem}.{}", if png { "png" } else { "jpg" }));
    std::fs::create_dir_all(out_dir).and_then(|()| std::fs::write(&out, bytes)).map_err(|e| e.to_string())?;
    Ok(OutgoingImage { path: out, preview })
}

/// Resize an image file into a SimpleX profile data-URL (≤ [`MAX_PROFILE_IMAGE_CHARS`]).
pub fn encode_profile_image(path: &Path) -> Result<String, String> {
    let img = open_upright(path).map_err(|e| format!("open profile image: {e}"))?;
    let bytes = shrink_to_fit(&img, 256, 48, |b| data_url_len(b.len()) <= MAX_PROFILE_IMAGE_CHARS)
        .map_err(|_| "image too large for profile (could not fit 12.5KB data-URL)".to_string())?;
    Ok(data_url(&bytes, img.color().has_alpha()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outgoing_photo_is_small_upright_and_has_a_preview() {
        let dir = std::env::temp_dir().join(format!("cursus-outgoing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.png");
        let img = image::RgbImage::from_fn(3000, 2000, |x, y| {
            let v = ((x * 7919 + y * 104_729) % 251) as u8;
            image::Rgb([v, v.wrapping_mul(3), v.wrapping_mul(7)])
        });
        img.save(&path).unwrap();
        let out = prepare_outgoing_image(&path, &dir.join("out")).unwrap();
        assert_eq!(out.path, dir.join("out/big.jpg"));
        assert!(std::fs::metadata(&out.path).unwrap().len() as usize <= MAX_OUTGOING_IMAGE_BYTES);
        let sent = image::open(&out.path).unwrap();
        assert!(sent.width() <= OUTGOING_IMAGE_MAX_SIDE && sent.width() > sent.height());
        assert!(out.preview.starts_with("data:image/jpg;base64,") && out.preview.len() <= MAX_PREVIEW_CHARS);
        assert!(prepare_outgoing_image(&dir.join("missing.jpg"), &dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn transparent_images_stay_png() {
        let dir = std::env::temp_dir().join(format!("cursus-alpha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sticker.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([9, 9, 9, 0])).save(&path).unwrap();
        let out = prepare_outgoing_image(&path, &dir.join("out")).unwrap();
        assert!(out.path.ends_with("sticker.png") && out.preview.starts_with("data:image/png;base64,"));
        assert!(image::open(&out.path).unwrap().color().has_alpha());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_image_fits_the_official_budget() {
        let dir = std::env::temp_dir().join(format!("cursus-avatar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.png");
        // Noisy 1200×900: compresses badly, so the shrink loop has to work.
        let img = image::RgbImage::from_fn(1200, 900, |x, y| {
            let v = ((x * 7919 + y * 104_729) % 251) as u8;
            image::Rgb([v, v.wrapping_mul(3), v.wrapping_mul(7)])
        });
        img.save(&path).unwrap();
        let url = encode_profile_image(&path).unwrap();
        assert!(url.starts_with("data:image/jpg;base64,"));
        assert!(url.len() <= MAX_PROFILE_IMAGE_CHARS);
        let _ = std::fs::remove_dir_all(dir);
    }
}
