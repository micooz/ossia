use crate::{
    cache::{self, Cache},
    *,
};
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
        context.check()?;
        if width == 0 || width > 4096 {
            return Err(AppError::invalid("PNG width must be 1..4096"));
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
            &serde_json::to_vec(&(ContentHash::of(svg), width, "resvg-0.49.0-white-v1", fonts))
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
        pixmap.fill(tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        context.check()?;
        let bytes = pixmap
            .encode_png()
            .map_err(|e| AppError::invalid(e.to_string()))?;
        let asset = cache.asset(&bytes, "png")?;
        cache::atomic_write(&manifest, &serde_json::to_vec(&asset).unwrap())?;
        Ok(asset)
    }
}
