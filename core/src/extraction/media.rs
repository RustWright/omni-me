//! Fit document bytes into what a vision endpoint will actually accept.
//!
//! See `docs/src/extraction.md` for why this sits in the extractor rather than
//! at the HTTP route.

use std::io::Cursor;
use std::path::Path;
use std::process::Stdio;

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat};
use tokio::process::Command;

/// Longest edge of any image sent onward, established by the provider POC
/// (2026-09-09): above this the request is rejected, and below it the model
/// gains no accuracy from the extra pixels.
const MAX_LONG_EDGE: u32 = 2048;

/// Ceiling on the *base64* length of all images in one request. The POC found
/// 413s above ~5 MB; base64 inflates by 4/3, so this is the number that has to
/// be checked, not the raw byte count. The margin below 5 MB leaves room for
/// the prompt, the schema and the JSON envelope, which ride in the same body.
const MAX_ENCODED_BYTES: usize = 4_500_000;

/// Re-encode quality ladder. The first entry is the normal output; the rest are
/// tried only when a multi-page raster overflows [`MAX_ENCODED_BYTES`]. Bounded
/// on purpose — a quality search that can always find room would silently ship
/// an unreadable page instead of saying the document is too big.
const JPEG_QUALITY_LADDER: [u8; 3] = [82, 65, 50];

/// Pages rasterized from one scanned PDF.
///
/// A scanned statement that overflows this is refused rather than truncated: a
/// partial extraction is a transaction list with rows silently missing, which
/// reads as a complete answer and is the one failure mode worse than an error.
const MAX_RASTER_PAGES: usize = 8;

/// Resolution for [`rasterize_pdf`]. 150 DPI puts a letter page at 1275×1650 —
/// under [`MAX_LONG_EDGE`], so single-page scans skip the resize entirely.
const RASTER_DPI: u32 = 150;

/// Largest PDF handed to poppler for rasterization. Matches the cap
/// `statement::pdf` applies to the text path; the compression-bomb argument is
/// the same and rendering is the more expensive direction.
const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;

/// Wall-clock bound on one `pdftoppm` run. Longer than the text path's because
/// rendering eight pages is real work, not a parse.
const PDFTOPPM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("decode {mime} image: {source}")]
    Decode {
        mime: String,
        source: image::ImageError,
    },
    #[error("re-encode image as JPEG: {0}")]
    Encode(image::ImageError),
    #[error("no downscaler for {mime} — add it to `format_for` before allowing it in `supports`")]
    UnpreparableMime { mime: String },
    #[error("pdf is {size} bytes, over the {MAX_PDF_BYTES}-byte limit")]
    PdfTooLarge { size: usize },
    #[error("pdftoppm could not be run ({0}) — is poppler-utils installed?")]
    Spawn(String),
    #[error("pdftoppm exited with status {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("scanned pdf has more than {MAX_RASTER_PAGES} pages — too large to send as images")]
    TooManyPages,
    #[error(
        "document is {encoded} base64 bytes after downscaling, over the \
         {MAX_ENCODED_BYTES}-byte request limit"
    )]
    TooLarge { encoded: usize },
}

/// One image ready to go into a `data:` URL, alongside the MIME that describes
/// the bytes *now* — a resized PNG comes back as `image/jpeg`, so the caller
/// must not reuse the MIME it passed in.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

impl PreparedImage {
    /// Length this image contributes to the request body once base64'd.
    fn encoded_len(&self) -> usize {
        self.bytes.len().div_ceil(3) * 4
    }

    pub fn to_data_url(&self) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{b64}", self.mime)
    }
}

fn format_for(mime: &str) -> Option<ImageFormat> {
    match mime {
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/png" => Some(ImageFormat::Png),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// Encode as JPEG at `quality`, flattening any alpha onto white first.
///
/// The flatten is not cosmetic: JPEG has no alpha channel, and `to_rgb8` alone
/// drops it — turning the transparent background of a screenshotted receipt
/// black and hiding the text the model is being asked to read.
fn encode_jpeg(img: &image::DynamicImage, quality: u8) -> Result<Vec<u8>, MediaError> {
    let rgb = match img {
        image::DynamicImage::ImageRgb8(buf) => buf.clone(),
        other => {
            let rgba = other.to_rgba8();
            let mut flat = image::RgbImage::new(rgba.width(), rgba.height());
            for (x, y, px) in rgba.enumerate_pixels() {
                let a = f32::from(px[3]) / 255.0;
                let over = |c: u8| (f32::from(c) * a + 255.0 * (1.0 - a)).round() as u8;
                flat.put_pixel(x, y, image::Rgb([over(px[0]), over(px[1]), over(px[2])]));
            }
            flat
        }
    };

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut out), quality)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(MediaError::Encode)?;
    Ok(out)
}

/// Shrink `img` so its long edge is at most [`MAX_LONG_EDGE`], or return `None`
/// when it already fits. `resize` preserves the aspect ratio, fitting the image
/// inside the box rather than stretching it to the box.
fn shrink(img: &image::DynamicImage) -> Option<image::DynamicImage> {
    use image::GenericImageView;
    let (w, h) = img.dimensions();
    (w.max(h) > MAX_LONG_EDGE).then(|| {
        img.resize(
            MAX_LONG_EDGE,
            MAX_LONG_EDGE,
            image::imageops::FilterType::Lanczos3,
        )
    })
}

/// Prepare a single image for the request: downscale if oversized, otherwise
/// pass the original bytes through untouched.
///
/// Passing through matters. A 900×1200 phone-cropped receipt that already fits
/// gains nothing from a re-encode and loses the fine strokes on a faded thermal
/// print, which is exactly the text extraction depends on.
pub fn prepare_image(bytes: &[u8], mime: &str) -> Result<PreparedImage, MediaError> {
    let Some(format) = format_for(mime) else {
        // Unreachable today — `OpenAiCompatExtractor::supports` gates to the
        // three types `format_for` knows. It errors rather than passing the
        // bytes through because the only alternative is inventing a MIME for
        // them, and a HEIC labelled `image/jpeg` is a confusing endpoint
        // rejection at best and a misparse at worst. Whoever widens `supports`
        // should be stopped here, loudly, rather than debugging that.
        return Err(MediaError::UnpreparableMime {
            mime: mime.to_string(),
        });
    };

    let img =
        image::load_from_memory_with_format(bytes, format).map_err(|e| MediaError::Decode {
            mime: mime.to_string(),
            source: e,
        })?;

    let prepared = match shrink(&img) {
        Some(small) => PreparedImage {
            bytes: encode_jpeg(&small, JPEG_QUALITY_LADDER[0])?,
            mime: "image/jpeg",
        },
        None => PreparedImage {
            bytes: bytes.to_vec(),
            // Borrowed from the input rather than re-derived: these are the
            // original bytes, so the original MIME is still the true one.
            mime: match format {
                ImageFormat::Png => "image/png",
                ImageFormat::WebP => "image/webp",
                _ => "image/jpeg",
            },
        },
    };

    // An already-small image can still be over budget (a 12 MP PNG screenshot
    // is under 2048px on neither edge, but a lossless 4000×3000 one is not).
    if check_budget(std::slice::from_ref(&prepared)).is_err() {
        let bytes = encode_jpeg(&img, JPEG_QUALITY_LADDER[0])?;
        let recoded = PreparedImage {
            bytes,
            mime: "image/jpeg",
        };
        check_budget(std::slice::from_ref(&recoded))?;
        return Ok(recoded);
    }
    Ok(prepared)
}

/// Total base64 length of `images` against [`MAX_ENCODED_BYTES`].
fn check_budget(images: &[PreparedImage]) -> Result<(), MediaError> {
    let encoded: usize = images.iter().map(PreparedImage::encoded_len).sum();
    if encoded > MAX_ENCODED_BYTES {
        return Err(MediaError::TooLarge { encoded });
    }
    Ok(())
}

/// Render a scanned PDF's pages to images the endpoint can read.
///
/// Only reached when `pdftotext` came back empty — a generated PDF still goes
/// the text route, which is both cheaper and more accurate than asking a model
/// to read a picture of text it could have had verbatim.
pub async fn rasterize_pdf(pdf_bytes: &[u8]) -> Result<Vec<PreparedImage>, MediaError> {
    if pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(MediaError::PdfTooLarge {
            size: pdf_bytes.len(),
        });
    }

    let mut temp = tempfile::NamedTempFile::new()
        .map_err(|e| MediaError::Spawn(format!("create temp: {e}")))?;
    use std::io::Write;
    temp.write_all(pdf_bytes)
        .map_err(|e| MediaError::Spawn(format!("write temp: {e}")))?;
    temp.flush()
        .map_err(|e| MediaError::Spawn(format!("flush temp: {e}")))?;

    let out_dir =
        tempfile::tempdir().map_err(|e| MediaError::Spawn(format!("create temp dir: {e}")))?;
    let pages = run_pdftoppm(temp.path(), out_dir.path()).await?;

    // `-l` clamps silently to the document's own page count, so asking for one
    // page past the cap is how "there are more" is distinguished from "that is
    // all of them".
    if pages.len() > MAX_RASTER_PAGES {
        return Err(MediaError::TooManyPages);
    }

    let decoded: Vec<image::DynamicImage> = pages
        .iter()
        .map(|raw| {
            image::load_from_memory_with_format(raw, ImageFormat::Jpeg).map_err(|e| {
                MediaError::Decode {
                    mime: "image/jpeg".into(),
                    source: e,
                }
            })
        })
        .collect::<Result<_, _>>()?;

    // Walk the quality ladder rather than failing at the first overflow: eight
    // dense pages can land just over budget, and a slightly softer JPEG is a
    // far better answer than refusing the document. The ladder is finite, so a
    // genuinely oversized document still ends in `TooLarge`.
    let mut last = None;
    for quality in JPEG_QUALITY_LADDER {
        let prepared = decoded
            .iter()
            .map(|img| {
                let bytes = match shrink(img) {
                    Some(small) => encode_jpeg(&small, quality)?,
                    None => encode_jpeg(img, quality)?,
                };
                Ok(PreparedImage {
                    bytes,
                    mime: "image/jpeg",
                })
            })
            .collect::<Result<Vec<_>, MediaError>>()?;

        match check_budget(&prepared) {
            Ok(()) => return Ok(prepared),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("ladder is non-empty"))
}

/// Spawn `pdftoppm` into `out_dir` and read the page images back in page order.
async fn run_pdftoppm(pdf: &Path, out_dir: &Path) -> Result<Vec<Vec<u8>>, MediaError> {
    let prefix = out_dir.join("page");
    // `kill_on_drop` pairs with the timeout: without it a poppler stuck on a
    // malformed page outlives its own deadline and keeps the CPU.
    let child = Command::new("pdftoppm")
        .arg("-jpeg")
        .arg("-r")
        .arg(RASTER_DPI.to_string())
        .arg("-l")
        .arg((MAX_RASTER_PAGES + 1).to_string())
        .arg(pdf)
        .arg(&prefix)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| MediaError::Spawn(e.to_string()))?;

    let output = match tokio::time::timeout(PDFTOPPM_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result.map_err(|e| MediaError::Spawn(e.to_string()))?,
        Err(_) => {
            return Err(MediaError::Failed {
                status: format!("timed out after {}s", PDFTOPPM_TIMEOUT.as_secs()),
                stderr: String::new(),
            });
        }
    };
    if !output.status.success() {
        return Err(MediaError::Failed {
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // poppler names files `page-1.jpg`, zero-padded to the width of the page
    // count — so `page-10` sorts before `page-2` lexicographically. Sorting on
    // the parsed number instead keeps a statement's rows in the right order
    // regardless of how many pages it has.
    let mut entries: Vec<(u32, std::path::PathBuf)> = std::fs::read_dir(out_dir)
        .map_err(|e| MediaError::Spawn(format!("read output dir: {e}")))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?;
            let n = stem.rsplit_once('-')?.1.parse::<u32>().ok()?;
            Some((n, path))
        })
        .collect();
    entries.sort_by_key(|(n, _)| *n);

    if entries.is_empty() {
        return Err(MediaError::Failed {
            status: "ok".into(),
            stderr: "pdftoppm produced no pages".into(),
        });
    }

    entries
        .into_iter()
        .map(|(_, path)| {
            std::fs::read(&path).map_err(|e| MediaError::Spawn(format!("read page: {e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a solid-colour test image of the given size in the given format.
    fn synth(w: u32, h: u32, format: ImageFormat) -> Vec<u8> {
        // Noise, not a flat fill: a solid image compresses to almost nothing,
        // which would make every budget assertion below pass for the wrong
        // reason.
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 239) as u8]);
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    /// The gap this module exists to close: a phone-sized photo must come back
    /// within the request budget instead of 413-ing at the endpoint.
    #[test]
    fn a_phone_sized_photo_is_downscaled_under_budget() {
        let photo = synth(4032, 3024, ImageFormat::Jpeg);
        let prepared = prepare_image(&photo, "image/jpeg").unwrap();
        assert!(
            prepared.encoded_len() <= MAX_ENCODED_BYTES,
            "still over budget: {} base64 bytes",
            prepared.encoded_len()
        );
        let img = image::load_from_memory_with_format(&prepared.bytes, ImageFormat::Jpeg).unwrap();
        use image::GenericImageView;
        let (w, h) = img.dimensions();
        assert_eq!(w.max(h), MAX_LONG_EDGE, "long edge not clamped: {w}x{h}");
    }

    /// An image already inside both limits must be forwarded byte-identical —
    /// re-encoding a faded thermal receipt costs detail for no gain.
    #[test]
    fn a_small_image_passes_through_untouched() {
        let small = synth(800, 600, ImageFormat::Png);
        let prepared = prepare_image(&small, "image/png").unwrap();
        assert_eq!(prepared.bytes, small);
        assert_eq!(prepared.mime, "image/png");
    }

    /// A lossless screenshot can be under 2048px on both edges and still blow
    /// the budget; the size check has to be independent of the pixel check.
    #[test]
    fn a_small_but_heavy_image_is_recoded() {
        let heavy = synth(2000, 2000, ImageFormat::Png);
        assert!(
            heavy.len().div_ceil(3) * 4 > MAX_ENCODED_BYTES,
            "fixture is not actually over budget ({} raw bytes)",
            heavy.len()
        );
        let prepared = prepare_image(&heavy, "image/png").unwrap();
        assert_eq!(prepared.mime, "image/jpeg");
        assert!(prepared.encoded_len() <= MAX_ENCODED_BYTES);
    }

    /// Transparency must be composited onto white, not dropped. Alpha-dropping
    /// turns a transparent receipt screenshot black and hides the text.
    #[test]
    fn transparency_flattens_to_white_not_black() {
        let mut rgba = image::RgbaImage::new(4, 4);
        for px in rgba.pixels_mut() {
            *px = image::Rgba([0, 0, 0, 0]); // fully transparent black
        }
        let jpeg = encode_jpeg(&image::DynamicImage::ImageRgba8(rgba), 90).unwrap();
        let back = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg).unwrap();
        let px = back.to_rgb8().get_pixel(1, 1).0;
        assert!(
            px.iter().all(|&c| c > 200),
            "transparent pixels did not flatten to white: {px:?}"
        );
    }

    /// ⚠️ `supports()` and `format_for()` are two lists of MIME types that must
    /// agree. This asserts they do — widening one without the other is the
    /// exact drift `UnpreparableMime` exists to catch, and catching it in a
    /// test is cheaper than catching it against a live endpoint.
    #[test]
    fn every_supported_image_type_has_a_downscaler() {
        use crate::extraction::DocumentExtractor;
        let ext = crate::extraction::openai_compat::OpenAiCompatExtractor::new("u", "m", "");
        for mime in ["image/jpeg", "image/png", "image/webp"] {
            assert!(
                ext.supports(mime),
                "{mime} no longer supported — update this list"
            );
            assert!(
                format_for(mime).is_some(),
                "{mime} is supported but has no downscaler"
            );
        }
        assert!(
            matches!(
                prepare_image(b"x", "image/heic"),
                Err(MediaError::UnpreparableMime { .. })
            ),
            "an unknown image type must fail loudly, not be relabelled"
        );
    }

    /// The size gate must reject before poppler is spawned.
    #[tokio::test]
    async fn an_oversized_pdf_is_refused_before_poppler_sees_it() {
        let huge = vec![0u8; MAX_PDF_BYTES + 1];
        let err = rasterize_pdf(&huge).await.unwrap_err();
        assert!(matches!(err, MediaError::PdfTooLarge { .. }), "{err}");
    }

    /// Bytes that are not a PDF must fail loudly rather than yield zero pages
    /// that would read as an empty but valid document.
    #[tokio::test]
    async fn a_non_pdf_fails_rather_than_returning_no_pages() {
        let err = rasterize_pdf(b"this is not a pdf").await.unwrap_err();
        // Absent poppler this is a Spawn error; the assertion is that no path
        // reaches `Ok` with an empty page list.
        assert!(
            matches!(
                err,
                MediaError::Failed { .. } | MediaError::Spawn(_) | MediaError::Decode { .. }
            ),
            "unexpected: {err}"
        );
    }
}
