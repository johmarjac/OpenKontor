//! Stock of goods: current quantities, warnings below the minimum, counting (Inventur)
//! and reordering from suppliers.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum StockTab {
    #[default]
    Artikel,
    Lager,
}

#[derive(Default)]
pub struct StockState {
    pub tab: StockTab,
    /// Article being counted: id and the quantity counted.
    pub count: Option<(u64, f64, String)>,
    pub selected: Option<u64>,
}

const STOCK_COLS: [Col; 7] = [
    col("Nr.", 0.8),
    col("Bezeichnung", 2.4),
    rcol("Bestand", 0.9),
    rcol("Mindestbestand", 1.0),
    rcol("Einkaufspreis", 1.0),
    rcol("Lagerwert", 1.0),
    rcol("Status", 1.1),
];

impl App {
    pub(super) fn stock_page(&mut self, ui: &mut Ui) {
        let tracked: Vec<Article> = self.data.articles.iter().filter(|a| a.track_stock).cloned().collect();
        let low: Vec<Article> = tracked.iter().filter(|a| self.data.stock_of(a.id) < a.min_stock).cloned().collect();
        let value: Cents = tracked
            .iter()
            .map(|a| (self.data.stock_of(a.id).max(0.0) * a.purchase_price as f64).round() as Cents)
            .sum();
        let w = (ui.available_width() - 28.0) / 3.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            kpi(ui, w, t("Artikel mit Lager"), &tracked.len().to_string(), t("Lagerbestand wird geführt"), ACCENT);
            kpi(ui, w, t("Lagerwert"), &fmt_money(value), t("zu Einkaufspreisen"), ACCENT);
            kpi(ui, w, t("Nachbestellen"), &low.len().to_string(), t("unter Mindestbestand"), if low.is_empty() { GREEN } else { ORANGE });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(muted(t("Lieferscheine und Rechnungen buchen Ware aus, erhaltene Bestellungen buchen sie ein.")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !low.is_empty() && primary(ui, t("Bestellung für fehlende Ware")).clicked() {
                    let mut doc = self.new_doc(DocKind::Bestellung);
                    doc.lines = low
                        .iter()
                        .map(|a| Line {
                            article_id: Some(a.id),
                            text: a.name.clone(),
                            qty: (a.min_stock * 2.0 - self.data.stock_of(a.id)).max(1.0).ceil(),
                            unit: a.unit.clone(),
                            price_net: a.purchase_price,
                            vat: a.vat,
                            discount_pct: 0.0,
                        })
                        .collect();
                    self.doc_edit = Some(doc);
                    self.section = Section::Belege;
                }
            });
        });
        ui.add_space(8.0);
        let mut clicked = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &STOCK_COLS);
            ScrollArea::vertical().id_salt("stock_list").auto_shrink(false).max_height(260.0).show(ui, |ui| {
                if tracked.is_empty() {
                    empty_hint(ui, t("Noch keine Artikel mit Lagerbestand. Im Artikel „Lagerbestand führen“ anhaken."));
                }
                for a in &tracked {
                    let stock = self.data.stock_of(a.id);
                    let status = if stock < 0.0 {
                        Cell::MutedColored(t("Negativ").into(), RED)
                    } else if stock < a.min_stock {
                        Cell::MutedColored(t("Nachbestellen").into(), ORANGE)
                    } else {
                        Cell::MutedColored(t("Ausreichend").into(), GREEN)
                    };
                    let cells = [
                        Cell::Muted(a.number.clone()),
                        Cell::B(a.name.clone()),
                        Cell::T(format!("{} {}", fmt_qty(stock), a.unit)),
                        Cell::Muted(fmt_qty(a.min_stock)),
                        Cell::Money(a.purchase_price),
                        Cell::Money((stock.max(0.0) * a.purchase_price as f64).round() as Cents),
                        status,
                    ];
                    if table_row(ui, &STOCK_COLS, &cells, self.stock.selected == Some(a.id)).clicked() {
                        clicked = Some(a.id);
                    }
                }
            });
        });
        if let Some(id) = clicked {
            self.stock.selected = Some(id);
        }
        if let Some(id) = self.stock.selected
            && let Some(a) = self.data.articles.iter().find(|a| a.id == id).cloned()
        {
            ui.add_space(10.0);
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    card_title(ui, &tf!("Lagerbewegungen: {}", a.name));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(t("Bestand zählen (Inventur) …")).clicked() {
                            self.stock.count = Some((a.id, self.data.stock_of(a.id), String::new()));
                        }
                    });
                });
                ScrollArea::vertical().id_salt("stock_moves").max_height(200.0).show(ui, |ui| {
                    let moves: Vec<&StockMove> = self.data.stock_moves.iter().filter(|m| m.article_id == a.id).collect();
                    if moves.is_empty() {
                        ui.label(muted(t("Noch keine Bewegungen.")));
                    }
                    let mut open_doc = None;
                    for m in moves.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(muted(fmt_date(m.date)));
                            ui.label(RichText::new(format!("{:+}", m.qty).replace('.', ",")).color(if m.qty >= 0.0 { GREEN } else { RED }).strong());
                            if let Some(did) = m.doc_id {
                                if ui.link(&m.note).clicked() {
                                    open_doc = Some(did);
                                }
                            } else {
                                ui.label(&m.note);
                            }
                        });
                    }
                    if let Some(id) = open_doc {
                        self.open_doc(Some(id));
                    }
                });
            });
        }
    }

    pub(super) fn stock_dialogs(&mut self, ctx: &egui::Context) {
        let Some((id, mut counted, mut note)) = self.stock.count.take() else {
            return;
        };
        let Some(a) = self.data.articles.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let current = self.data.stock_of(id);
        let (mut keep, mut save) = (true, false);
        let modal = egui::Modal::new(Id::unique("stock_count")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new(tf!("Inventur: {}", a.name)).size(18.0).strong());
            ui.add_space(6.0);
            egui::Grid::new("count_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(muted(t("Bestand laut Lager")));
                ui.label(format!("{} {}", fmt_qty(current), a.unit));
                ui.end_row();
                ui.label(muted(t("Tatsächlich gezählt")));
                qty_edit(ui, Id::unique("count_qty"), &mut counted, 90.0);
                ui.end_row();
                ui.label(muted(t("Grund")));
                ui.add(TextEdit::singleline(&mut note).hint_text(t("z. B. Inventur, Bruch, Eigenverbrauch")).desired_width(240.0));
                ui.end_row();
            });
            ui.label(muted(tf!("Korrektur: {}", format!("{:+}", counted - current).replace('.', ","))));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Bestand übernehmen")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save {
            let note = if note.trim().is_empty() { t("Inventur").to_string() } else { note.clone() };
            if self.run(Op::StockCorrection(id, counted - current, note)).is_ok() {
                self.info(t("Bestand korrigiert."));
                keep = false;
            }
        }
        if keep {
            self.stock.count = Some((id, counted, note));
        }
    }
}
