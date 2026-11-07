//! Line icons drawn as vectors, so they look the same everywhere and need no icon font.
//! Shapes are designed on a 24 × 24 grid and scaled to any size.

use eframe::egui::{
    self, Color32, CornerRadius, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2, WidgetText, pos2, vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Users,
    Package,
    Document,
    Wallet,
    Chart,
    History,
    Settings,
    Globe,
    UserPlus,
    Receipt,
    Percent,
    SignIn,
    Close,
    Mail,
    Inbox,
    ArrowUp,
    Server,
    Bank,
    Repeat,
    Lock,
    Download,
}

struct Pen<'a> {
    p: &'a Painter,
    rect: Rect,
    stroke: Stroke,
}

impl Pen<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        let s = self.rect.width() / 24.0;
        pos2(self.rect.left() + x * s, self.rect.top() + y * s)
    }
    fn scale(&self) -> f32 {
        self.rect.width() / 24.0
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::line(pts, self.stroke));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::closed_line(pts, self.stroke));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.pt(x, y), r * self.scale(), self.stroke);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.pt(x, y), r * self.scale(), self.stroke.color);
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) {
        let rect = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect_stroke(
            rect,
            CornerRadius::same((r * self.scale()) as u8),
            self.stroke,
            StrokeKind::Middle,
        );
    }
    /// An arc from angle `a0` to `a1` (degrees, 0 = right, 90 = down).
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32) {
        let steps = 18;
        let pts: Vec<(f32, f32)> = (0..=steps)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians();
                (x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.line(&pts);
    }
    fn ellipse(&self, x: f32, y: f32, rx: f32, ry: f32) {
        let pts: Vec<(f32, f32)> = (0..36)
            .map(|i| {
                let a = (i as f32 * 10.0).to_radians();
                (x + rx * a.cos(), y + ry * a.sin())
            })
            .collect();
        self.closed(&pts);
    }
}

/// Draws an icon into a square.
pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let rect = Rect::from_center_size(rect.center(), Vec2::splat(size));
    let pen = Pen { p, rect, stroke: Stroke::new((size / 24.0 * 1.8).max(1.2), color) };
    match icon {
        Icon::Home => {
            pen.line(&[(3.0, 11.0), (12.0, 3.5), (21.0, 11.0)]);
            pen.line(&[(5.5, 9.5), (5.5, 20.5), (18.5, 20.5), (18.5, 9.5)]);
            pen.line(&[(10.0, 20.5), (10.0, 14.5), (14.0, 14.5), (14.0, 20.5)]);
        }
        Icon::Users => {
            pen.circle(9.0, 8.0, 3.6);
            pen.arc(9.0, 21.0, 6.5, 180.0, 360.0);
            pen.arc(16.5, 8.5, 3.0, -80.0, 90.0);
            pen.arc(16.5, 20.5, 5.5, 270.0, 360.0);
        }
        Icon::Package => {
            pen.closed(&[(12.0, 2.5), (20.5, 7.0), (20.5, 17.0), (12.0, 21.5), (3.5, 17.0), (3.5, 7.0)]);
            pen.line(&[(3.5, 7.0), (12.0, 11.5), (20.5, 7.0)]);
            pen.line(&[(12.0, 11.5), (12.0, 21.5)]);
            pen.line(&[(7.75, 4.75), (16.25, 9.25)]);
        }
        Icon::Document => {
            pen.closed(&[(5.5, 2.5), (14.5, 2.5), (19.0, 7.0), (19.0, 21.5), (5.5, 21.5)]);
            pen.line(&[(14.5, 2.5), (14.5, 7.0), (19.0, 7.0)]);
            pen.line(&[(9.0, 12.0), (15.5, 12.0)]);
            pen.line(&[(9.0, 16.0), (15.5, 16.0)]);
        }
        Icon::Wallet => {
            pen.rect(3.0, 6.0, 21.0, 19.5, 2.5);
            pen.line(&[(5.0, 6.0), (16.0, 3.0), (17.0, 6.0)]);
            pen.rect(15.0, 10.5, 21.0, 15.0, 1.0);
            pen.dot(17.5, 12.75, 0.9);
        }
        Icon::Chart => {
            pen.line(&[(3.5, 3.0), (3.5, 20.5), (21.0, 20.5)]);
            pen.rect(7.0, 12.5, 10.0, 17.5, 0.5);
            pen.rect(12.0, 7.0, 15.0, 17.5, 0.5);
            pen.rect(17.0, 10.0, 20.0, 17.5, 0.5);
        }
        Icon::History => {
            pen.arc(12.0, 12.0, 8.5, -150.0, 180.0);
            pen.line(&[(3.5, 7.0), (4.6, 7.8), (6.0, 6.5)]);
            pen.line(&[(12.0, 7.5), (12.0, 12.0), (15.0, 14.0)]);
        }
        Icon::Settings => {
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let (c, s) = (a.cos(), a.sin());
                pen.line(&[(12.0 + 7.0 * c, 12.0 + 7.0 * s), (12.0 + 9.5 * c, 12.0 + 9.5 * s)]);
            }
            pen.circle(12.0, 12.0, 6.8);
            pen.circle(12.0, 12.0, 2.8);
        }
        Icon::Globe => {
            pen.circle(12.0, 12.0, 9.0);
            pen.ellipse(12.0, 12.0, 4.0, 9.0);
            pen.line(&[(3.0, 12.0), (21.0, 12.0)]);
        }
        Icon::UserPlus => {
            pen.circle(9.5, 8.0, 3.6);
            pen.arc(9.5, 21.0, 6.5, 180.0, 360.0);
            pen.line(&[(19.0, 8.0), (19.0, 14.0)]);
            pen.line(&[(16.0, 11.0), (22.0, 11.0)]);
        }
        Icon::Receipt => {
            pen.line(&[
                (5.5, 2.5), (5.5, 21.5), (8.0, 20.0), (10.5, 21.5), (13.0, 20.0), (15.5, 21.5),
                (18.5, 20.0), (18.5, 2.5), (5.5, 2.5),
            ]);
            pen.line(&[(9.0, 8.0), (15.0, 8.0)]);
            pen.line(&[(9.0, 12.0), (15.0, 12.0)]);
            pen.line(&[(9.0, 16.0), (12.5, 16.0)]);
        }
        Icon::Percent => {
            pen.line(&[(19.0, 5.0), (5.0, 19.0)]);
            pen.circle(7.0, 7.0, 2.5);
            pen.circle(17.0, 17.0, 2.5);
        }
        Icon::Inbox => {
            pen.closed(&[(3.0, 13.0), (6.0, 4.5), (18.0, 4.5), (21.0, 13.0), (21.0, 19.5), (3.0, 19.5)]);
            pen.line(&[(3.0, 13.0), (8.0, 13.0), (9.5, 16.0), (14.5, 16.0), (16.0, 13.0), (21.0, 13.0)]);
        }
        Icon::Mail => {
            pen.rect(2.5, 5.0, 21.5, 19.0, 2.0);
            pen.line(&[(3.0, 6.5), (12.0, 13.0), (21.0, 6.5)]);
        }
        Icon::Close => {
            pen.line(&[(6.0, 6.0), (18.0, 18.0)]);
            pen.line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::ArrowUp => {
            pen.line(&[(12.0, 19.0), (12.0, 5.0)]);
            pen.line(&[(6.0, 11.0), (12.0, 5.0), (18.0, 11.0)]);
        }
        Icon::SignIn => {
            pen.line(&[(14.0, 3.5), (19.5, 3.5), (19.5, 20.5), (14.0, 20.5)]);
            pen.line(&[(3.5, 12.0), (14.0, 12.0)]);
            pen.line(&[(10.0, 8.0), (14.0, 12.0), (10.0, 16.0)]);
        }
        Icon::Server => {
            pen.rect(3.5, 3.5, 20.5, 10.5, 1.5);
            pen.rect(3.5, 13.5, 20.5, 20.5, 1.5);
            pen.dot(7.0, 7.0, 0.9);
            pen.dot(7.0, 17.0, 0.9);
        }
        Icon::Bank => {
            pen.closed(&[(3.0, 9.0), (12.0, 3.5), (21.0, 9.0)]);
            for x in [6.0, 10.0, 14.0, 18.0] {
                pen.line(&[(x, 11.0), (x, 17.5)]);
            }
            pen.line(&[(3.0, 20.5), (21.0, 20.5)]);
        }
        Icon::Repeat => {
            pen.arc(12.0, 12.0, 7.5, 200.0, 350.0);
            pen.line(&[(17.0, 6.0), (19.4, 10.0), (15.2, 10.6)]);
            pen.arc(12.0, 12.0, 7.5, 20.0, 170.0);
            pen.line(&[(7.0, 18.0), (4.6, 14.0), (8.8, 13.4)]);
        }
        Icon::Lock => {
            pen.rect(5.0, 10.5, 19.0, 20.5, 2.0);
            pen.arc(12.0, 10.5, 4.5, 180.0, 360.0);
            pen.dot(12.0, 15.5, 1.2);
        }
        Icon::Download => {
            pen.line(&[(12.0, 3.5), (12.0, 15.0)]);
            pen.line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            pen.line(&[(4.0, 16.5), (4.0, 20.5), (20.0, 20.5), (20.0, 16.5)]);
        }
    }
}

/// An icon on its own, e.g. next to a label.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, icon, color);
    resp
}

/// A small square button showing only an icon.
pub fn icon_only_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let size = Vec2::splat(ui.spacing().interact_size.y - 2.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);
        paint_icon(ui.painter(), rect.shrink(6.0), icon, visuals.text_color());
    }
    resp.on_hover_text(tooltip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A button with an icon in front of its text.
pub fn icon_button(ui: &mut Ui, icon: Icon, text: impl Into<WidgetText>) -> Response {
    let pad = ui.spacing().button_padding;
    let icon_size = 16.0;
    let galley = text.into().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    let size = vec2(
        pad.x * 2.0 + icon_size + 8.0 + galley.size().x,
        (galley.size().y.max(icon_size) + pad.y * 2.0).max(ui.spacing().interact_size.y),
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let icon_rect = Rect::from_center_size(
            pos2(rect.left() + pad.x + icon_size / 2.0, rect.center().y),
            Vec2::splat(icon_size),
        );
        paint_icon(ui.painter(), icon_rect, icon, crate::ui_kit::ACCENT);
        let text_pos = pos2(icon_rect.right() + 8.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
