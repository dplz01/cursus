//! Lightweight media helpers (ffmpeg CLI — no heavy player crates).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;

pub const WAVEFORM_BARS: usize = 96;
/// Official SimpleX `maxProfileImageSize` (characters of the data-URL).
pub const MAX_PROFILE_IMAGE_CHARS: usize = 12_500;
/// Official bio cap (`MAX_BIO_LENGTH_BYTES`).
pub const MAX_BIO_BYTES: usize = 160;

pub struct WaveformInfo {
    pub peaks: Vec<f32>,
    /// Decoded length at 8 kHz mono (seconds).
    pub duration_secs: f32,
}

/// Downsample mono float PCM into normalized peak bars for chat waveforms.
pub fn extract_waveform_peaks(path: &Path, bars: usize) -> Result<Vec<f32>, String> {
    Ok(analyze_audio(path, bars)?.peaks)
}

pub fn analyze_audio(path: &Path, bars: usize) -> Result<WaveformInfo, String> {
    let bars = bars.max(8);
    let raw = path.with_extension("peaks.f32");
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
        ])
        .arg(path)
        .args(["-ac", "1", "-ar", "8000", "-f", "f32le"])
        .arg(&raw)
        .status()
        .map_err(|e| format!("ffmpeg waveform: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&raw);
        return Err("ffmpeg could not decode audio for waveform".into());
    }
    let bytes = std::fs::read(&raw).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&raw);
    if bytes.len() < 4 {
        return Err("empty audio".into());
    }
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let duration_secs = samples.len() as f32 / 8000.0;
    let chunk = (samples.len() / bars).max(1);
    let mut peaks = Vec::with_capacity(bars);
    for i in 0..bars {
        let start = i * chunk;
        let end = (start + chunk).min(samples.len());
        let peak = samples[start..end]
            .iter()
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        peaks.push(peak);
    }
    let max = peaks.iter().copied().fold(0.0_f32, f32::max).max(1e-6);
    Ok(WaveformInfo {
        peaks: peaks.into_iter().map(|p| (p / max).clamp(0.05, 1.0)).collect(),
        duration_secs,
    })
}

/// Duration of a captured voice file. Prefers WAV PCM size, then ffprobe.
pub fn audio_file_duration_secs(path: &Path) -> Option<f32> {
    wav_pcm_duration_secs(path).or_else(|| ffprobe_duration_secs(path))
}

fn wav_pcm_duration_secs(path: &Path) -> Option<f32> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let channels = u16::from_le_bytes(bytes[22..24].try_into().ok()?) as f32;
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().ok()?) as f32;
    let bits = u16::from_le_bytes(bytes[34..36].try_into().ok()?) as f32;
    if sample_rate <= 0.0 || channels <= 0.0 || bits <= 0.0 {
        return None;
    }
    let mut i = 12usize;
    let mut data_len = None;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let sz = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        if id == b"data" {
            data_len = Some(sz.min(bytes.len().saturating_sub(i + 8)));
            break;
        }
        i = i.saturating_add(8).saturating_add(sz);
        if sz % 2 == 1 {
            i = i.saturating_add(1);
        }
    }
    let data_len = data_len.unwrap_or(bytes.len().saturating_sub(44)) as f32;
    let bytes_per_sec = sample_rate * channels * (bits / 8.0);
    if bytes_per_sec <= 0.0 {
        return None;
    }
    Some(data_len / bytes_per_sec)
}

fn ffprobe_duration_secs(path: &Path) -> Option<f32> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Prefer decoded/file length; fall back to wall-clock capture time.
pub fn voice_duration_secs(
    path: &Path,
    elapsed: std::time::Duration,
    decoded_secs: Option<f32>,
) -> u32 {
    let from_file = decoded_secs
        .filter(|d| *d >= 0.45)
        .or_else(|| audio_file_duration_secs(path).filter(|d| *d >= 0.45));
    let secs = from_file.unwrap_or(elapsed.as_secs_f32());
    secs.round().clamp(1.0, 300.0) as u32
}

/// Poster frame for a video (PNG next to the file, reused if fresh).
///
/// Writes PNG — ffmpeg 7+ / 9 MJPEG JPEG encode often fails with
/// "Non full-range YUV is non-standard" unless `-strict unofficial`.
pub fn video_thumbnail(path: &Path) -> Result<PathBuf, String> {
    if let Some(hit) = cached_video_thumbnail(path) {
        return Ok(hit);
    }
    let thumb = thumb_path_png(path);
    if let Some(parent) = thumb.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Prefer no input-seek first — short clips break `-ss` before `-i`.
    // PNG avoids ffmpeg's broken default MJPEG path on recent builds.
    let attempts: &[&[&str]] = &[
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            "__IN__",
            "-frames:v",
            "1",
            "-vf",
            "scale=480:-2,format=rgb24",
            "__OUT__",
        ],
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            "__IN__",
            "-ss",
            "0.3",
            "-frames:v",
            "1",
            "-vf",
            "scale=720:-2,format=rgb24",
            "__OUT__",
        ],
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            "__IN__",
            "-vf",
            "thumbnail,scale=480:-2,format=rgb24",
            "-frames:v",
            "1",
            "__OUT__",
        ],
        // Last resort: JPEG with unofficial MJPEG (ffmpeg 9+).
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            "__IN__",
            "-frames:v",
            "1",
            "-q:v",
            "5",
            "-vf",
            "scale=480:-2",
            "-strict",
            "unofficial",
            "__JPG__",
        ],
    ];
    let jpg_fallback = thumb_path_jpg(path);
    let mut last = String::from("ffmpeg thumb failed");
    for recipe in attempts {
        let out = if recipe.contains(&"__JPG__") {
            &jpg_fallback
        } else {
            &thumb
        };
        let args: Vec<&str> = recipe
            .iter()
            .map(|s| match *s {
                "__IN__" => path.to_str().unwrap_or(""),
                "__OUT__" => thumb.to_str().unwrap_or(""),
                "__JPG__" => jpg_fallback.to_str().unwrap_or(""),
                other => other,
            })
            .collect();
        if args.iter().any(|s| s.is_empty()) {
            continue;
        }
        match Command::new("ffmpeg").args(&args).status() {
            Ok(status) if status.success() && out.is_file() => {
                if std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false) {
                    return Ok(out.clone());
                }
                last = "ffmpeg wrote empty thumb".into();
            }
            Ok(status) => last = format!("ffmpeg thumb exited {status}"),
            Err(e) => last = format!("ffmpeg thumb: {e}"),
        }
        let _ = std::fs::remove_file(out);
    }
    Err(last)
}

fn thumb_path_png(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    path.with_file_name(format!("{stem}.thumb.png"))
}

fn thumb_path_jpg(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    path.with_file_name(format!("{stem}.thumb.jpg"))
}

fn thumb_fresh(path: &Path, thumb: &Path) -> bool {
    let Ok(src) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(dst) = std::fs::metadata(thumb) else {
        return false;
    };
    dst.len() > 0 && dst.modified().ok() >= src.modified().ok()
}

/// Return a cached video poster if one already exists (never runs ffmpeg).
pub fn cached_video_thumbnail(path: &Path) -> Option<PathBuf> {
    let png = thumb_path_png(path);
    if thumb_fresh(path, &png) {
        return Some(png);
    }
    let jpg = thumb_path_jpg(path);
    if thumb_fresh(path, &jpg) {
        return Some(jpg);
    }
    None
}

pub fn is_video_name(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "mkv" | "webm" | "m4v" | "avi" | "3gp" | "3g2" | "mpeg" | "mpg" | "ts")
    )
}

pub fn is_image_name(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp")
    )
}

/// Scale `(sw, sh)` to fit inside `max_w` × `max_h` without changing aspect ratio.
pub fn fit_within(sw: u32, sh: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let max_w = max_w.max(1);
    let max_h = max_h.max(1);
    let sw = sw.max(1) as f32;
    let sh = sh.max(1) as f32;
    let scale = (max_w as f32 / sw).min(max_h as f32 / sh);
    let w = (sw * scale).round().clamp(1.0, max_w as f32) as u32;
    let h = (sh * scale).round().clamp(1.0, max_h as f32) as u32;
    (w.max(1), h.max(1))
}

/// Return a cached rounded preview if one already exists (never decodes).
pub fn cached_rounded_fitted_preview(
    path: &Path,
    max_w: u32,
    max_h: u32,
) -> Option<(PathBuf, u32, u32)> {
    let (sw, sh) = image::image_dimensions(path).ok()?;
    if sw == 0 || sh == 0 {
        return None;
    }
    let (width, height) = fit_within(sw, sh, max_w.max(32), max_h.max(32));
    let out = rounded_preview_path(path, width, height);
    if !out.is_file() {
        return None;
    }
    let src = std::fs::metadata(path).ok()?;
    let dst = std::fs::metadata(&out).ok()?;
    if dst.len() == 0 || dst.modified().ok()? < src.modified().ok()? {
        return None;
    }
    Some((out, width, height))
}

/// Aspect-preserving resize + rounded-corner PNG (iced can't clip to radius).
///
/// Returns `(cached_path, display_width, display_height)`.
/// Header-only dimension probe first so cached previews skip a full decode.
pub fn rounded_fitted_preview(
    path: &Path,
    max_w: u32,
    max_h: u32,
    radius: f32,
) -> Result<(PathBuf, u32, u32), String> {
    if let Some(hit) = cached_rounded_fitted_preview(path, max_w, max_h) {
        return Ok(hit);
    }
    let (sw, sh) = image::image_dimensions(path).map_err(|e| format!("preview size: {e}"))?;
    if sw == 0 || sh == 0 {
        return Err("empty image".into());
    }
    let (width, height) = fit_within(sw, sh, max_w.max(32), max_h.max(32));
    let out = rounded_preview_path(path, width, height);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let img = image::open(path).map_err(|e| format!("decode preview: {e}"))?;
    let rgba = img.to_rgba8();
    let mut resized =
        image::imageops::resize(&rgba, width, height, image::imageops::FilterType::Triangle);
    let r = radius
        .max(0.0)
        .min(width as f32 / 2.0)
        .min(height as f32 / 2.0);
    for y in 0..height {
        for x in 0..width {
            let cover =
                rounded_rect_coverage(x as f32 + 0.5, y as f32 + 0.5, width as f32, height as f32, r);
            if cover >= 0.999 {
                continue;
            }
            let px = resized.get_pixel_mut(x, y);
            px.0[3] = ((px.0[3] as f32) * cover).round() as u8;
        }
    }
    resized
        .save(&out)
        .map_err(|e| format!("write rounded preview: {e}"))?;
    Ok((out, width, height))
}

fn rounded_preview_path(path: &Path, width: u32, height: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    path.with_file_name(format!("{stem}.round{width}x{height}.png"))
}

fn rounded_rect_coverage(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    if r <= 0.0 {
        return 1.0;
    }
    let in_corner = (x < r || x > w - r) && (y < r || y > h - r);
    if !in_corner {
        return 1.0;
    }
    let cx = if x < r { r } else { w - r };
    let cy = if y < r { r } else { h - r };
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    let aa = 1.0;
    if dist <= r - aa {
        1.0
    } else if dist >= r + aa {
        0.0
    } else {
        ((r + aa - dist) / (2.0 * aa)).clamp(0.0, 1.0)
    }
}

/// Resize an image file into a SimpleX profile data-URL (≤ [`MAX_PROFILE_IMAGE_CHARS`]).
pub fn encode_profile_image(path: &Path) -> Result<String, String> {
    let img = image::open(path).map_err(|e| format!("open profile image: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    // Shrink until JPEG data-URL fits the official char budget.
    let mut max_side = 256u32;
    loop {
        let scale = (max_side as f32 / w as f32).min(max_side as f32 / h as f32).min(1.0);
        let rw = ((w as f32) * scale).round().max(1.0) as u32;
        let rh = ((h as f32) * scale).round().max(1.0) as u32;
        let resized = image::imageops::resize(&rgba, rw, rh, image::imageops::FilterType::Triangle);
        for quality in [80u8, 60, 40, 25] {
            let mut bytes = Vec::new();
            let mut cursor = Cursor::new(&mut bytes);
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
            image::DynamicImage::ImageRgba8(resized.clone())
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

/// Decode a profile data-URL into raw image bytes (png/jpeg) for iced.
pub fn decode_profile_image_bytes(data_url: &str) -> Result<Vec<u8>, String> {
    let s = data_url.trim();
    let (_, b64) = s
        .split_once("base64,")
        .ok_or_else(|| "profile image is not a data-URL".to_string())?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 decode: {e}"))
}

/// Decode a profile data-URL into circular RGBA pixels `(width, height, raw RGBA)`.
pub fn round_profile_avatar_rgba(
    data_url: &str,
    size_px: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let size = size_px.max(16);
    let raw = decode_profile_image_bytes(data_url)?;
    let img = image::load_from_memory(&raw).map_err(|e| format!("decode profile image: {e}"))?;
    let rgba = img.to_rgba8();
    let (sw, sh) = rgba.dimensions();
    if sw == 0 || sh == 0 {
        return Err("empty profile image".into());
    }
    let scale = (size as f32 / sw as f32).max(size as f32 / sh as f32);
    let rw = ((sw as f32) * scale).round().max(1.0) as u32;
    let rh = ((sh as f32) * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&rgba, rw, rh, image::imageops::FilterType::Triangle);
    let x0 = rw.saturating_sub(size) / 2;
    let y0 = rh.saturating_sub(size) / 2;
    let mut cropped = image::imageops::crop_imm(&resized, x0, y0, size, size).to_image();
    let r = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let cover = rounded_rect_coverage(x as f32 + 0.5, y as f32 + 0.5, size as f32, size as f32, r);
            if cover >= 0.999 {
                continue;
            }
            let px = cropped.get_pixel_mut(x, y);
            px.0[3] = ((px.0[3] as f32) * cover).round() as u8;
        }
    }
    Ok((size, size, cropped.into_raw()))
}

/// Truncate a bio to at most [`MAX_BIO_BYTES`] UTF-8 bytes (official client rule).
pub fn truncate_bio_bytes(s: &str) -> String {
    if s.len() <= MAX_BIO_BYTES {
        return s.to_string();
    }
    let mut end = MAX_BIO_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_within_preserves_landscape() {
        let (w, h) = fit_within(1920, 1080, 300, 400);
        assert_eq!(w, 300);
        assert_eq!(h, 169);
        assert!((w as f32 / h as f32 - 1920.0 / 1080.0).abs() < 0.02);
    }

    #[test]
    fn fit_within_preserves_portrait() {
        let (w, h) = fit_within(1080, 1920, 300, 400);
        assert_eq!(h, 400);
        assert_eq!(w, 225);
        assert!((w as f32 / h as f32 - 1080.0 / 1920.0).abs() < 0.02);
    }

    #[test]
    fn fit_within_square() {
        let (w, h) = fit_within(800, 800, 300, 400);
        assert_eq!(w, 300);
        assert_eq!(h, 300);
    }

    #[test]
    fn viewer_preview_path_is_stable() {
        let p = PathBuf::from("/tmp/photo.JPG");
        let a = viewer_preview_path(&p, 1600);
        let b = viewer_preview_path(&p, 1600);
        assert_eq!(a, b);
        assert!(a
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.contains("view1600")));
    }

    fn write_pcm_wav(path: &Path, samples: usize, sample_rate: u32) {
        let data_len = (samples * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(44 + data_len as usize, 0);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn wav_duration_from_pcm_size() {
        let dir = std::env::temp_dir().join(format!(
            "cursus-wav-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("voice.wav");
        write_pcm_wav(&path, 16_000 * 7, 16_000);
        let d = wav_pcm_duration_secs(&path).unwrap();
        assert!((d - 7.0).abs() < 0.01);
        // Header-only / truncated files must not override a longer capture clock.
        write_pcm_wav(&path, 200, 16_000);
        assert_eq!(
            voice_duration_secs(&path, std::time::Duration::from_secs(8), None),
            8
        );
        write_pcm_wav(&path, 16_000 * 5, 16_000);
        assert_eq!(
            voice_duration_secs(&path, std::time::Duration::from_millis(900), None),
            5
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
