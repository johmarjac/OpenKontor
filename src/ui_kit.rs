//! Shared look: colours, cards, buttons, tables and input fields.

use crate::model::*;
use crate::i18n::{t, to_german, tr};
use crate::tf;
use chrono::NaiveDate;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Id, Margin, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2, pos2, vec2,
};

pub const NAV_BG: Color32 = Color32::from_rgb(0x15, 0x2e, 0x47);
pub const NAV_SEL: Color32 = Color32::from_rgb(0x23, 0x4b, 0x70);
pub const NAV_TEXT: Color32 = Color32::from_rgb(0xc9, 0xd6, 0xe3);
pub const ACCENT: Color32 = Color32::from_rgb(0x1f, 0x6f, 0x8b);
pub const TEAL: Color32 = Color32::from_rgb(0x2f, 0xb3, 0x9c);
pub const BG: Color32 = Color32::from_rgb(0xf2, 0xf4, 0xf7);
pub const CARD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(0xe0, 0xe5, 0xeb);
pub const TEXT: Color32 = Color32::from_rgb(0x1d, 0x27, 0x33);
pub const MUTED: Color32 = Color32::from_rgb(0x6b, 0x77, 0x85);
pub const GREEN: Color32 = Color32::from_rgb(0x1f, 0x8a, 0x55);
pub const RED: Color32 = Color32::from_rgb(0xc4, 0x3d, 0x3d);
pub const ORANGE: Color32 = Color32::from_rgb(0xd9, 0x82, 0x1a);
pub const INCOME: Color32 = Color32::from_rgb(0x2f, 0xb3, 0x9c);
pub const EXPENSE: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x6c);

pub fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 7.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.visuals.panel_fill = BG;
        s.visuals.window_fill = CARD;
        s.visuals.extreme_bg_color = Color32::WHITE;
        s.visuals.override_text_color = Some(TEXT);
        s.visuals.selection.bg_fill = Color32::from_rgb(0xcf, 0xe6, 0xef);
        s.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        s.visuals.hyperlink_color = ACCENT;
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xf6, 0xf8, 0xfa);
        s.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.active.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.open.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        for (style, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 11.5),
            (egui::TextStyle::Heading, 21.0),
            (egui::TextStyle::Monospace, 13.0),
        ] {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
    });
}

pub fn card_frame() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame().show(ui, add).inner
}

pub fn card_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(15.5).strong());
    ui.add_space(4.0);
}

pub fn primary(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(6)),
    )
}

pub fn primary_enabled(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(6)),
    )
}

pub fn danger(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(text).color(RED)).stroke(Stroke::new(1.0, RED)))
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED)
}

pub fn chip_colors(label: &str) -> (Color32, Color32) {
    match to_german(label) {
        "Entwurf" => (Color32::from_rgb(0xec, 0xef, 0xf3), Color32::from_rgb(0x55, 0x60, 0x6e)),
        "Offen" => (Color32::from_rgb(0xe1, 0xee, 0xfa), Color32::from_rgb(0x1e, 0x5c, 0x96)),
        "Überfällig" => (Color32::from_rgb(0xfc, 0xe6, 0xe6), Color32::from_rgb(0xb3, 0x30, 0x30)),
        "Bezahlt" | "Einnahme" => (Color32::from_rgb(0xdf, 0xf4, 0xe8), Color32::from_rgb(0x1c, 0x75, 0x47)),
        "Angenommen" => (Color32::from_rgb(0xdc, 0xf3, 0xef), Color32::from_rgb(0x1a, 0x76, 0x67)),
        "Abgelehnt" | "Storniert" => (Color32::from_rgb(0xf1, 0xe9, 0xe9), Color32::from_rgb(0x8a, 0x55, 0x55)),
        "Ausgabe" => (Color32::from_rgb(0xfd, 0xec, 0xe5), Color32::from_rgb(0xa8, 0x4a, 0x2a)),
        "Kunde" => (Color32::from_rgb(0xe4, 0xef, 0xf6), Color32::from_rgb(0x1f, 0x5f, 0x7f)),
        "Lieferant" => (Color32::from_rgb(0xf3, 0xee, 0xe2), Color32::from_rgb(0x7a, 0x5a, 0x1c)),
        _ => (Color32::from_rgb(0xec, 0xef, 0xf3), MUTED),
    }
}

pub fn status_label(d: &Document, today: NaiveDate) -> &'static str {
    if d.is_overdue(today) { t("Überfällig") } else { d.status.label() }
}

pub fn chip(ui: &mut Ui, label: &str) {
    let (bg, fg) = chip_colors(label);
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| ui.label(RichText::new(tr(label)).color(fg).size(12.5).strong()));
}

// ---- Tables -------------------------------------------------------------

#[derive(Clone)]
pub enum Cell {
    T(String),
    B(String),
    Muted(String),
    Money(Cents),
    MoneyColored(Cents, Color32),
    Chip(String),
}

pub struct Col {
    pub title: &'static str,
    pub weight: f32,
    pub right: bool,
}

pub const fn col(title: &'static str, weight: f32) -> Col {
    Col { title, weight, right: false }
}
pub const fn rcol(title: &'static str, weight: f32) -> Col {
    Col { title, weight, right: true }
}

fn col_rects(rect: Rect, cols: &[Col]) -> Vec<Rect> {
    let total: f32 = cols.iter().map(|c| c.weight).sum();
    let mut x = rect.left() + 10.0;
    let width = rect.width() - 20.0;
    cols.iter()
        .map(|c| {
            let w = width * c.weight / total;
            let r = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
            x += w;
            r.shrink2(vec2(5.0, 0.0))
        })
        .collect()
}

pub fn table_header(ui: &mut Ui, cols: &[Col]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(0xf5, 0xf7, 0xfa));
    for (c, r) in cols.iter().zip(col_rects(rect, cols)) {
        let (pos, align) = if c.right {
            (pos2(r.right(), r.center().y), Align2::RIGHT_CENTER)
        } else {
            (pos2(r.left(), r.center().y), Align2::LEFT_CENTER)
        };
        p.with_clip_rect(r).text(pos, align, t(c.title), FontId::proportional(12.5), MUTED);
    }
}

/// One clickable table row; the whole row lights up on hover.
pub fn table_row(ui: &mut Ui, cols: &[Col], cells: &[Cell], selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(0xe3, 0xf0, 0xf5));
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(0xf0, 0xf5, 0xf9));
    }
    p.line_segment(
        [pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())],
        Stroke::new(1.0, Color32::from_rgb(0xec, 0xef, 0xf3)),
    );
    for ((c, r), cell) in cols.iter().zip(col_rects(rect, cols)).zip(cells) {
        let pc = p.with_clip_rect(r);
        let (pos, align) = if c.right {
            (pos2(r.right(), r.center().y), Align2::RIGHT_CENTER)
        } else {
            (pos2(r.left(), r.center().y), Align2::LEFT_CENTER)
        };
        let font = FontId::proportional(13.5);
        match cell {
            Cell::T(s) => {
                pc.text(pos, align, s, font, TEXT);
            }
            Cell::B(s) => {
                pc.text(pos, align, s, FontId::proportional(13.5), ACCENT);
            }
            Cell::Muted(s) => {
                pc.text(pos, align, s, font, MUTED);
            }
            Cell::Money(v) => {
                pc.text(pos, align, fmt_money(*v), font, TEXT);
            }
            Cell::MoneyColored(v, col) => {
                pc.text(pos, align, fmt_money(*v), font, *col);
            }
            Cell::Chip(label) => {
                let (bg, fg) = chip_colors(label);
                let galley = pc.layout_no_wrap(tr(label), FontId::proportional(12.0), fg);
                let size = galley.size() + vec2(16.0, 6.0);
                let min = if c.right {
                    pos2(r.right() - size.x, r.center().y - size.y / 2.0)
                } else {
                    pos2(r.left(), r.center().y - size.y / 2.0)
                };
                let chip_rect = Rect::from_min_size(min, size);
                pc.rect_filled(chip_rect, CornerRadius::same(9), bg);
                pc.galley(chip_rect.min + vec2(8.0, 3.0), galley, fg);
            }
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn empty_hint(ui: &mut Ui, text: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| ui.label(muted(text)));
    ui.add_space(20.0);
}

// ---- Inputs -------------------------------------------------------------

/// A money field in German notation ("1.234,56"); edits apply as you type.
pub fn money_edit(ui: &mut Ui, id: Id, value: &mut Cents, width: f32) -> bool {
    let buf_id = id.with("buf");
    let mut buf = ui
        .data(|d| d.get_temp::<String>(buf_id))
        .unwrap_or_else(|| fmt_amount(*value));
    let resp = ui.add(
        TextEdit::singleline(&mut buf)
            .id(id)
            .desired_width(width)
            .align(Align2::RIGHT_CENTER),
    );
    let mut changed = false;
    if resp.changed()
        && let Some(v) = parse_amount(&buf)
    {
        *value = v;
        changed = true;
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(buf_id, buf));
    } else {
        ui.data_mut(|d| d.remove::<String>(buf_id));
    }
    changed
}

pub fn qty_edit(ui: &mut Ui, id: Id, value: &mut f64, width: f32) -> bool {
    let buf_id = id.with("buf");
    let mut buf = ui
        .data(|d| d.get_temp::<String>(buf_id))
        .unwrap_or_else(|| fmt_qty(*value));
    let resp = ui.add(
        TextEdit::singleline(&mut buf)
            .id(id)
            .desired_width(width)
            .align(Align2::RIGHT_CENTER),
    );
    let mut changed = false;
    if resp.changed()
        && let Some(v) = parse_qty(&buf)
    {
        *value = v;
        changed = true;
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(buf_id, buf));
    } else {
        ui.data_mut(|d| d.remove::<String>(buf_id));
    }
    changed
}

/// A date field "TT.MM.JJJJ" with a red frame while the input is not a valid date.
pub fn date_edit(ui: &mut Ui, id: Id, value: &mut NaiveDate) -> bool {
    let buf_id = id.with("buf");
    let mut buf = ui
        .data(|d| d.get_temp::<String>(buf_id))
        .unwrap_or_else(|| fmt_date(*value));
    let valid = parse_date(&buf).is_some();
    let resp = ui.add(
        TextEdit::singleline(&mut buf)
            .id(id)
            .desired_width(96.0)
            .text_color(if valid { TEXT } else { RED }),
    );
    let mut changed = false;
    if resp.changed()
        && let Some(v) = parse_date(&buf)
    {
        *value = v;
        changed = true;
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(buf_id, buf));
    } else {
        ui.data_mut(|d| d.remove::<String>(buf_id));
    }
    changed
}

pub fn vat_combo(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut u8) {
    egui::ComboBox::from_id_salt(id)
        .width(64.0)
        .selected_text(format!("{} %", value))
        .show_ui(ui, |ui| {
            for r in VAT_RATES {
                ui.selectable_value(value, r, format!("{r} %"));
            }
        });
}

/// Edits early-payment discount stages and the net period. Returns true if anything changed.
pub fn discount_editor(ui: &mut Ui, id: Id, discounts: &mut Vec<DiscountStage>, net_days: &mut u32) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (i, s) in discounts.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(muted(t("Skonto")));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut s.percent)
                        .range(0.0..=20.0)
                        .speed(0.1)
                        .custom_formatter(|v, _| format!("{} %", fmt_qty(v)))
                        .custom_parser(|s| parse_qty(s.trim_end_matches('%'))),
                )
                .changed();
            ui.label(muted(t("bei Zahlung innerhalb von")));
            changed |= ui
                .push_id(id.with(i), |ui| ui.add(egui::DragValue::new(&mut s.days).range(0..=365).suffix(t(" Tage"))))
                .inner
                .changed();
            if crate::icons::icon_only_button(ui, crate::icons::Icon::Close, t("Skontostufe entfernen")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        discounts.remove(i);
        changed = true;
    }
    ui.horizontal(|ui| {
        ui.label(muted(t("Zahlungsziel netto")));
        changed |= ui.add(egui::DragValue::new(net_days).range(0..=365).suffix(t(" Tage"))).changed();
        if discounts.len() < 3 && ui.button(t("+ Skontostufe")).clicked() {
            let days = discounts.last().map(|s| s.days + 7).unwrap_or(7).min(net_days.saturating_sub(1));
            discounts.push(DiscountStage { days, percent: 2.0 });
            changed = true;
        }
    });
    if discounts.iter().any(|s| s.days >= *net_days) {
        ui.label(RichText::new(t("Skontofristen müssen kürzer als das Zahlungsziel sein.")).color(ORANGE));
    }
    changed
}

/// A drop-down to choose a payment condition; `none_label` adds an empty choice.
pub fn term_combo(ui: &mut Ui, salt: &str, terms: &[PaymentTerm], value: &mut Option<u64>, none_label: Option<&str>) -> bool {
    let before = *value;
    let selected = terms
        .iter()
        .find(|x| Some(x.id) == *value)
        .map(|x| tr(&x.name))
        .unwrap_or_else(|| none_label.unwrap_or("").to_string());
    egui::ComboBox::from_id_salt(salt).width(320.0).selected_text(selected).show_ui(ui, |ui| {
        if let Some(label) = none_label {
            ui.selectable_value(value, None, label);
        }
        for x in terms {
            ui.selectable_value(value, Some(x.id), tr(&x.name)).on_hover_text(x.summary());
        }
    });
    *value != before
}

pub fn text_row(ui: &mut Ui, label: &str, value: &mut String, width: f32) {
    ui.label(muted(label));
    ui.add(TextEdit::singleline(value).desired_width(width));
    ui.end_row();
}

// ---- Dashboard pieces ---------------------------------------------------

pub fn kpi(ui: &mut Ui, width: f32, title: &str, value: &str, sub: &str, color: Color32) {
    card_frame().show(ui, |ui| {
        ui.set_width(width - 34.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(4.0, 54.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(2), color);
            ui.vertical(|ui| {
                ui.label(muted(title).size(12.5));
                ui.label(RichText::new(value).size(22.0).strong().color(TEXT));
                ui.label(muted(sub).size(12.0));
            });
        });
    });
}

/// Monthly income and expense bars.
pub fn bar_chart(ui: &mut Ui, values: &[(Cents, Cents); 12], height: f32) {
    let width = ui.available_width();
    let (resp, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
    let rect = resp.rect;
    let left = rect.left() + 52.0;
    let bottom = rect.bottom() - 22.0;
    let top = rect.top() + 8.0;
    let max = values
        .iter()
        .map(|(a, b)| (*a).max(*b))
        .max()
        .unwrap_or(0)
        .max(100_00) as f32;
    // Round the scale up to a tidy number.
    let step = [1_000_00, 2_500_00, 5_000_00, 10_000_00, 25_000_00, 50_000_00, 100_000_00]
        .into_iter()
        .find(|s| (*s as f32) * 4.0 >= max)
        .unwrap_or(250_000_00) as f32;
    let scale_max = step * 4.0;
    for i in 0..=4 {
        let v = step * i as f32;
        let y = bottom - (bottom - top) * v / scale_max;
        painter.line_segment(
            [pos2(left, y), pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgb(0xee, 0xf1, 0xf4)),
        );
        painter.text(
            pos2(left - 8.0, y),
            Align2::RIGHT_CENTER,
            tf!("{} T€", fmt_k(v as i64)),
            FontId::proportional(11.0),
            MUTED,
        );
    }
    let slot = (rect.right() - left) / 12.0;
    let bar_w = (slot * 0.3).min(16.0);
    let hover = resp.hover_pos();
    for (i, (inc, exp)) in values.iter().enumerate() {
        let x0 = left + slot * i as f32;
        let cx = x0 + slot / 2.0;
        let slot_rect = Rect::from_min_max(pos2(x0, top), pos2(x0 + slot, bottom));
        if hover.is_some_and(|p| slot_rect.contains(p)) {
            painter.rect_filled(slot_rect, CornerRadius::same(4), Color32::from_rgb(0xf3, 0xf6, 0xf9));
        }
        for (k, (v, color)) in [(*inc, INCOME), (*exp, EXPENSE)].into_iter().enumerate() {
            let h = (bottom - top) * v as f32 / scale_max;
            let x = cx - bar_w - 1.0 + k as f32 * (bar_w + 2.0);
            let r = Rect::from_min_max(pos2(x, bottom - h), pos2(x + bar_w, bottom));
            painter.rect_filled(r, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, color);
        }
        painter.text(
            pos2(cx, bottom + 11.0),
            Align2::CENTER_CENTER,
            t(MONTHS[i]),
            FontId::proportional(11.5),
            MUTED,
        );
        if hover.is_some_and(|p| slot_rect.contains(p)) {
            let text = format!(
                "{}\n{} {}\n{} {}",
                t(MONTHS[i]),
                t("Einnahmen"),
                fmt_money(*inc),
                t("Ausgaben"),
                fmt_money(*exp)
            );
            let galley = painter.layout(text, FontId::proportional(12.0), TEXT, 200.0);
            let mut pos = pos2(cx + 10.0, top + 4.0);
            if pos.x + galley.size().x + 16.0 > rect.right() {
                pos.x = cx - galley.size().x - 26.0;
            }
            let box_rect = Rect::from_min_size(pos, galley.size() + vec2(16.0, 12.0));
            painter.rect(
                box_rect,
                CornerRadius::same(6),
                Color32::WHITE,
                Stroke::new(1.0, BORDER),
                StrokeKind::Inside,
            );
            painter.galley(pos + vec2(8.0, 6.0), galley, TEXT);
        }
    }
}

fn fmt_k(cents: i64) -> String {
    let k = cents as f64 / 100_000.0;
    if (k - k.round()).abs() < 0.05 {
        format!("{}", k.round() as i64)
    } else {
        format!("{k:.1}").replace('.', ",")
    }
}

pub fn legend(ui: &mut Ui, items: &[(&str, Color32)]) {
    ui.horizontal(|ui| {
        for (label, color) in items {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(2), *color);
            ui.label(muted(*label).size(12.5));
            ui.add_space(8.0);
        }
    });
}

/// The app's mark: a rounded square with a ledger stroke.
pub fn logo(ui: &mut Ui, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same((size * 0.25) as u8), TEAL);
    let s = size;
    let o = r.min;
    for (i, w) in [0.55f32, 0.4, 0.5].iter().enumerate() {
        let y = o.y + s * (0.32 + 0.18 * i as f32);
        p.line_segment(
            [pos2(o.x + s * 0.22, y), pos2(o.x + s * (0.22 + w), y)],
            Stroke::new(s * 0.08, Color32::WHITE),
        );
    }
}

/// A right-aligned label in a box of fixed width (for amounts in grids).
pub fn right_label(ui: &mut Ui, width: f32, text: RichText) {
    ui.allocate_ui_with_layout(
        vec2(width, ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| ui.label(text),
    );
}
