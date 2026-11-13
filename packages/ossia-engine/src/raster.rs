use crate::{
    cache::{self, Cache},
    *,
};
use imagequant::RGBA;
use resvg::{tiny_skia, usvg};

impl Engine {
    /// Rasterize a verified SVG page with bounded dimensions and no external image reads.
    pub fn rasterize_svg(
        &self,
        svg: &[u8],
        width: u32,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef, AppError> {
        self.rasterize_svg_with_options(svg, width, None, false, root, context)
    }

    /// Rasterize a score page, optionally quantizing it and preserving a transparent background.
    pub fn rasterize_svg_with_options(
        &self,
        svg: &[u8],
        width: u32,
        quality: Option<u8>,
        transparent: bool,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef, AppError> {
        context.check()?;
        if width == 0 || width > 4096 {
            return Err(AppError::invalid("PNG width must be 1..4096"));
        }
        if quality.is_some_and(|quality| !(1..=100).contains(&quality)) {
            return Err(AppError::invalid("PNG quality must be 1..100"));
        }
        let mut options = usvg::Options::default();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
        options.fontdb_mut().load_system_fonts();
        options.fontdb_mut().load_fonts_dir(
            self.root
                .join(format!("share/lilypond/{}/fonts", self.version)),
        );
        let tree = usvg::Tree::from_data(svg, &options)
            .map_err(|e| AppError::invalid(format!("invalid preview SVG: {e}")))?;
        let scale = width as f32 / tree.size().width();
        let height = (tree.size().height() * scale).ceil() as u32;
        if height == 0 || u64::from(width) * u64::from(height) > 16_000_000 {
            return Err(AppError::invalid("PNG exceeds 16 million pixels"));
        }
        let mut fonts = Vec::new();
        for face in options.fontdb.faces() {
            let source = match &face.source {
                usvg::fontdb::Source::File(path) => {
                    let meta = fs::metadata(path).map_err(io_error)?;
                    format!(
                        "{}:{}:{:?}",
                        path.display(),
                        meta.len(),
                        meta.modified().map_err(io_error)?
                    )
                }
                usvg::fontdb::Source::Binary(data) => {
                    ContentHash::of(data.as_ref().as_ref()).to_string()
                }
            };
            fonts.push(format!("{source}:{}:{:?}", face.index, face.families));
        }
        fonts.sort();
        let key = ContentHash::of(
            &serde_json::to_vec(&(
                ContentHash::of(svg),
                width,
                quality,
                transparent,
                "resvg-0.49.0-png-v2",
                fonts,
            ))
            .unwrap(),
        );
        let cache = Cache::new(root)?;
        let _lock = cache.lock(key.as_str(), context)?;
        let manifest = cache::confined(&cache.root, &format!("png/{key}.json"))?;
        if manifest.exists() {
            let asset: ArtifactRef =
                serde_json::from_slice(&fs::read(&manifest).map_err(io_error)?)
                    .map_err(|e| AppError::invalid(e.to_string()))?;
            let bytes = fs::read(cache::confined(&cache.root, &asset.path)?).map_err(io_error)?;
            if ContentHash::of(&bytes) != asset.sha256 || bytes.len() as u64 != asset.bytes {
                return Err(AppError::invalid("cached PNG is damaged"));
            }
            return Ok(asset);
        }
        context.check()?;
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| AppError::invalid("PNG allocation failed"))?;
        pixmap.fill(if transparent {
            tiny_skia::Color::from_rgba8(0, 0, 0, 0)
        } else {
            tiny_skia::Color::WHITE
        });
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        context.check()?;
        let bytes = quality.map_or_else(
            || {
                pixmap
                    .encode_png()
                    .map_err(|e| AppError::invalid(e.to_string()))
            },
            |quality| encode_quantized_png(&pixmap, quality),
        )?;
        let asset = cache.asset(&bytes, "png")?;
        cache::atomic_write(&manifest, &serde_json::to_vec(&asset).unwrap())?;
        Ok(asset)
    }
}

fn encode_quantized_png(pixmap: &tiny_skia::Pixmap, quality: u8) -> Result<Vec<u8>, AppError> {
    let pixels = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            let alpha = pixel[3];
            if alpha == 0 {
                return RGBA::new(0, 0, 0, 0);
            }
            let restore = |value: u8| ((u16::from(value) * 255) / u16::from(alpha)).min(255) as u8;
            RGBA::new(
                restore(pixel[0]),
                restore(pixel[1]),
                restore(pixel[2]),
                alpha,
            )
        })
        .collect::<Vec<_>>();
    let mut attributes = imagequant::Attributes::new();
    attributes
        .set_quality(1, quality)
        .map_err(|error| AppError::invalid(format!("invalid PNG quality: {error}")))?;
    let mut image = attributes
        .new_image(
            pixels,
            pixmap.width() as usize,
            pixmap.height() as usize,
            0.0,
        )
        .map_err(|error| {
            AppError::invalid(format!("could not prepare PNG quantization: {error}"))
        })?;
    let mut result = attributes
        .quantize(&mut image)
        .map_err(|error| AppError::invalid(format!("could not quantize PNG: {error}")))?;
    let (palette, indices) = result
        .remapped(&mut image)
        .map_err(|error| AppError::invalid(format!("could not remap PNG palette: {error}")))?;
    let palette_bytes = palette
        .iter()
        .flat_map(|color| [color.r, color.g, color.b])
        .collect::<Vec<_>>();
    let transparency = palette.iter().map(|color| color.a).collect::<Vec<_>>();
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, pixmap.width(), pixmap.height());
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(palette_bytes);
        if transparency.iter().any(|alpha| *alpha != 255) {
            encoder.set_trns(transparency);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|error| AppError::invalid(format!("could not write PNG header: {error}")))?;
        writer
            .write_image_data(&indices)
            .map_err(|error| AppError::invalid(format!("could not write PNG pixels: {error}")))?;
    }
    Ok(output)
}

#[cfg(test)]
#[path = "../tests/unit/raster.rs"]
mod tests;
