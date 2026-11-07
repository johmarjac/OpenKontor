//! Bank statements: read CAMT or CSV files from online banking and match each payment
//! with an open invoice, an incoming receipt or a new booking.

use super::*;
use crate::bank::{Suggestion, suggestions};

pub struct BankState {
    pub filter: Option<BankStatus>,
    pub selected: Option<u64>,
    pub category: String,
    pub vat: u8,
    pub text: String,
    pub search: String,
}

impl Default for BankState {
    fn default() -> Self {
        Self {
            filter: Some(BankStatus::Offen),
            selected: None,
            category: String::new(),
            vat: 19,
            text: String::new(),
            search: String::new(),
        }
    }
}

const BANK_COLS: [Col; 5] = [col("Datum", 0.8), col("Name", 1.8), col("Verwendungszweck", 3.0), rcol("Betrag", 1.0), rcol("Status", 0.9)];

impl App {
    pub(super) fn bank_dialogs(&mut self, _ctx: &egui::Context) {}

    fn import_bank_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("Kontoauszug (CAMT, CSV)"), &["xml", "csv", "txt", "zip"])
            .pick_file()
        else {
            return;
        };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return self.error(e.to_string()),
        };
        match crate::bank::parse(&name, &bytes) {
            Ok(lines) => {
                let n = lines.len();
                if self.run(Op::ImportBank(lines)).is_ok() {
                    let last = self.data.log.last().map(|l| l.action.clone()).unwrap_or_default();
                    self.info(if last.is_empty() { tf!("{} Umsätze gelesen.", n) } else { tr(&last) });
                    self.bank.filter = Some(BankStatus::Offen);
                }
            }
            Err(e) => self.error(tr(&e)),
        }
    }

    pub(super) fn bank_page(&mut self, ui: &mut Ui) {
        let open: Vec<&BankTx> = self.data.bank.iter().filter(|t| t.status == BankStatus::Offen).collect();
        let sure: Vec<(u64, BankAssign)> = open
            .iter()
            .filter_map(|tx| suggestions(&self.data, tx).into_iter().next().filter(|s| s.score >= 100).map(|s| (tx.id, s.assign)))
            .collect();
        let (inflow, outflow): (Cents, Cents) = open.iter().fold((0, 0), |(i, o), t| if t.amount >= 0 { (i + t.amount, o) } else { (i, o - t.amount) });
        let w = (ui.available_width() - 28.0) / 3.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            kpi(ui, w, t("Offene Umsätze"), &open.len().to_string(), &tf!("{} sicher erkannt", sure.len()), if open.is_empty() { GREEN } else { ORANGE });
            kpi(ui, w, t("Eingänge offen"), &fmt_money(inflow), t("noch keiner Rechnung zugeordnet"), INCOME);
            kpi(ui, w, t("Ausgänge offen"), &fmt_money(outflow), t("noch keinem Beleg zugeordnet"), EXPENSE);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.bank.filter, Some(BankStatus::Offen), t("Offen"));
            ui.selectable_value(&mut self.bank.filter, Some(BankStatus::Zugeordnet), t("Zugeordnet"));
            ui.selectable_value(&mut self.bank.filter, Some(BankStatus::Ignoriert), t("Ignoriert"));
            ui.selectable_value(&mut self.bank.filter, None, t("Alle"));
            ui.add(TextEdit::singleline(&mut self.bank.search).hint_text(t("Name, Zweck, Betrag …")).desired_width(200.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Download, t("Kontoauszug einlesen …"))
                    .on_hover_text(t("CAMT-Datei (XML) oder CSV aus dem Online-Banking"))
                    .clicked()
                {
                    self.import_bank_file();
                }
                if !sure.is_empty() && ui.button(tf!("{} sichere Zuordnungen übernehmen", sure.len())).clicked() {
                    let mut done = 0;
                    for (id, how) in sure.clone() {
                        if self.run(Op::AssignBank(id, how)).is_ok() {
                            done += 1;
                        }
                    }
                    self.info(tf!("{} Umsätze zugeordnet.", done));
                }
            });
        });
        ui.add_space(8.0);
        let q = self.bank.search.to_lowercase();
        let list: Vec<BankTx> = self
            .data
            .bank
            .iter()
            .rev()
            .filter(|t| self.bank.filter.is_none_or(|f| t.status == f))
            .filter(|t| q.is_empty() || format!("{} {} {}", t.name, t.purpose, fmt_amount(t.amount)).to_lowercase().contains(&q))
            .cloned()
            .collect();
        let detail_w = (ui.available_width() * 0.40).clamp(340.0, 480.0);
        let mut pick = None;
        let selected = self.bank.selected.and_then(|id| self.data.bank.iter().find(|t| t.id == id).cloned());
        if let Some(tx) = selected {
            egui::Panel::right("bank_detail")
                .exact_size(detail_w)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().inner_margin(Margin { left: 14, ..Margin::ZERO }))
                .show(ui, |ui| {
                    ScrollArea::vertical().id_salt("bank_detail_scroll").show(ui, |ui| self.bank_detail(ui, &tx, detail_w - 48.0));
                });
        }
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &BANK_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if list.is_empty() {
                    empty_hint(
                        ui,
                        if self.data.bank.is_empty() {
                            t("Noch keine Kontoauszüge. Laden Sie im Online-Banking die Umsätze als CAMT (XML) oder CSV herunter und lesen Sie die Datei hier ein.")
                        } else {
                            t("Keine Umsätze in dieser Ansicht.")
                        },
                    );
                }
                for tx in &list {
                    let cells = [
                        Cell::T(fmt_date(tx.date)),
                        Cell::B(tx.name.clone()),
                        Cell::Muted(tx.purpose.clone()),
                        Cell::MoneyColored(tx.amount, if tx.amount >= 0 { GREEN } else { RED }),
                        Cell::Muted(t(tx.status.name()).into()),
                    ];
                    if table_row(ui, &BANK_COLS, &cells, self.bank.selected == Some(tx.id)).clicked() {
                        pick = Some(tx.id);
                    }
                }
            });
        });
        if let Some(id) = pick {
            self.bank.selected = Some(id);
            self.bank.text.clear();
            let tx = self.data.bank.iter().find(|t| t.id == id);
            self.bank.category = if tx.is_some_and(|t| t.amount >= 0) { INCOME_CATEGORIES[0].into() } else { EXPENSE_CATEGORIES[9].into() };
        }
    }

    fn bank_detail(&mut self, ui: &mut Ui, tx: &BankTx, width: f32) {
        let mut action: Option<Op> = None;
        let mut open_doc = None;
        let mut open_receipt = None;
        card(ui, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                card_title(ui, &tx.name);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::icons::icon_only_button(ui, Icon::Close, t("Schließen")).clicked() {
                        self.bank.selected = None;
                    }
                });
            });
            ui.label(RichText::new(fmt_money(tx.amount)).size(20.0).strong().color(if tx.amount >= 0 { GREEN } else { RED }));
            ui.label(muted(tf!("gebucht am {}", fmt_date(tx.date))));
            if !tx.iban.is_empty() {
                ui.label(muted(format!("IBAN {}", tx.iban)));
            }
            ui.add_space(4.0);
            ui.label(&tx.purpose);
            ui.add_space(8.0);
            match (tx.status, tx.link) {
                (BankStatus::Zugeordnet, Some(link)) => {
                    match link {
                        BankLink::Document(id) => {
                            let label = self.data.documents.iter().find(|d| d.id == id).map(|d| format!("{} {}", d.kind.label(), d.number)).unwrap_or_default();
                            if ui.link(tf!("Zugeordnet: {}", label)).clicked() {
                                open_doc = Some(id);
                            }
                        }
                        BankLink::Receipt(id) => {
                            let label = self.data.receipts.iter().find(|r| r.id == id).map(|r| r.file_name.clone()).unwrap_or_default();
                            if ui.link(tf!("Zugeordnet: Beleg „{}“", label)).clicked() {
                                open_receipt = Some(id);
                            }
                        }
                        BankLink::Booking(id) => {
                            let label = self.data.bookings.iter().find(|b| b.id == id).map(|b| tr(&b.category)).unwrap_or_default();
                            ui.label(RichText::new(tf!("Gebucht als „{}“", label)).color(GREEN));
                        }
                    }
                    ui.add_space(6.0);
                    if ui.button(t("Zuordnung zurücknehmen")).clicked() {
                        action = Some(Op::UnassignBank(tx.id));
                    }
                }
                (BankStatus::Ignoriert, _) => {
                    ui.label(muted(t("Dieser Umsatz wird nicht gebucht (z. B. Umbuchung zwischen eigenen Konten).")));
                    if ui.button(t("Wieder als offen führen")).clicked() {
                        action = Some(Op::IgnoreBank(tx.id));
                    }
                }
                _ => {
                    let sugg: Vec<Suggestion> = suggestions(&self.data, tx);
                    ui.label(RichText::new(t("Vorschläge")).strong());
                    if sugg.is_empty() {
                        ui.label(muted(t("Keine passende offene Rechnung oder kein passender Beleg gefunden.")));
                    }
                    for s in sugg {
                        ui.horizontal(|ui| {
                            let color = if s.score >= 90 { GREEN } else if s.score >= 60 { ACCENT } else { MUTED };
                            ui.label(RichText::new(format!("{} %", s.score)).color(color).strong());
                            ui.label(&s.label);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button(t("Zuordnen")).clicked() {
                                    action = Some(Op::AssignBank(tx.id, s.assign.clone()));
                                }
                            });
                        });
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(RichText::new(t("Andere Rechnung")).strong());
                    let small = self.data.company.small_business;
                    let mut chosen: Option<u64> = None;
                    egui::ComboBox::from_id_salt("bank_doc").width(width - 20.0).selected_text(t("Offene Rechnung wählen …")).show_ui(ui, |ui| {
                        for d in self.data.documents.iter().filter(|d| d.kind.is_billing() && d.status == DocStatus::Offen && d.open_amount(small).signum() == tx.amount.signum()) {
                            let label = format!("{} – {} ({})", d.number, self.data.contact_name(d.contact_id), fmt_money(d.open_amount(small)));
                            if ui.selectable_label(false, label).clicked() {
                                chosen = Some(d.id);
                            }
                        }
                    });
                    if let Some(id) = chosen {
                        action = Some(Op::AssignBank(tx.id, BankAssign::Document { id, discount: false }));
                    }
                    if tx.amount < 0 {
                        let mut chosen: Option<u64> = None;
                        egui::ComboBox::from_id_salt("bank_receipt").width(width - 20.0).selected_text(t("Eingangsbeleg wählen …")).show_ui(ui, |ui| {
                            for r in self.data.receipts.iter().rev().filter(|r| r.status != ReceiptStatus::Verworfen && !self.data.bank.iter().any(|x| x.link == Some(BankLink::Receipt(r.id)))) {
                                let label = format!("{} {} ({})", r.supplier, r.number, fmt_money(r.gross));
                                if ui.selectable_label(false, label).clicked() {
                                    chosen = Some(r.id);
                                }
                            }
                        });
                        if let Some(id) = chosen {
                            action = Some(Op::AssignBank(tx.id, BankAssign::Receipt(id)));
                        }
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(RichText::new(t("Ohne Beleg buchen")).strong());
                    let cats: &[&str] = if tx.amount >= 0 { &INCOME_CATEGORIES } else { &EXPENSE_CATEGORIES };
                    egui::ComboBox::from_id_salt("bank_cat").width(width - 20.0).selected_text(tr(&self.bank.category)).show_ui(ui, |ui| {
                        for c in cats {
                            ui.selectable_value(&mut self.bank.category, c.to_string(), t(c));
                        }
                    });
                    ui.horizontal(|ui| {
                        vat_combo(ui, "bank_vat", &mut self.bank.vat);
                        ui.add(TextEdit::singleline(&mut self.bank.text).hint_text(t("Buchungstext (sonst Verwendungszweck)")).desired_width(width - 110.0));
                    });
                    ui.horizontal(|ui| {
                        if primary(ui, t("Buchen")).clicked() {
                            action = Some(Op::AssignBank(
                                tx.id,
                                BankAssign::Booking { category: self.bank.category.clone(), vat: self.bank.vat, text: self.bank.text.clone() },
                            ));
                        }
                        if ui.button(t("Ignorieren")).on_hover_text(t("z. B. Umbuchung zwischen eigenen Konten")).clicked() {
                            action = Some(Op::IgnoreBank(tx.id));
                        }
                    });
                }
            }
        });
        if let Some(op) = action
            && self.run(op).is_ok()
        {
            self.info(t("Erledigt."));
            // Go on with the next open line.
            if self.bank.filter == Some(BankStatus::Offen) {
                self.bank.selected = self.data.bank.iter().rev().find(|t| t.status == BankStatus::Offen).map(|t| t.id);
            }
        }
        if let Some(id) = open_doc {
            self.open_doc(Some(id));
        }
        if let Some(id) = open_receipt {
            self.open_receipt(id);
        }
    }
}
