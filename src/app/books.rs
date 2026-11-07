//! Bookkeeping beyond income and expenses: cash book with daily closing, the asset
//! register with write-offs, and manual entries for the double-entry bookkeeping.

use super::*;
use crate::accounting;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BooksTab {
    #[default]
    Buchungen,
    Kasse,
    Anlagen,
    Umbuchungen,
}

pub struct BooksState {
    pub tab: BooksTab,
    pub asset_edit: Option<Asset>,
    pub journal_edit: Option<JournalEntry>,
    pub asset_year: i32,
    pub close_day: NaiveDate,
    pub counted: Cents,
    pub count_cash: bool,
}

impl Default for BooksState {
    fn default() -> Self {
        Self {
            tab: BooksTab::Buchungen,
            asset_edit: None,
            journal_edit: None,
            asset_year: today().year(),
            close_day: today(),
            counted: 0,
            count_cash: false,
        }
    }
}

const CASH_COLS: [Col; 6] = [
    col("Datum", 0.9),
    col("Buchungstext", 2.6),
    col("Beleg", 1.0),
    rcol("Einnahme", 1.0),
    rcol("Ausgabe", 1.0),
    rcol("Bestand", 1.1),
];

const ASSET_COLS: [Col; 9] = [
    col("Nr.", 0.8),
    col("Bezeichnung", 2.0),
    col("Konto", 1.6),
    col("Anschaffung", 0.9),
    rcol("Kosten", 1.0),
    rcol("Jahre", 0.5),
    rcol("Wert 1.1.", 1.0),
    rcol("AfA", 0.9),
    rcol("Wert 31.12.", 1.0),
];

const JOURNAL_COLS: [Col; 5] = [col("Datum", 0.9), col("Soll", 1.6), col("Haben", 1.6), rcol("Betrag", 1.0), col("Buchungstext", 2.4)];

impl App {
    pub(super) fn books_page(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.books.tab, BooksTab::Buchungen, t("Buchungen"));
            ui.selectable_value(&mut self.books.tab, BooksTab::Kasse, t("Kassenbuch"));
            ui.selectable_value(&mut self.books.tab, BooksTab::Anlagen, t("Anlagen"));
            ui.selectable_value(&mut self.books.tab, BooksTab::Umbuchungen, t("Umbuchungen"));
        });
        ui.add_space(6.0);
        match self.books.tab {
            BooksTab::Buchungen => self.bookings(ui),
            BooksTab::Kasse => self.cash_book(ui),
            BooksTab::Anlagen => self.assets(ui),
            BooksTab::Umbuchungen => self.journal(ui),
        }
    }

    pub(super) fn books_dialogs(&mut self, ctx: &egui::Context) {
        if self.books.asset_edit.is_some() {
            self.asset_dialog(ctx);
        }
        if self.books.journal_edit.is_some() {
            self.journal_dialog(ctx);
        }
    }

    fn cash_book(&mut self, ui: &mut Ui) {
        let book: Vec<(Booking, Cents)> = self.data.cash_book().into_iter().map(|(b, bal)| (b.clone(), bal)).collect();
        let closed = self.data.cash_closings.last().cloned();
        let balance = self.data.cash_balance(self.today);
        let negative = book.iter().find(|(_, bal)| *bal < 0).map(|(b, bal)| (b.date, *bal));
        let w = (ui.available_width() - 28.0) / 3.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            kpi(ui, w, t("Kassenbestand heute"), &fmt_money(balance), &tf!("Anfangsbestand {}", fmt_money(self.data.company.cash_opening)), if balance >= 0 { GREEN } else { RED });
            kpi(
                ui,
                w,
                t("Abgeschlossen bis"),
                &closed.as_ref().map(|c| fmt_date(c.date)).unwrap_or_else(|| "–".into()),
                &closed.as_ref().map(|c| tf!("Bestand {}", fmt_money(c.balance))).unwrap_or_else(|| t("noch kein Abschluss").into()),
                ACCENT,
            );
            kpi(ui, w, t("Kassenbuchungen"), &book.len().to_string(), t("Zahlungsweg „Kasse“"), ACCENT);
        });
        ui.add_space(10.0);
        if let Some((d, bal)) = negative {
            ui.label(RichText::new(tf!("Achtung: Am {} wäre der Kassenbestand negativ ({}). Eine Kasse kann nie unter null fallen – bitte Buchungen prüfen.", fmt_date(d), fmt_money(bal))).color(RED));
            ui.add_space(6.0);
        }
        ui.horizontal(|ui| {
            if primary(ui, t("+ Barausgabe")).clicked() {
                self.booking_edit = Some(Booking { date: self.today, payment: CASH.into(), ..Booking::default() });
            }
            if ui.button(t("+ Bareinnahme")).clicked() {
                self.booking_edit = Some(Booking {
                    date: self.today,
                    kind: BookingKind::Einnahme,
                    category: INCOME_CATEGORIES[0].into(),
                    payment: CASH.into(),
                    ..Booking::default()
                });
            }
            ui.separator();
            ui.label(muted(t("Tagesabschluss bis")));
            date_edit(ui, Id::unique("cash_close_day"), &mut self.books.close_day);
            ui.checkbox(&mut self.books.count_cash, t("gezählt:"));
            if self.books.count_cash {
                money_edit(ui, Id::unique("cash_counted"), &mut self.books.counted, 90.0);
            }
            if ui.button(t("Kasse abschließen")).on_hover_text(t("Danach können Kassenbuchungen bis zu diesem Tag nicht mehr geändert werden (GoBD).")).clicked() {
                let counted = self.books.count_cash.then_some(self.books.counted);
                self.confirm = Some((
                    tf!("Kasse bis {} abschließen? Kassenbuchungen bis zu diesem Tag sind danach gesperrt.", fmt_date(self.books.close_day)),
                    Op::CloseCash(self.books.close_day, counted),
                ));
            }
        });
        ui.add_space(8.0);
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &CASH_COLS);
            ScrollArea::vertical().auto_shrink(false).max_height(ui.available_height() - 30.0).show(ui, |ui| {
                if book.is_empty() {
                    empty_hint(ui, t("Noch keine Bargeldbuchungen. Wählen Sie bei einer Buchung den Zahlungsweg „Kasse“."));
                }
                for (b, bal) in book.iter().rev() {
                    let locked = closed.as_ref().is_some_and(|c| b.date <= c.date);
                    let text = if locked { tf!("{} (abgeschlossen)", b.text) } else { b.text.clone() };
                    let cells = [
                        Cell::T(fmt_date(b.date)),
                        Cell::T(text),
                        Cell::Muted(b.receipt.clone()),
                        if b.kind == BookingKind::Einnahme { Cell::MoneyColored(b.gross, GREEN) } else { Cell::Muted(String::new()) },
                        if b.kind == BookingKind::Ausgabe { Cell::MoneyColored(b.gross, RED) } else { Cell::Muted(String::new()) },
                        Cell::MoneyColored(*bal, if *bal < 0 { RED } else { TEXT }),
                    ];
                    if table_row(ui, &CASH_COLS, &cells, false).clicked() {
                        open = Some(b.clone());
                    }
                }
            });
            if !self.data.cash_closings.is_empty() {
                ui.label(muted(tf!(
                    "{} Abschlüsse, zuletzt {}",
                    self.data.cash_closings.len(),
                    closed.as_ref().map(|c| format!("{} ({})", fmt_date(c.date), c.time)).unwrap_or_default()
                )));
            }
        });
        if open.is_some() {
            self.booking_edit = open;
        }
    }

    fn assets(&mut self, ui: &mut Ui) {
        let year = self.books.asset_year;
        let years: Vec<i32> = {
            let mut y: Vec<i32> = self.years();
            y.extend(self.data.assets.iter().map(|a| a.date.year()));
            y.sort_unstable();
            y.dedup();
            y.reverse();
            y
        };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("asset_year").selected_text(year.to_string()).width(80.0).show_ui(ui, |ui| {
                for y in &years {
                    ui.selectable_value(&mut self.books.asset_year, *y, y.to_string());
                }
            });
            ui.label(muted(t("Anlagenverzeichnis mit Abschreibung (AfA) des Jahres")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary(ui, t("+ Anlagegut")).clicked() {
                    self.books.asset_edit = Some(Asset::default());
                }
            });
        });
        ui.add_space(8.0);
        let list: Vec<Asset> = self.data.assets.iter().filter(|a| a.date.year() <= year).cloned().collect();
        let (mut start, mut afa, mut end) = (0, 0, 0);
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &ASSET_COLS);
            ScrollArea::vertical().auto_shrink(false).max_height(ui.available_height() - 34.0).show(ui, |ui| {
                if list.is_empty() {
                    empty_hint(ui, t("Noch keine Anlagegüter. Größere Anschaffungen (z. B. Computer, Maschinen, Fahrzeuge) werden hier über die Jahre abgeschrieben."));
                }
                for a in &list {
                    let (s, f, e) = (accounting::value_at_start(a, year), accounting::afa_for_year(a, year), accounting::value_at_end(a, year));
                    start += s;
                    afa += f;
                    end += e;
                    let name = if a.disposed.is_some_and(|d| d.year() <= year) { tf!("{} (Abgang)", a.name) } else { a.name.clone() };
                    let cells = [
                        Cell::Muted(a.number.clone()),
                        Cell::B(name),
                        Cell::Muted(tr(&a.account)),
                        Cell::T(fmt_date(a.date)),
                        Cell::Money(a.cost),
                        Cell::T(if a.method == AfaMethod::Linear { a.years.to_string() } else { "–".into() }),
                        Cell::Money(s),
                        Cell::MoneyColored(f, EXPENSE),
                        Cell::Money(e),
                    ];
                    if table_row(ui, &ASSET_COLS, &cells, false).clicked() {
                        open = Some(a.clone());
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tf!("Summe {}: Wert am 1.1. {} · Abschreibung {} · Wert am 31.12. {}", year, fmt_money(start), fmt_money(afa), fmt_money(end))).strong());
            });
        });
        if open.is_some() {
            self.books.asset_edit = open;
        }
    }

    fn asset_dialog(&mut self, ctx: &egui::Context) {
        let mut a = self.books.asset_edit.take().unwrap();
        let (mut keep, mut save, mut delete) = (true, false, false);
        let modal = egui::Modal::new(Id::unique("asset")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new(if a.id == 0 { t("Neues Anlagegut") } else { t("Anlagegut") }).size(18.0).strong());
            ui.add_space(6.0);
            egui::Grid::new("asset_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                text_row(ui, t("Bezeichnung"), &mut a.name, 340.0);
                ui.label(muted(t("Inventar-Nr.")));
                ui.add(TextEdit::singleline(&mut a.number).hint_text(t("wird vergeben")).desired_width(140.0));
                ui.end_row();
                ui.label(muted(t("Anlagekonto")));
                egui::ComboBox::from_id_salt("asset_account").width(340.0).selected_text(tr(&a.account)).show_ui(ui, |ui| {
                    for acc in ASSET_ACCOUNTS {
                        ui.selectable_value(&mut a.account, acc.to_string(), t(acc));
                    }
                });
                ui.end_row();
                ui.label(muted(t("Anschaffung am")));
                date_edit(ui, Id::unique("asset_date"), &mut a.date);
                ui.end_row();
                ui.label(muted(t("Kosten netto")));
                ui.horizontal(|ui| {
                    money_edit(ui, Id::unique("asset_cost"), &mut a.cost, 110.0);
                    ui.label("€");
                });
                ui.end_row();
                ui.label(muted(t("Abschreibung")));
                ui.vertical(|ui| {
                    ui.radio_value(&mut a.method, AfaMethod::Linear, t("Linear über die Nutzungsdauer"));
                    ui.radio_value(&mut a.method, AfaMethod::Gwg, t("Geringwertiges Wirtschaftsgut (bis 800 € netto, sofort)"));
                    ui.radio_value(&mut a.method, AfaMethod::Sammelposten, t("Sammelposten (250 bis 1.000 € netto, 5 Jahre)"));
                });
                ui.end_row();
                if a.method == AfaMethod::Linear {
                    ui.label(muted(t("Nutzungsdauer")));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut a.years).range(1..=50).suffix(t(" Jahre")));
                        ui.label(muted(t("laut AfA-Tabelle, z. B. Computer 1, Büromöbel 13, Pkw 6")));
                    });
                    ui.end_row();
                }
                ui.label(muted(t("Abgang")));
                ui.horizontal(|ui| {
                    let mut gone = a.disposed.is_some();
                    if ui.checkbox(&mut gone, t("verkauft oder entsorgt am")).changed() {
                        a.disposed = gone.then_some(self.today);
                    }
                    if let Some(d) = &mut a.disposed {
                        date_edit(ui, Id::unique("asset_gone"), d);
                    }
                });
                ui.end_row();
                ui.label(muted(t("Notiz")));
                ui.add(TextEdit::singleline(&mut a.note).desired_width(340.0));
                ui.end_row();
            });
            if a.cost > 0 {
                ui.add_space(6.0);
                ui.label(RichText::new(t("Abschreibungsplan")).strong());
                let start = a.date.year();
                ui.horizontal_wrapped(|ui| {
                    for y in start..start + 12 {
                        let v = accounting::afa_for_year(&a, y);
                        if v == 0 && y > start {
                            break;
                        }
                        ui.label(muted(format!("{y}: {}", fmt_money(v))));
                    }
                });
            }
            if a.method == AfaMethod::Gwg && a.cost > 80_000 {
                ui.label(RichText::new(t("Geringwertige Wirtschaftsgüter dürfen höchstens 800 € netto kosten.")).color(ORANGE));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
                if a.id != 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger(ui, t("Löschen")).clicked() {
                            delete = true;
                        }
                    });
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save && self.run(Op::SaveAsset(a.clone())).is_ok() {
            self.info(t("Anlagegut gespeichert."));
            keep = false;
        }
        if delete {
            self.confirm = Some((tf!("Anlagegut „{}“ löschen?", a.name), Op::DeleteAsset(a.id)));
            keep = false;
        }
        if keep {
            self.books.asset_edit = Some(a);
        }
    }

    fn journal(&mut self, ui: &mut Ui) {
        let chart = self.data.company.chart;
        ui.horizontal(|ui| {
            ui.label(muted(t("Manuelle Buchungen Soll an Haben, z. B. Eröffnungsbilanz (gegen 9000 Saldenvorträge), Abgrenzungen oder Korrekturen.")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary(ui, t("+ Umbuchung")).clicked() {
                    self.books.journal_edit = Some(JournalEntry { date: self.today, ..JournalEntry::default() });
                }
            });
        });
        if self.data.company.accounting == Accounting::Euer {
            ui.label(RichText::new(t("Ihre Firma ermittelt den Gewinn per Einnahmen-Überschuss-Rechnung. Umbuchungen wirken nur auf GuV und Bilanz (Einstellungen → Buchhaltung).")).color(ORANGE));
        }
        ui.add_space(8.0);
        let name = |acc: &str| {
            let n = accounting::account_name(&acc_to_skr03(chart, acc));
            if n.is_empty() { acc.to_string() } else { format!("{acc} {}", tr(&n)) }
        };
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &JOURNAL_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if self.data.journal.is_empty() {
                    empty_hint(ui, t("Noch keine Umbuchungen."));
                }
                for e in self.data.journal.iter().rev() {
                    let cells = [
                        Cell::T(fmt_date(e.date)),
                        Cell::T(name(&e.debit)),
                        Cell::T(name(&e.credit)),
                        Cell::Money(e.amount),
                        Cell::T(e.text.clone()),
                    ];
                    if table_row(ui, &JOURNAL_COLS, &cells, false).clicked() {
                        open = Some(e.clone());
                    }
                }
            });
        });
        if open.is_some() {
            self.books.journal_edit = open;
        }
    }

    fn journal_dialog(&mut self, ctx: &egui::Context) {
        let mut e = self.books.journal_edit.take().unwrap();
        let (mut keep, mut save, mut delete) = (true, false, false);
        let chart = self.data.company.chart;
        let modal = egui::Modal::new(Id::unique("journal")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(t("Umbuchung")).size(18.0).strong());
            ui.add_space(6.0);
            egui::Grid::new("journal_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(muted(t("Datum")));
                date_edit(ui, Id::unique("j_date"), &mut e.date);
                ui.end_row();
                for (label, value) in [(t("Sollkonto"), &mut e.debit), (t("Habenkonto"), &mut e.credit)] {
                    ui.label(muted(label));
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(value).desired_width(70.0).hint_text("0000"));
                        let n = accounting::account_name(&acc_to_skr03(chart, value));
                        ui.label(muted(tr(&n)));
                    });
                    ui.end_row();
                }
                ui.label(muted(t("Betrag")));
                ui.horizontal(|ui| {
                    money_edit(ui, Id::unique("j_amount"), &mut e.amount, 110.0);
                    ui.label("€");
                });
                ui.end_row();
                text_row(ui, t("Buchungstext"), &mut e.text, 320.0);
                text_row(ui, t("Belegnummer"), &mut e.receipt, 160.0);
            });
            ui.label(muted(match chart {
                Chart::Skr03 => t("Kontonummern im SKR 03, z. B. 1200 Bank, 1000 Kasse, 1400 Forderungen, 1600 Verbindlichkeiten, 0800 Kapital, 9000 Saldenvorträge."),
                Chart::Skr04 => t("Kontonummern im SKR 04, z. B. 1800 Bank, 1600 Kasse, 1200 Forderungen, 3300 Verbindlichkeiten, 2900 Kapital, 9000 Saldenvorträge."),
            }));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
                if e.id != 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if danger(ui, t("Löschen")).clicked() {
                            delete = true;
                        }
                    });
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if save && self.run(Op::SaveJournal(e.clone())).is_ok() {
            self.info(t("Umbuchung gespeichert."));
            keep = false;
        }
        if delete {
            self.confirm = Some((t("Diese Umbuchung löschen?").into(), Op::DeleteJournal(e.id)));
            keep = false;
        }
        if keep {
            self.books.journal_edit = Some(e);
        }
    }
}

/// An account number from the company's chart back to SKR 03 (for its name).
pub fn acc_to_skr03(chart: Chart, acc: &str) -> String {
    accounting::to_skr03(chart, acc)
}
