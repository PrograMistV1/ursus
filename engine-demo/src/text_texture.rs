use resvg::{tiny_skia, usvg};
use std::sync::Arc;

/// Renders text on a colored background in an RGBA8 pixel buffer using SVG.
/// Not intended to be reusable - just a quick way to get
/// a texture with a caption for material testing.
pub fn render_label_texture(lines: &[&str], bg_color: &str, fg_color: &str) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let font_size = 40.0;
    let line_height = 32.0;
    let center_y = 128.0;

    let total_height = line_height * lines.len() as f32;
    let start_y = center_y - total_height / 2.0 + line_height / 2.0;

    let tspans: String = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let y = start_y + line_height * i as f32;
            format!(r#"<tspan x="128" y="{y}">{line}</tspan>"#)
        })
        .collect();

    let svg = format!(
        r#"<svg width="256" height="256" xmlns="http://www.w3.org/2000/svg">
            <rect width="256" height="256" fill="{bg_color}"/>
            <text font-size="{font_size}" font-family="sans-serif"
                  fill="{fg_color}" text-anchor="middle">{tspans}</text>
        </svg>"#
    );

    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    let options = usvg::Options { fontdb: Arc::new(fontdb), ..Default::default() };

    let tree = usvg::Tree::from_str(&svg, &options)
        .map_err(|e| anyhow::anyhow!("SVG parsing error for texture '{:?}': {}", lines, e))?;

    let mut pixmap = tiny_skia::Pixmap::new(256, 256)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap for texture '{:?}'", lines))?;

    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    let mut pixels = pixmap.data().to_vec();
    for chunk in pixels.chunks_mut(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.001 {
            chunk[0] = (chunk[0] as f32 / a).min(255.0) as u8;
            chunk[1] = (chunk[1] as f32 / a).min(255.0) as u8;
            chunk[2] = (chunk[2] as f32 / a).min(255.0) as u8;
        }
    }

    Ok((pixels, 256, 256))
}
