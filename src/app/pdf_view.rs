//! Draws PDF pages on screen exactly as they print: same fonts, same positions.

use super::*;
use crate::pdf::{Align as PdfAlign, PAGE_SIZE, PreviewPage, Rgb, Shape};

/// Registers the PDF fonts so previews measure text like the printed page.
pub fn install_fonts(ctx: &egui::Context) {
    let (regular, bold) = crate::pdf::preview_fonts();
    for (name, data) in [("pdf", regular), ("pdf-bold", bold)] {
        ctx.add_font(egui::FontInsert::new(
            name,
            egui::FontData::from_static(data),
            vec![egui::InsertFontFamily { family: egui::FontFamily::Name(name.into()), priority: egui::FontPriority::Highest }],
        ));
    }
}

fn color(c: Rgb) -> Color32 {
    Color32::from_rgb((c.0 * 255.0) as u8, (c.1 * 255.0) as u8, (c.2 * 255.0) as u8)
}

/// The picture of a slot as a texture, made once and kept while the picture stays the same.
fn texture(ctx: &egui::Context, logo: &Logo) -> Option<egui::TextureHandle> {
    if logo.width == 0 || logo.height == 0 {
        return None;
    }
    let key = Id::unique(("pdf_image", logo.width, logo.height, logo.rgb.len(), logo.rgb.get(..64).unwrap_or(&logo.rgb), logo.rgb.get(logo.rgb.len().saturating_sub(64)..).unwrap_or("")));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(t);
    }
    let pixels = crate::pdf::logo_pixels(logo);
    if pixels.len() < (logo.width * logo.height * 3) as usize {
        return None;
    }
    let image = egui::ColorImage::from_rgb([logo.width as usize, logo.height as usize], &pixels[..(logo.width * logo.height * 3) as usize]);
    let t = ctx.load_texture("pdf_image", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(key, t.clone()));
    Some(t)
}

/// Draws the pages one below the other, `width` pixels wide.
pub fn show_pages(ui: &mut Ui, pages: &[PreviewPage], width: f32) {
    let (pw, ph) = PAGE_SIZE;
    let scale = width / pw;
    for (n, page) in pages.iter().enumerate() {
        if n > 0 {
            ui.add_space(10.0);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(width, ph * scale), egui::Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let painter = ui.painter_at(rect.expand(1.0));
        painter.rect_filled(rect.translate(vec2(0.0, 3.0)).expand(2.0), CornerRadius::same(2), Color32::from_black_alpha(18));
        painter.rect_filled(rect, CornerRadius::ZERO, Color32::WHITE);
        painter.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);
        let at = |x: f32, y: f32| rect.min + vec2(x * scale, (ph - y) * scale);
        let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        for shape in &page.shapes {
            match shape {
                Shape::Text { x, y, size, bold, color: c, align, text } => {
                    let family = egui::FontFamily::Name(if *bold { "pdf-bold" } else { "pdf" }.into());
                    let galley = painter.layout_no_wrap(text.clone(), egui::FontId::new(size * scale, family), color(*c));
                    let baseline = galley
                        .rows
                        .first()
                        .map(|r| r.pos.y + r.row.glyphs.first().map(|g| g.pos.y).unwrap_or(size * scale * 0.9))
                        .unwrap_or(size * scale * 0.9);
                    let p = at(*x, *y);
                    let left = match align {
                        PdfAlign::Left => p.x,
                        PdfAlign::Right => p.x - galley.size().x,
                    };
                    painter.galley(egui::pos2(left, p.y - baseline), galley, color(*c));
                }
                Shape::Line { x1, y1, x2, y2, width, color: c } => {
                    painter.line_segment([at(*x1, *y1), at(*x2, *y2)], Stroke::new((width * scale).max(0.5), color(*c)));
                }
                Shape::Rect { x, y, w, h, color: c } => {
                    painter.rect_filled(egui::Rect::from_two_pos(at(*x, *y), at(x + w, y + h)), CornerRadius::ZERO, color(*c));
                }
                Shape::Image { slot, x, y, w, h } => {
                    if let Some((_, logo)) = page.images.iter().find(|(s, _)| s == slot)
                        && let Some(t) = texture(ui.ctx(), logo)
                    {
                        painter.image(t.id(), egui::Rect::from_two_pos(at(*x, *y), at(x + w, y + h)), full_uv, Color32::WHITE);
                    }
                }
            }
        }
    }
}
