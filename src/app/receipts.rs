//! The incoming-receipts inbox: list, upload (button or drag and drop), checking the
//! recognised details next to the original, booking, and the shared mailbox settings.

use super::*;
use crate::imap::{ImapSecurity, InboxSettings};

/// What is shown next to the details of a receipt.
pub enum Preview {
    Image(egui::TextureHandle),
    Text(String),
}

#[derive(Default)]
pub struct ReceiptsState {
    pub filter: Option<ReceiptStatus>,
    pub search: String,
    pub edit: Option<Receipt>,
    pub preview: Option<(u64, Preview)>,
    pub inbox: Option<(InboxSettings, bool)>,
    pub inbox_error: Option<String>,
    /// Small pictures for the thumbnail view; `None` = no picture (a drawn sheet instead).
    pub thumbs: std::collections::HashMap<u64, Option<egui::TextureHandle>>,
}

const TILE_W: f32 = 196.0;
const TILE_H: f32 = 262.0;
const PICTURE_H: f32 = 170.0;

const COLS: [Col; 7] = [
    col("Eingang", 1.1),
    col("Lieferant", 2.0),
    col("Rechnungs-Nr.", 1.2),
    col("Datum", 0.9),
    col("Erkannt", 1.2),
    col("Status", 0.9),
    rcol("Betrag brutto", 1.0),
];

fn chip_name(s: ReceiptStatus) -> &'static str {
    match s {
        ReceiptStatus::Neu => "Offen",
        ReceiptStatus::Gebucht => "Bezahlt",
        ReceiptStatus::Verworfen => "Storniert",
    }
}

impl App {
    /// Adds files (from the file dialog or dropped onto the window) to the inbox.
    pub(super) fn add_receipt_files(&mut self, files: Vec<(String, Vec<u8>)>) {
        let mut added = 0;
        let mut last = None;
        let mut problems = Vec::new();
        for (name, bytes) in files {
            match self.backend.add_receipt(&mut self.data, &name, &bytes) {
                Ok(id) => {
                    added += 1;
                    last = Some(id);
                }
                Err(e) => problems.push(tr(&e)),
            }
        }
        self.section = Section::Belegeingang;
        if added == 1
            && let Some(id) = last
        {
            self.open_receipt(id);
        }
        if !problems.is_empty() {
            self.error(problems.join("\n"));
        } else if added > 0 {
            self.info(tf!("{} Beleg(e) übernommen.", added));
        }
    }

    /// Takes files dropped onto the window.
    pub(super) fn take_dropped_files(&mut self, ui: &mut Ui) {
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() || self.screen != Screen::Main {
            return;
        }
        let files: Vec<(String, Vec<u8>)> = dropped
            .into_iter()
            .filter_map(|f| {
                let name = f.path().file_name()?.to_string_lossy().into_owned();
                let bytes = f.bytes().ok()?;
                Some((name, bytes))
            })
            .collect();
        if !files.is_empty() {
            self.add_receipt_files(files);
        }
    }

    /// Shows a hint while files are dragged over the window.
    pub(super) fn drop_hint(&self, ui: &mut Ui) {
        if self.screen != Screen::Main || ui.ctx().input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let rect = ui.ctx().content_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("drop")));
        painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_black_alpha(140));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t("Loslassen, um die Dateien in den Belegeingang zu legen"),
            egui::FontId::proportional(24.0),
            Color32::WHITE,
        );
    }

    fn pick_receipt_files(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter(t("Belege"), &["pdf", "xml", "jpg", "jpeg", "png", "tif", "tiff"])
            .pick_files()
        else {
            return;
        };
        let files = paths
            .into_iter()
            .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), std::fs::read(&p).ok()?)))
            .collect();
        self.add_receipt_files(files);
    }

    pub(super) fn open_receipt(&mut self, id: u64) {
        self.receipts.edit = self.data.receipts.iter().find(|r| r.id == id).cloned();
        self.section = Section::Belegeingang;
    }

    pub(super) fn receipts_page(&mut self, ui: &mut Ui) {
        if self.receipts.edit.is_some() {
            self.receipt_detail(ui);
            return;
        }
        let count = |s: ReceiptStatus| self.data.receipts.iter().filter(|r| r.status == s).count();
        let (new, booked, discarded) = (count(ReceiptStatus::Neu), count(ReceiptStatus::Gebucht), count(ReceiptStatus::Verworfen));
        let is_remote = matches!(self.backend, Backend::Remote(_));
        ui.horizontal(|ui| {
            let f = &mut self.receipts.filter;
            ui.selectable_value(f, Some(ReceiptStatus::Neu), tf!("Zu prüfen ({})", new));
            ui.selectable_value(f, Some(ReceiptStatus::Gebucht), tf!("Gebucht ({})", booked));
            ui.selectable_value(f, Some(ReceiptStatus::Verworfen), tf!("Nicht relevant ({})", discarded));
            ui.selectable_value(f, None, t("Alle"));
            ui.separator();
            let before = self.cfg.receipt_tiles;
            ui.selectable_value(&mut self.cfg.receipt_tiles, false, t("Liste")).on_hover_text(t("Belege als Tabelle"));
            ui.selectable_value(&mut self.cfg.receipt_tiles, true, t("Vorschaubilder")).on_hover_text(t("Belege als Kacheln mit Vorschau"));
            if before != self.cfg.receipt_tiles {
                self.cfg.save();
            }
            ui.separator();
            ui.add(TextEdit::singleline(&mut self.receipts.search).hint_text(t("Lieferant, Nummer, Text …")).desired_width(200.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Inbox, t("Dateien hinzufügen …")).clicked() {
                    self.pick_receipt_files();
                }
                if is_remote && ui.button(t("Postfach jetzt abrufen")).clicked() {
                    match self.backend.check_inbox() {
                        Ok(()) => self.info(t("Das Sammelpostfach wird abgerufen – neue Belege erscheinen gleich hier.")),
                        Err(e) => self.error(tr(&e)),
                    }
                }
            });
        });
        ui.add_space(8.0);
        let q = self.receipts.search.to_lowercase();
        let mut list: Vec<&Receipt> = self
            .data
            .receipts
            .iter()
            .filter(|r| self.receipts.filter.is_none_or(|s| r.status == s))
            .filter(|r| {
                q.is_empty()
                    || format!("{} {} {} {}", r.supplier, r.number, r.file_name, r.text).to_lowercase().contains(&q)
            })
            .collect();
        list.reverse();
        let mut open = None;
        if self.cfg.receipt_tiles {
            let list: Vec<Receipt> = list.into_iter().cloned().collect();
            self.receipt_tiles(ui, &list);
            return;
        }
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &COLS);
            ScrollArea::vertical().auto_shrink(false).max_height(ui.available_height() - 60.0).show(ui, |ui| {
                if list.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        icon(ui, Icon::Inbox, 42.0, MUTED);
                        ui.label(RichText::new(t("Keine Belege in dieser Ansicht.")).size(16.0));
                        ui.label(muted(t(
                            "Ziehen Sie Rechnungen (PDF, E-Rechnung als XML, Fotos) einfach in dieses Fenster oder nutzen Sie „Dateien hinzufügen …“.",
                        )));
                    });
                    ui.add_space(30.0);
                }
                for r in &list {
                    let cells = [
                        Cell::Muted(r.received.clone()),
                        if r.supplier.is_empty() { Cell::Muted(r.file_name.clone()) } else { Cell::B(r.supplier.clone()) },
                        Cell::T(r.number.clone()),
                        Cell::T(r.date.map(fmt_date).unwrap_or_default()),
                        if r.recognized_by.is_empty() { Cell::Muted(t("nicht erkannt").into()) } else { Cell::T(tr(&r.recognized_by)) },
                        Cell::Chip(t(chip_name(r.status)).into()),
                        Cell::Money(r.gross),
                    ];
                    if table_row(ui, &COLS, &cells, false).clicked() {
                        open = Some(r.id);
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(muted(t(
                "E-Rechnungen (XRechnung, ZUGFeRD) werden exakt übernommen; PDFs und Fotos per Texterkennung. Bitte die Angaben vor dem Buchen prüfen.",
            )));
        });
        if let Some(id) = open {
            self.open_receipt(id);
        }
    }

    /// The thumbnail view: one tile per receipt with a small picture of the original.
    fn receipt_tiles(&mut self, ui: &mut Ui, list: &[Receipt]) {
        let ctx = ui.ctx().clone();
        // Pictures are made a few at a time so the window stays responsive.
        let mut budget = 2;
        for r in list {
            if budget == 0 {
                ctx.request_repaint();
                break;
            }
            if !self.receipts.thumbs.contains_key(&r.id) {
                let tex = self.make_thumb(&ctx, r);
                self.receipts.thumbs.insert(r.id, tex);
                budget -= 1;
            }
        }
        let mut open = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            if list.is_empty() {
                empty_hint(ui, t("Keine Belege in dieser Ansicht."));
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                for r in list {
                    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), egui::Sense::click());
                    let p = ui.painter();
                    let hovered = resp.hovered();
                    p.rect_filled(rect, CornerRadius::same(10), CARD);
                    p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(if hovered { 2.0 } else { 1.0 }, if hovered { ACCENT } else { BORDER }), egui::StrokeKind::Inside);
                    let pic = egui::Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(TILE_W - 20.0, PICTURE_H));
                    p.rect_filled(pic, CornerRadius::same(6), Color32::from_rgb(0xf5, 0xf7, 0xfa));
                    match self.receipts.thumbs.get(&r.id) {
                        Some(Some(tex)) => {
                            let size = tex.size_vec2();
                            let scale = (pic.width() / size.x).min(pic.height() / size.y);
                            let shown = egui::Rect::from_center_size(pic.center(), size * scale);
                            p.image(tex.id(), shown, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        _ => paint_sheet(p, pic, r),
                    }
                    let mut y = pic.bottom() + 10.0;
                    let title = if r.supplier.is_empty() { r.file_name.clone() } else { r.supplier.clone() };
                    let clip = p.with_clip_rect(rect.shrink(8.0));
                    clip.text(egui::pos2(rect.left() + 12.0, y), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(14.0), TEXT);
                    y += 20.0;
                    let line2 = [r.number.clone(), r.date.map(fmt_date).unwrap_or_default()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                    clip.text(egui::pos2(rect.left() + 12.0, y), egui::Align2::LEFT_TOP, line2, egui::FontId::proportional(12.0), MUTED);
                    y += 22.0;
                    clip.text(egui::pos2(rect.left() + 12.0, y), egui::Align2::LEFT_TOP, fmt_money(r.gross), egui::FontId::proportional(14.0), TEXT);
                    let (bg, fg) = chip_colors(t(chip_name(r.status)));
                    let label = tr(t(chip_name(r.status)));
                    let galley = p.layout_no_wrap(label, egui::FontId::proportional(11.5), fg);
                    let chip = egui::Rect::from_min_size(egui::pos2(rect.right() - 12.0 - galley.size().x - 14.0, y - 1.0), galley.size() + vec2(14.0, 6.0));
                    p.rect_filled(chip, CornerRadius::same(9), bg);
                    p.galley(chip.min + vec2(7.0, 3.0), galley, fg);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        open = Some(r.id);
                    }
                }
            });
        });
        if let Some(id) = open {
            self.open_receipt(id);
        }
    }

    /// A small picture of a receipt: photos directly, PDFs via Poppler when installed.
    fn make_thumb(&self, ctx: &egui::Context, r: &Receipt) -> Option<egui::TextureHandle> {
        let lower = r.file_name.to_lowercase();
        let is_image = [".png", ".jpg", ".jpeg"].iter().any(|x| lower.ends_with(x));
        let is_pdf = lower.ends_with(".pdf");
        if !is_image && !is_pdf {
            return None;
        }
        let bytes = self.backend.receipt_file(r.id).ok()?;
        let png = if is_pdf { crate::ocr::pdf_thumbnail(&bytes)? } else { bytes };
        let img = image::load_from_memory(&png).ok()?.thumbnail(360, 360).to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        Some(ctx.load_texture(format!("thumb-{}", r.id), color, Default::default()))
    }

    fn load_preview(&mut self, ctx: &egui::Context, r: &Receipt) {
        if self.receipts.preview.as_ref().is_some_and(|(id, _)| *id == r.id) {
            return;
        }
        let lower = r.file_name.to_lowercase();
        let is_image = [".png", ".jpg", ".jpeg"].iter().any(|x| lower.ends_with(x));
        let preview = if is_image {
            match self.backend.receipt_file(r.id).and_then(|b| image::load_from_memory(&b).map_err(|e| e.to_string())) {
                Ok(img) => {
                    let img = img.thumbnail(1400, 1400).to_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                    Preview::Image(ctx.load_texture(format!("receipt-{}", r.id), color, Default::default()))
                }
                Err(_) => Preview::Text(r.text.clone()),
            }
        } else {
            Preview::Text(r.text.clone())
        };
        self.receipts.preview = Some((r.id, preview));
    }

    fn receipt_detail(&mut self, ui: &mut Ui) {
        let mut r = self.receipts.edit.take().unwrap();
        self.load_preview(&ui.ctx().clone(), &r);
        let editable = r.status == ReceiptStatus::Neu && self.backend.can_write();
        let mut close = false;
        let mut action: Option<Op> = None;
        let mut save = false;
        let mut book = false;
        let mut again = false;
        let mut show_original = false;
        ui.horizontal(|ui| {
            if ui.button(t("← Zurück")).clicked() {
                close = true;
            }
            ui.label(RichText::new(if r.supplier.is_empty() { r.file_name.clone() } else { r.supplier.clone() }).size(19.0).strong());
            chip(ui, t(chip_name(r.status)));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            match r.status {
                ReceiptStatus::Neu => {
                    if primary_enabled(ui, editable, t("Als Ausgabe buchen")).clicked() {
                        book = true;
                    }
                    if ui.add_enabled(editable, egui::Button::new(t("Speichern"))).clicked() {
                        save = true;
                    }
                    if ui.add_enabled(editable, egui::Button::new(t("Erneut erkennen"))).clicked() {
                        again = true;
                    }
                    if ui.add_enabled(editable, egui::Button::new(t("Nicht relevant"))).clicked() {
                        action = Some(Op::DiscardReceipt(r.id));
                    }
                }
                ReceiptStatus::Gebucht => {
                    if ui.button(t("Buchung zurücknehmen")).clicked() {
                        action = Some(Op::UnbookReceipt(r.id));
                    }
                }
                ReceiptStatus::Verworfen => {
                    if ui.button(t("Wiederherstellen")).clicked() {
                        action = Some(Op::DiscardReceipt(r.id));
                    }
                }
            }
            if icon_button(ui, Icon::Document, t("Original öffnen")).clicked() {
                show_original = true;
            }
            let can_delete = r.status != ReceiptStatus::Gebucht && self.backend.can_write();
            let delete = ui
                .add_enabled(can_delete, egui::Button::new(RichText::new(t("Löschen")).color(RED)))
                .on_hover_text(t("Für versehentlich hinzugefügte Dateien. Gebuchte Belege zuerst zurückbuchen."))
                .on_disabled_hover_text(t("Gebuchte Belege können nicht gelöscht werden. Bitte zuerst die Buchung zurücknehmen."));
            if delete.clicked() {
                self.confirm = Some((
                    tf!(
                        "Beleg „{}“ endgültig löschen? Nur für versehentlich hinzugefügte Dateien – Rechnungen, die aufbewahrt werden müssen, besser als „Nicht relevant“ markieren.",
                        r.file_name
                    ),
                    Op::DeleteReceipt(r.id),
                ));
            }
        });
        ui.add_space(10.0);

        let total_w = ui.available_width();
        let preview_w = (total_w * 0.45).clamp(320.0, 560.0);
        egui::Panel::right("receipt_preview")
            .exact_size(preview_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 16, ..Margin::ZERO }))
            .show(ui, |ui| {
                card(ui, |ui| {
                    ui.set_width(preview_w - 50.0);
                    card_title(ui, t("Original"));
                    ScrollArea::vertical().id_salt("receipt_preview_scroll").auto_shrink(false).show(ui, |ui| {
                        match &self.receipts.preview {
                            Some((_, Preview::Image(tex))) => {
                                let w = ui.available_width();
                                let size = tex.size_vec2();
                                ui.add(egui::Image::new(tex).fit_to_exact_size(vec2(w, w * size.y / size.x.max(1.0))));
                            }
                            Some((_, Preview::Text(text))) if !text.trim().is_empty() => {
                                ui.label(muted(t("Erkannter Text:")));
                                ui.label(RichText::new(text).monospace().size(11.5));
                            }
                            _ => {
                                ui.label(muted(t("Keine Vorschau – „Original öffnen“ zeigt die Datei.")));
                            }
                        }
                    });
                });
            });

        ScrollArea::vertical().id_salt("receipt_form").auto_shrink(false).show(ui, |ui| {
            card(ui, |ui| {
                ui.set_width(ui.available_width() - 34.0);
                card_title(ui, t("Angaben zur Rechnung"));
                if !r.hint.is_empty() {
                    ui.label(RichText::new(tr(&r.hint)).color(ORANGE));
                }
                ui.add_enabled_ui(editable, |ui| {
                    egui::Grid::new("receipt_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                        ui.label(muted(t("Lieferant")));
                        ui.horizontal(|ui| {
                            ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut r.supplier));
                            let mut chosen = r.contact_id;
                            egui::ComboBox::from_id_salt("receipt_contact")
                                .selected_text(t("Kontakt …"))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    let mut contacts: Vec<&Contact> = self.data.contacts.iter().collect();
                                    contacts.sort_by_key(|c| (c.kind != ContactKind::Lieferant, c.display_name().to_lowercase()));
                                    for c in contacts {
                                        ui.selectable_value(&mut chosen, Some(c.id), c.display_name());
                                    }
                                });
                            if chosen != r.contact_id
                                && let Some(c) = self.data.contact(chosen)
                            {
                                r.contact_id = chosen;
                                r.supplier = c.display_name();
                            }
                        });
                        ui.end_row();
                        ui.label(muted(t("Rechnungs-Nr.")));
                        ui.add_sized([260.0, 26.0], TextEdit::singleline(&mut r.number));
                        ui.end_row();
                        ui.label(muted(t("Rechnungsdatum")));
                        let mut d = r.date.unwrap_or(self.today);
                        if date_edit(ui, Id::unique("receipt_date"), &mut d) || r.date.is_none() && editable {
                            r.date = Some(d);
                        }
                        ui.end_row();
                        ui.label(muted(t("Fällig am")));
                        ui.horizontal(|ui| {
                            let mut due = r.due.unwrap_or(d);
                            if date_edit(ui, Id::unique("receipt_due"), &mut due) {
                                r.due = Some(due);
                            }
                            if r.due.is_none() {
                                ui.label(muted(t("(nicht angegeben)")));
                            }
                        });
                        ui.end_row();
                        ui.label(muted(t("Betrag brutto")));
                        ui.horizontal(|ui| {
                            money_edit(ui, Id::unique("receipt_gross"), &mut r.gross, 110.0);
                            ui.label("€");
                        });
                        ui.end_row();
                        ui.label(muted(t("Umsatzsteuer")));
                        if r.vat.len() > 1 {
                            ui.vertical(|ui| {
                                for p in &r.vat {
                                    ui.label(tf!("{} %: netto {} · Steuer {}", p.rate, fmt_money(p.net), fmt_money(p.tax)));
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                vat_combo(ui, "receipt_vat", &mut r.vat_rate);
                                let net = (r.gross as f64 * 100.0 / (100.0 + r.vat_rate as f64)).round() as Cents;
                                ui.label(muted(tf!("netto {} · Vorsteuer {}", fmt_money(net), fmt_money(r.gross - net))));
                            });
                        }
                        ui.end_row();
                        ui.label(muted(t("Kategorie / Konto")));
                        egui::ComboBox::from_id_salt("receipt_cat")
                            .width(300.0)
                            .selected_text(if r.category.is_empty() { t("Bitte wählen …").to_string() } else { tr(&r.category) })
                            .show_ui(ui, |ui| {
                                for c in EXPENSE_CATEGORIES {
                                    ui.selectable_value(&mut r.category, c.to_string(), t(c));
                                }
                            });
                        ui.end_row();
                        ui.label(muted(t("Zahlungsweg")));
                        ui.horizontal(|ui| {
                            for p in ["Bank", "Kasse", "Karte", "PayPal"] {
                                ui.selectable_value(&mut r.payment, p.to_string(), t(p));
                            }
                        });
                        ui.end_row();
                        ui.label(muted(t("Notiz")));
                        ui.add(TextEdit::multiline(&mut r.note).desired_rows(2).desired_width(300.0));
                        ui.end_row();
                    });
                });
            });
            ui.add_space(12.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width() - 34.0);
                card_title(ui, t("Herkunft"));
                egui::Grid::new("receipt_info").num_columns(2).spacing([14.0, 5.0]).show(ui, |ui| {
                    let mut row = |k: &str, v: String| {
                        if !v.is_empty() {
                            ui.label(muted(t_dyn(k)));
                            ui.label(v);
                            ui.end_row();
                        }
                    };
                    row("Erkannt durch", if r.recognized_by.is_empty() { t("nicht erkannt").into() } else { tr(&r.recognized_by) });
                    row("Eingang", format!("{} · {}", r.received, tr(&r.source)));
                    row("Datei", format!("{} ({} KB)", r.file_name, r.size.div_ceil(1024)));
                    row("Prüfsumme (SHA-256)", r.sha256.chars().take(24).collect::<String>() + "…");
                    row("IBAN", r.iban.clone());
                    row("USt-IdNr.", r.vat_id.clone());
                });
                if r.status == ReceiptStatus::Gebucht {
                    ui.add_space(4.0);
                    for b in self.data.bookings.iter().filter(|b| r.booking_ids.contains(&b.id)) {
                        ui.label(RichText::new(tf!("Gebucht: {} · {} · {}", fmt_date(b.date), tr(&b.category), fmt_money(b.gross))).color(GREEN));
                    }
                }
                ui.label(muted(t("Die Originaldatei wird unverändert aufbewahrt (GoBD).")));
            });
        });

        // Apply what was clicked.
        if save || book {
            match self.run(Op::SaveReceipt(Box::new(r.clone()))) {
                Ok(_) if book => {
                    if self.run(Op::BookReceipt(r.id)).is_ok() {
                        self.info(t("Beleg als Ausgabe gebucht."));
                        close = true;
                    }
                }
                Ok(_) => self.info(t("Gespeichert.")),
                Err(()) => {}
            }
        }
        if let Some(op) = action
            && self.run(op).is_ok()
        {
            close = true;
        }
        if again {
            match self.backend.recognize_receipt(&mut self.data, r.id) {
                Ok(()) => {
                    self.info(t("Beleg neu erkannt."));
                    self.receipts.preview = None;
                }
                Err(e) => self.error(tr(&e)),
            }
        }
        if show_original {
            match self.backend.receipt_file(r.id) {
                Ok(bytes) => {
                    let path = std::env::temp_dir().join(format!("OpenKontor Beleg {} {}", r.id, r.file_name));
                    match std::fs::write(&path, bytes) {
                        Ok(()) => export::open_file(&path),
                        Err(e) => self.error(e.to_string()),
                    }
                }
                Err(e) => self.error(tr(&e)),
            }
        }
        if close {
            self.receipts.preview = None;
            return;
        }
        // Show the stored state after actions that changed it.
        let fresh = (save || again).then(|| self.data.receipts.iter().find(|x| x.id == r.id).cloned()).flatten();
        self.receipts.edit = Some(fresh.unwrap_or(r));
    }

    // ---- Shared mailbox settings ---------------------------------------------------------

    pub(super) fn settings_inbox(&mut self, ui: &mut Ui) {
        let Backend::Remote(_) = &self.backend else {
            card(ui, |ui| {
                ui.set_width(ui.available_width().min(840.0));
                card_title(ui, t("Sammelpostfach für Belege"));
                ui.label(muted(t("Das Sammelpostfach gibt es nur mit einem OpenKontor-Server.")));
            });
            return;
        };
        if !self.backend.is_admin() {
            return;
        }
        if self.receipts.inbox.is_none() && self.receipts.inbox_error.is_none() {
            match self.backend.inbox_settings() {
                Ok(s) => self.receipts.inbox = Some(s),
                Err(e) => self.receipts.inbox_error = Some(tr(&e)),
            }
        }
        let mut save = false;
        let mut check = false;
        let mut reload = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Sammelpostfach für Belege"));
            ui.label(muted(t(
                "Der Server ruft dieses Postfach regelmäßig ab. Anhänge (PDF, E-Rechnung, Fotos) ungelesener E-Mails landen im Belegeingang; die E-Mails werden als gelesen markiert.",
            )));
            if let Some(e) = &self.receipts.inbox_error {
                ui.label(RichText::new(e).color(RED));
            }
            let Some((s, password_set)) = &mut self.receipts.inbox else { return };
            ui.checkbox(&mut s.enabled, t("Sammelpostfach regelmäßig abrufen"));
            egui::Grid::new("imap").num_columns(4).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(muted(t("Postfach-Server (IMAP)")));
                ui.add_sized([240.0, 26.0], TextEdit::singleline(&mut s.host).hint_text("imap.example.de"));
                ui.label(muted(t("Port")));
                ui.add(egui::DragValue::new(&mut s.port).range(1..=65535));
                ui.end_row();
                ui.label(muted(t("Verschlüsselung")));
                egui::ComboBox::from_id_salt("imap_sec").selected_text(t(s.security.name())).show_ui(ui, |ui| {
                    for sec in ImapSecurity::ALL {
                        ui.selectable_value(&mut s.security, sec, t(sec.name()));
                    }
                });
                ui.label(muted(t("Ordner")));
                ui.add_sized([120.0, 26.0], TextEdit::singleline(&mut s.folder));
                ui.end_row();
                ui.label(muted(t("Benutzername")));
                ui.add_sized([240.0, 26.0], TextEdit::singleline(&mut s.username).hint_text("belege@firma.de"));
                ui.label(muted(t("Passwort")));
                ui.add_sized(
                    [180.0, 26.0],
                    TextEdit::singleline(&mut s.password)
                        .password(true)
                        .hint_text(if *password_set { t("gespeichert – leer lassen") } else { "" }),
                );
                ui.end_row();
                ui.label(muted(t("Abruf alle")));
                ui.add(egui::DragValue::new(&mut s.interval_minutes).range(1..=1440).suffix(t(" Minuten")));
                ui.end_row();
            });
            ui.label(muted(t("Microsoft 365: outlook.office365.com · Google: imap.gmail.com (mit App-Passwort) · IONOS: imap.ionos.de")));
            super::settings_more::microsoft_app_editor(ui, "inbox_ms", &mut s.microsoft, *password_set, "IMAP.AccessAsApp");
            if !s.last_check.is_empty() {
                ui.label(muted(tf!("Letzter Abruf: {} – {}", s.last_check, tr(&s.last_result))));
            }
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Jetzt abrufen")).clicked() {
                    check = true;
                }
                if ui.button(t("Aktualisieren")).clicked() {
                    reload = true;
                }
            });
        });
        if save && let Some((s, _)) = &self.receipts.inbox {
            match self.backend.save_inbox_settings(s) {
                Ok(()) => {
                    self.info(t("Sammelpostfach gespeichert."));
                    reload = true;
                }
                Err(e) => self.error(tr(&e)),
            }
        }
        if check {
            match self.backend.check_inbox() {
                Ok(()) => self.info(t("Das Sammelpostfach wird abgerufen – neue Belege erscheinen gleich im Belegeingang.")),
                Err(e) => self.error(tr(&e)),
            }
        }
        if reload {
            self.receipts.inbox = None;
            self.receipts.inbox_error = None;
        }
    }
}

/// Labels built from string slices that are already German keys.
fn t_dyn(key: &str) -> String {
    tr(key)
}

/// A drawn sheet for receipts without a picture: kind of file and the first lines of text.
fn paint_sheet(p: &egui::Painter, area: egui::Rect, r: &Receipt) {
    let sheet = egui::Rect::from_center_size(area.center(), vec2(area.height() * 0.72, area.height() - 16.0));
    p.rect_filled(sheet, CornerRadius::same(3), Color32::WHITE);
    p.rect_stroke(sheet, CornerRadius::same(3), Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    let lower = r.file_name.to_lowercase();
    let kind = if lower.ends_with(".xml") || r.recognized_by.contains("XRechnung") || r.recognized_by.contains("ZUGFeRD") {
        "E-Rechnung"
    } else if lower.ends_with(".pdf") {
        "PDF"
    } else {
        "Datei"
    };
    let badge = egui::Rect::from_min_size(sheet.min + vec2(6.0, 6.0), vec2(sheet.width() - 12.0, 16.0));
    p.rect_filled(badge, CornerRadius::same(3), if kind == "E-Rechnung" { TEAL } else { ACCENT });
    p.text(badge.center(), egui::Align2::CENTER_CENTER, t(kind), egui::FontId::proportional(10.0), Color32::WHITE);
    let clip = p.with_clip_rect(sheet.shrink(5.0));
    let mut y = badge.bottom() + 6.0;
    for line in r.text.lines().map(str::trim).filter(|l| !l.is_empty()).take(11) {
        clip.text(egui::pos2(sheet.left() + 6.0, y), egui::Align2::LEFT_TOP, line, egui::FontId::proportional(7.5), MUTED);
        y += 10.0;
    }
}
