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

/// If `path` is a JPEG with a non-identity EXIF orientation, write an upright,
/// EXIF-free copy under `out_dir` and return its path. Receivers (including
/// official SimpleX clients) do not all honour EXIF, so the rotation is baked in.
pub fn upright_copy_for_send(path: &Path, out_dir: &Path) -> Result<Option<PathBuf>, String> {
    if exif_orientation(path).is_none_or(|o| o == Orientation::NoTransforms) {
        return Ok(None);
    }
    let img = open_upright(path)?;
    let img = if img.width().max(img.height()) > OUTGOING_IMAGE_MAX_SIDE {
        img.resize(OUTGOING_IMAGE_MAX_SIDE, OUTGOING_IMAGE_MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("image.jpg");
    let out = out_dir.join(name);
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut w = std::io::BufWriter::new(file);
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 88);
    image::DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(enc).map_err(|e| format!("jpeg encode: {e}"))?;
    Ok(Some(out))
}

/// Resize an image file into a SimpleX profile data-URL (≤ [`MAX_PROFILE_IMAGE_CHARS`]).
pub fn encode_profile_image(path: &Path) -> Result<String, String> {
    let img = open_upright(path).map_err(|e| format!("open profile image: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    // Shrink until the JPEG data-URL fits the official char budget.
    let mut max_side = 256u32;
    loop {
        let scale = (max_side as f32 / w as f32).min(max_side as f32 / h as f32).min(1.0);
        let rw = ((w as f32) * scale).round().max(1.0) as u32;
        let rh = ((h as f32) * scale).round().max(1.0) as u32;
        let resized = image::imageops::resize(&rgba, rw, rh, image::imageops::FilterType::Triangle);
        for quality in [80u8, 60, 40, 25] {
            let mut bytes = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut bytes), quality);
            image::DynamicImage::ImageRgba8(resized.clone())
                .to_rgb8()
                .write_with_encoder(enc)
                .map_err(|e| format!("jpeg encode: {e}"))?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let data_url = format!("data:image/jpg;base64,{b64}");
            if data_url.len() <= MAX_PROFILE_IMAGE_CHARS {
                return Ok(data_url);
            }
        }
        if max_side <= 48 {
            return Err("image too large for profile (could not fit 12.5KB data-URL)".into());
        }
        max_side = (max_side * 3 / 4).max(48);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
