//! Lightweight media helpers (ffmpeg CLI; symphonia for waveform decode).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use image::metadata::Orientation;

pub const WAVEFORM_BARS: usize = 96;
/// Official SimpleX `maxProfileImageSize` (characters of the data-URL).
pub const MAX_PROFILE_IMAGE_CHARS: usize = 12_500;
/// Official bio cap (`MAX_BIO_LENGTH_BYTES`).
pub const MAX_BIO_BYTES: usize = 160;

pub struct WaveformInfo {
    pub peaks: Vec<f32>,
    /// Decoded length at 8 kHz mono (seconds).
    pub duration_secs: f32,
    /// Loudest absolute sample before normalisation (0.0–1.0); ~0 means silence.
    pub raw_peak: f32,
}

/// Downsample mono float PCM into normalized peak bars for chat waveforms.
pub fn extract_waveform_peaks(path: &Path, bars: usize) -> Result<Vec<f32>, String> {
    Ok(analyze_audio(path, bars)?.peaks)
}

pub fn analyze_audio(path: &Path, bars: usize) -> Result<WaveformInfo, String> {
    let bars = bars.max(8);
    // Decode in-process first: phones (musl postmarketOS) often have no
    // working ffmpeg, which left every voice note on the flat placeholder.
    let samples = match decode_mono_8k(path) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return Err("empty audio".into()),
        Err(native_err) => decode_mono_8k_ffmpeg(path)
            .map_err(|e| format!("waveform decode failed ({native_err}; {e})"))?,
    };
    let duration_secs = samples.len() as f32 / WAVE_RATE as f32;
    Ok(WaveformInfo {
        peaks: waveform_bars(&samples, bars),
        duration_secs,
        raw_peak: samples.iter().fold(0.0_f32, |m, s| m.max(s.abs())),
    })
}

/// Sample rate used for waveform analysis.
const WAVE_RATE: u32 = 8000;

/// Pure-Rust decode (WAV/PCM, AAC/M4A, MP3, Ogg Vorbis) to mono ~8 kHz.
/// Downsampling averages blocks, which is cheap and enough for bars.
fn decode_mono_8k(path: &Path) -> Result<Vec<f32>, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_AAC, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions { enable_gapless: true, ..Default::default() },
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let track_id = track.id;
    // AAC encoders prepend ~1024 priming frames; M4A edit lists that would
    // trim them aren't applied here, so skip them to keep bars aligned.
    let mut skip = match track.codec_params.delay {
        Some(d) => d as usize,
        None if track.codec_params.codec == CODEC_TYPE_AAC => 1024,
        None => 0,
    };
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut acc = 0.0_f32;
    let mut acc_n = 0_u32;
    let mut block = 1_u32;
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => {
                if out.is_empty() {
                    return Err(e.to_string());
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        let spec = *decoded.spec();
        block = (spec.rate / WAVE_RATE).max(1);
        let channels = spec.channels.count().max(1);
        if buf
            .as_ref()
            .is_none_or(|b| b.capacity() < decoded.capacity() * channels)
        {
            buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let Some(sb) = buf.as_mut() else { break };
        sb.copy_interleaved_ref(decoded);
        for frame in sb.samples().chunks(channels) {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            acc += frame.iter().sum::<f32>() / channels as f32;
            acc_n += 1;
            if acc_n >= block {
                let v = acc / acc_n as f32;
                out.push(if v.is_finite() { v } else { 0.0 });
                acc = 0.0;
                acc_n = 0;
            }
        }
    }
    if acc_n > block / 2 {
        out.push(acc / acc_n as f32);
    }
    Ok(out)
}

/// Fallback for formats symphonia lacks (e.g. Opus): ffmpeg to stdout.
fn decode_mono_8k_ffmpeg(path: &Path) -> Result<Vec<f32>, String> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-ac", "1", "-ar", "8000", "-f", "f32le", "-"])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !output.status.success() || output.stdout.len() < 4 {
        return Err("ffmpeg could not decode audio".into());
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .map(|v| if v.is_finite() { v } else { 0.0 })
        .collect())
}

/// Loudness bars (0.05–1.0) from mono samples: per-bar RMS with the DC
/// offset removed, the noise floor (10th percentile) subtracted, and a
/// gentle curve so speech reads as peaks and pauses as dips.
pub fn waveform_bars(samples: &[f32], bars: usize) -> Vec<f32> {
    const MIN_BAR: f32 = 0.05;
    let bars = bars.max(1);
    if samples.is_empty() {
        return vec![MIN_BAR; bars];
    }
    let mean = samples.iter().map(|&s| s as f64).sum::<f64>() / samples.len() as f64;
    let mean = mean as f32;
    let rms: Vec<f32> = (0..bars)
        .map(|i| {
            let start = i * samples.len() / bars;
            let end = ((i + 1) * samples.len() / bars).max(start + 1).min(samples.len());
            let chunk = &samples[start.min(samples.len() - 1)..end];
            let sq = chunk.iter().map(|&s| (s - mean) * (s - mean)).sum::<f32>();
            (sq / chunk.len() as f32).sqrt()
        })
        .collect();
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = sorted[sorted.len() / 10];
    let max = *sorted.last().unwrap_or(&0.0);
    let span = max - floor;
    if span <= 1e-5 {
        // Flat signal (silence or constant hum): draw a low, even line.
        return vec![if max > 1e-4 { 0.2 } else { MIN_BAR }; bars];
    }
    rms.into_iter()
        .map(|r| (((r - floor) / span).clamp(0.0, 1.0)).powf(0.7).max(MIN_BAR))
        .collect()
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

/// EXIF orientation of an image file (headers only, no pixel decode). Phone
/// cameras store portrait shots as landscape pixels plus this tag.
pub fn exif_orientation(path: &Path) -> Option<Orientation> {
    let mut decoder = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    use image::ImageDecoder;
    decoder.orientation().ok()
}

/// Decode an image file with EXIF orientation applied.
pub fn open_upright(path: &Path) -> Result<image::DynamicImage, String> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| format!("open image: {e}"))?
        .into_decoder()
        .map_err(|e| format!("decode image: {e}"))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|e| format!("decode image: {e}"))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Pixel dimensions as displayed (width/height swapped for 90° EXIF tags).
pub fn upright_dimensions(path: &Path) -> Option<(u32, u32)> {
    let (w, h) = image::image_dimensions(path).ok()?;
    Some(match exif_orientation(path) {
        Some(
            Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH,
        ) => (h, w),
        _ => (w, h),
    })
}

/// Longest side of a re-encoded outgoing photo (matches official clients' ~1600px).
const OUTGOING_IMAGE_MAX_SIDE: u32 = 1920;

/// If `path` is a JPEG with a non-identity EXIF orientation, write an upright,
/// EXIF-free copy under `out_dir` and return its path. Receivers (including
/// official SimpleX clients) do not all honour EXIF, so we bake the rotation in.
pub fn upright_copy_for_send(path: &Path, out_dir: &Path) -> Result<Option<PathBuf>, String> {
    if exif_orientation(path).is_none_or(|o| o == Orientation::NoTransforms) {
        return Ok(None);
    }
    let img = open_upright(path)?;
    let img = if img.width().max(img.height()) > OUTGOING_IMAGE_MAX_SIDE {
        img.resize(
            OUTGOING_IMAGE_MAX_SIDE,
            OUTGOING_IMAGE_MAX_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("image.jpg");
    let out = out_dir.join(name);
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut w = std::io::BufWriter::new(file);
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 88);
    image::DynamicImage::ImageRgb8(img.to_rgb8())
        .write_with_encoder(enc)
        .map_err(|e| format!("jpeg encode: {e}"))?;
    Ok(Some(out))
}

/// Return a cached rounded preview if one already exists (never decodes).
pub fn cached_rounded_fitted_preview(
    path: &Path,
    max_w: u32,
    max_h: u32,
) -> Option<(PathBuf, u32, u32)> {
    let (sw, sh) = upright_dimensions(path)?;
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
    let (sw, sh) = upright_dimensions(path).ok_or_else(|| "preview size: unreadable image".to_string())?;
    if sw == 0 || sh == 0 {
        return Err("empty image".into());
    }
    let (width, height) = fit_within(sw, sh, max_w.max(32), max_h.max(32));
    let out = rounded_preview_path(path, width, height);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let img = open_upright(path)?;
    // Cheap pre-shrink: full-res camera JPEGs (12MP+) are slow to filter on ARM.
    let img = if img.width() > width * 2 || img.height() > height * 2 {
        img.thumbnail(width * 2, height * 2)
    } else {
        img
    };
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
    path.with_file_name(format!("{stem}.upright{width}x{height}.png"))
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
    let img = open_upright(path).map_err(|e| format!("open profile image: {e}"))?;
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

    /// 40×20 JPEG with an EXIF Orientation=6 (rotate 90° CW) APP1 segment.
    fn jpeg_with_orientation(orientation: u16) -> Vec<u8> {
        let img = image::RgbImage::from_fn(40, 20, |x, _| {
            if x < 20 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) }
        });
        let mut jpg = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new(&mut jpg))
            .unwrap();
        let mut tiff = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
        tiff.extend_from_slice(&[0x01, 0x12, 0x00, 0x03, 0, 0, 0, 1]);
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let mut out = jpg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpg[2..]);
        out
    }

    #[test]
    fn exif_orientation_is_applied_for_preview_and_send() {
        let dir = std::env::temp_dir().join(format!("cursus-exif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cam.jpg");
        std::fs::write(&path, jpeg_with_orientation(6)).unwrap();
        assert_eq!(exif_orientation(&path), Some(Orientation::Rotate90));
        assert_eq!(upright_dimensions(&path), Some((20, 40)));
        let up = open_upright(&path).unwrap();
        assert_eq!((up.width(), up.height()), (20, 40));
        // Rotated CW: the left (red) half ends up on top.
        let top = up.to_rgb8().get_pixel(10, 5).0;
        assert!(top[0] > 200 && top[2] < 60, "top pixel {top:?}");
        let sent = upright_copy_for_send(&path, &dir.join("out")).unwrap().unwrap();
        assert!(exif_orientation(&sent).is_none_or(|o| o == Orientation::NoTransforms));
        assert_eq!(image::image_dimensions(&sent).unwrap(), (20, 40));
        // Untagged / identity JPEGs are sent untouched.
        std::fs::write(&path, jpeg_with_orientation(1)).unwrap();
        assert!(upright_copy_for_send(&path, &dir.join("out2")).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn waveform_decodes_wav_without_ffmpeg() {
        // 48 kHz stereo s16 WAV: quiet, loud burst, quiet.
        let rate = 48_000u32;
        let mut pcm = Vec::new();
        for i in 0..rate {
            let loud = (rate / 3..2 * rate / 3).contains(&i);
            let amp = if loud { 12_000.0 } else { 150.0 };
            let v = (amp * (i as f32 * 0.07).sin()) as i16;
            pcm.extend_from_slice(&v.to_le_bytes());
            pcm.extend_from_slice(&v.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(&pcm);
        let path = std::env::temp_dir().join(format!("cursus-wave-{}.wav", std::process::id()));
        std::fs::write(&path, wav).unwrap();
        let samples = decode_mono_8k(&path).expect("native decode");
        let _ = std::fs::remove_file(&path);
        assert!((samples.len() as i64 - 8000).abs() < 50, "{}", samples.len());
        let bars = waveform_bars(&samples, 12);
        assert!(bars[0] < 0.2 && bars[11] < 0.2, "{bars:?}");
        assert!(bars[6] > 0.8, "{bars:?}");
    }

    /// Compares native decode against ffmpeg on local voice notes.
    #[test]
    #[ignore]
    fn native_matches_ffmpeg_on_local_voice_files() {
        let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join(".local/share/cursus/tmp/plain");
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if !p.to_string_lossy().contains("voice-") {
                continue;
            }
            let a = decode_mono_8k(&p).unwrap();
            let b = decode_mono_8k_ffmpeg(&p).unwrap();
            let (wa, wb) = (waveform_bars(&a, 96), waveform_bars(&b, 96));
            // AAC priming/padding can shift bars slightly; compare on average.
            let diff = wa.iter().zip(&wb).map(|(x, y)| (x - y).abs()).sum::<f32>() / 96.0;
            println!("{}: {} vs {} samples, mean bar diff {diff:.3}", p.display(), a.len(), b.len());
            assert!(diff < 0.05);
        }
    }

    #[test]
    fn waveform_ignores_dc_offset_and_noise_floor() {
        // Constant offset + hiss, with two louder "words".
        let mut samples = Vec::new();
        for i in 0..8000 {
            let hiss = ((i * 7919) % 13) as f32 / 13.0 * 0.02 - 0.01;
            let word = if (2000..3000).contains(&i) || (5000..6500).contains(&i) {
                0.4 * ((i as f32) * 0.3).sin()
            } else {
                0.0
            };
            samples.push(0.5 + hiss + word);
        }
        let bars = waveform_bars(&samples, 16);
        let full = bars.iter().filter(|b| **b > 0.9).count();
        let low = bars.iter().filter(|b| **b < 0.2).count();
        assert!((1..8).contains(&full), "only the words should peak: {bars:?}");
        assert!(low >= 6, "pauses should dip: {bars:?}");
        // Flat input → even low line, not maxed.
        assert!(waveform_bars(&vec![0.7; 800], 8).iter().all(|b| *b <= 0.2));
    }

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
