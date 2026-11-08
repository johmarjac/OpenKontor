//! Taking over data from another program, the dunning run for all overdue invoices and
//! the cash-flow forecast.

use super::*;
use crate::import::{ImportKind, Parsed};

/// One overdue invoice in the dunning run.
#[derive(Clone)]
pub struct DunRow {
    pub doc_id: u64,
    pub selected: bool,
    pub level: u8,
    pub fee: Cents,
    pub email: String,
}

#[derive(Default)]
pub struct ExtrasState {
    /// A file read for taking over: kind, file name and what was understood.
    pub import: Option<(ImportKind, String, Parsed)>,
    pub dunning: Option<Vec<DunRow>>,
    /// Send reminders by e-mail where an address is known (else all as PDF for the post).
    pub dun_by_mail: bool,
    pub balance: Option<(Cents, NaiveDate)>,
    pub weeks: usize,
}

impl App {
    // ---- Taking over data ---------------------------------------------------------------

    /// The card under Settings → Data.
    pub(super) fn import_card(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Daten übernehmen (z. B. aus Lexware)"));
            ui.label(muted(t("Exportieren Sie im bisherigen Programm Kunden, Artikel und offene Posten als CSV-Datei und lesen Sie sie hier ein. Die Spalten werden an den Überschriften erkannt; vorhandene Nummern werden übersprungen.")));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (kind, label) in [
                    (ImportKind::Contacts, t("Kontakte …")),
                    (ImportKind::Articles, t("Artikel …")),
                    (ImportKind::OpenItems, t("Offene Posten …")),
                ] {
                    if ui.add_enabled(self.backend.allows(OpArea::Settings), egui::Button::new(label)).clicked() {
                        self.pick_import(kind);
                    }
                }
            });
            ui.label(muted(t("Reihenfolge: zuerst Kontakte, dann offene Posten – die Rechnungen werden über Kundennummer oder Namen zugeordnet.")));
        });
    }

    fn pick_import(&mut self, kind: ImportKind) {
        let Some(path) = rfd::FileDialog::new().add_filter(t("CSV-Datei"), &["csv", "txt"]).pick_file() else { return };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| crate::import::parse(kind, &b, &self.data)) {
            Ok(p) => self.extras.import = Some((kind, name, p)),
            Err(e) => self.error(tr(&e)),
        }
    }

    fn import_dialog(&mut self, ctx: &egui::Context) {
        let Some((kind, name, parsed)) = self.extras.import.take() else { return };
        let (mut keep, mut go) = (true, false);
        let modal = egui::Modal::new(Id::unique("import")).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.label(RichText::new(tf!("Daten übernehmen: {}", name)).size(18.0).strong());
            ui.label(match kind {
                ImportKind::Contacts => tf!("{} Kontakte erkannt", parsed.contacts.len()),
                ImportKind::Articles => tf!("{} Artikel erkannt", parsed.articles.len()),
                ImportKind::OpenItems => tf!("{} offene Posten erkannt", parsed.items.len()),
            });
            ui.add_space(4.0);
            ui.label(RichText::new(t("Zuordnung der Spalten")).strong());
            egui::Grid::new("import_map").num_columns(2).spacing([20.0, 3.0]).show(ui, |ui| {
                for (field, column) in &parsed.mapping {
                    ui.label(muted(t_dyn(field)));
                    ui.label(column);
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new(t("Beispiele")).strong());
            ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                for c in parsed.contacts.iter().take(5) {
                    ui.label(format!("{} · {} · {}", c.number, c.display_name(), c.zip_city));
                }
                for a in parsed.articles.iter().take(5) {
                    ui.label(format!("{} · {} · {}", a.number, a.name, fmt_money(a.price_net)));
                }
                for d in parsed.items.iter().take(5) {
                    ui.label(format!("{} · {} · {} · {}", d.number, self.data.contact_name(d.contact_id), fmt_date(d.date), fmt_money_cur(crate::i18n::lang(), d.totals(self.data.company.small_business).gross, d.currency())));
                }
            });
            if !parsed.problems.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tf!("{} Zeilen werden nicht übernommen:", parsed.problems.len())).color(ORANGE));
                ScrollArea::vertical().id_salt("import_problems").max_height(90.0).show(ui, |ui| {
                    for p in &parsed.problems {
                        ui.label(muted(p));
                    }
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_enabled(ui, parsed.count() > 0, t("Übernehmen")).clicked() {
                    go = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if go {
            let op = match kind {
                ImportKind::Contacts => Op::ImportContacts(parsed.contacts.clone()),
                ImportKind::Articles => Op::ImportArticles(parsed.articles.clone()),
                ImportKind::OpenItems => Op::ImportOpenItems(parsed.items.clone()),
            };
            if self.run(op).is_ok() {
                let msg = self.data.log.last().map(|l| tr(&l.action)).unwrap_or_default();
                self.info(msg);
                keep = false;
            }
        }
        if keep {
            self.extras.import = Some((kind, name, parsed));
        }
    }

    // ---- Dunning run ------------------------------------------------------------------------

    /// Collects all overdue invoices whose last reminder deadline has passed.
    pub(super) fn open_dunning(&mut self) {
        let rows: Vec<DunRow> = self
            .data
            .documents
            .iter()
            .filter(|d| d.is_overdue(self.today) && d.reminders.last().is_none_or(|r| r.due < self.today))
            .map(|d| {
                let level = crate::mail::next_reminder_level(d);
                DunRow {
                    doc_id: d.id,
                    selected: true,
                    level,
                    fee: self.data.company.reminder_fees.get(level as usize).copied().unwrap_or(0),
                    email: self.data.contact(d.contact_id).map(|c| c.email.clone()).unwrap_or_default(),
                }
            })
            .collect();
        if rows.is_empty() {
            self.info(t("Keine Rechnung ist zu mahnen: Alles bezahlt oder die Fristen der letzten Mahnungen laufen noch."));
            return;
        }
        self.extras.dun_by_mail = true;
        self.extras.dunning = Some(rows);
    }

    fn dunning_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut rows) = self.extras.dunning.take() else { return };
        let (mut keep, mut go) = (true, false);
        let small = self.data.company.small_business;
        let modal = egui::Modal::new(Id::unique("dunning")).show(ctx, |ui| {
            ui.set_width(760.0);
            ui.label(RichText::new(t("Mahnlauf")).size(18.0).strong());
            ui.label(muted(tf!("Überfällige Rechnungen, deren letzte Frist abgelaufen ist. Neue Zahlungsfrist: {} Tage.", self.data.company.reminder_days)));
            ui.add_space(6.0);
            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                egui::Grid::new("dun_grid").num_columns(6).spacing([14.0, 6.0]).striped(true).show(ui, |ui| {
                    for h in ["", "Rechnung", "Kunde", "Überfällig", "Stufe", "Gebühr"] {
                        ui.label(muted(t(h)));
                    }
                    ui.end_row();
                    for r in rows.iter_mut() {
                        let Some(d) = self.data.documents.iter().find(|d| d.id == r.doc_id) else { continue };
                        ui.checkbox(&mut r.selected, "");
                        ui.label(format!("{} ({})", d.number, fmt_money_cur(crate::i18n::lang(), d.open_amount(small), d.currency())));
                        ui.label(self.data.contact_name(d.contact_id));
                        ui.label(tf!("{} Tage", (self.today - d.due_date()).num_days()));
                        ui.label(t(reminder_title(r.level)));
                        money_edit(ui, Id::unique(("dun_fee", r.doc_id)), &mut r.fee, 70.0);
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            ui.checkbox(&mut self.extras.dun_by_mail, t("Per E-Mail senden, wo eine Adresse hinterlegt ist"));
            let post = rows.iter().filter(|r| r.selected && (!self.extras.dun_by_mail || r.email.is_empty())).count();
            if post > 0 {
                ui.label(muted(tf!("{} Mahnungen gehen per Post: Sie werden als PDF in einem Ordner gespeichert.", post)));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let n = rows.iter().filter(|r| r.selected).count();
                if primary_enabled(ui, n > 0, &tf!("{} Mahnungen erstellen", n)).clicked() {
                    go = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if go {
            self.run_dunning(&rows);
            keep = false;
        }
        if keep {
            self.extras.dunning = Some(rows);
        }
    }

    fn run_dunning(&mut self, rows: &[DunRow]) {
        let days = self.data.company.reminder_days;
        let deadline = self.today + chrono::Duration::days(days as i64);
        let (mut mailed, mut problems) = (0, Vec::new());
        let mut letters: Vec<(String, Vec<u8>)> = Vec::new();
        for r in rows.iter().filter(|r| r.selected) {
            let Some(doc) = self.data.documents.iter().find(|d| d.id == r.doc_id).cloned() else { continue };
            let kind = MailKind::Mahnung(r.level);
            if self.extras.dun_by_mail && !r.email.is_empty() {
                let lang = self.data.contact(doc.contact_id).map(|c| c.language).unwrap_or_default();
                let (subject, body) = crate::mail::template(&self.data.company, kind, lang);
                let req = crate::mail::SendRequest {
                    doc_id: doc.id,
                    kind,
                    to: r.email.clone(),
                    cc: String::new(),
                    subject: crate::mail::render(&subject, &self.data, &doc, r.fee, deadline),
                    body: crate::mail::render(&body, &self.data, &doc, r.fee, deadline),
                    fee: r.fee,
                    deadline_days: days,
                };
                match self.backend.send_mail(&mut self.data, &req) {
                    Ok(()) => mailed += 1,
                    Err(e) => problems.push(format!("{}: {}", doc.number, tr(&e))),
                }
            } else {
                let rec = ReminderRecord { level: r.level, date: self.today, fee: r.fee, due: deadline };
                letters.push((crate::pdf::reminder_file_name(&doc, &rec), crate::pdf::reminder_pdf(&self.data, &doc, &rec)));
                if self.run(Op::RecordReminder(doc.id, rec)).is_err() {
                    letters.pop();
                }
            }
        }
        if !letters.is_empty()
            && let Some(dir) = rfd::FileDialog::new().set_title(t("Ordner für die Mahnungen per Post")).pick_folder()
        {
            for (name, bytes) in &letters {
                if let Err(e) = std::fs::write(dir.join(format!("{name}.pdf").replace(".pdf.pdf", ".pdf")), bytes) {
                    problems.push(e.to_string());
                }
            }
            export::open_file(&dir);
        }
        if problems.is_empty() {
            self.info(tf!("Mahnlauf erledigt: {} per E-Mail, {} per Post.", mailed, letters.len()));
        } else {
            self.error(tf!("Mahnlauf: {} per E-Mail, {} per Post. Probleme: {}", mailed, letters.len(), problems.join("; ")));
        }
    }

    pub(super) fn extras_dialogs(&mut self, ctx: &egui::Context) {
        if self.extras.import.is_some() {
            self.import_dialog(ctx);
        }
        if self.extras.dunning.is_some() {
            self.dunning_dialog(ctx);
        }
    }

    // ---- Cash-flow forecast -----------------------------------------------------------------

    pub(super) fn report_forecast(&mut self, ui: &mut Ui) {
        let c = &self.data.company;
        let (mut balance, mut day) = self.extras.balance.unwrap_or((c.bank_balance, c.bank_balance_date.unwrap_or(self.today)));
        if self.extras.weeks == 0 {
            self.extras.weeks = 12;
        }
        let mut save = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(1000.0));
            card_title(ui, t("Liquiditätsvorschau"));
            ui.label(muted(t("Erwarteter Kontostand der nächsten Wochen: offene Rechnungen zum Fälligkeitstag, offene Eingangsbelege, wiederkehrende Rechnungen und die durchschnittlichen laufenden Ausgaben der letzten drei Monate.")));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(muted(t("Kontostand")));
                money_edit(ui, Id::unique("fc_balance"), &mut balance, 110.0);
                ui.label(muted(t("am")));
                date_edit(ui, Id::unique("fc_day"), &mut day);
                if ui.button(t("Merken")).on_hover_text(t("Speichert den Kontostand bei den Firmendaten")).clicked() {
                    save = true;
                }
                ui.separator();
                ui.label(muted(t("Wochen")));
                ui.add(egui::DragValue::new(&mut self.extras.weeks).range(4..=52));
            });
        });
        self.extras.balance = Some((balance, day));
        if save {
            let mut company = self.data.company.clone();
            company.bank_balance = balance;
            company.bank_balance_date = Some(day);
            if self.run(Op::SaveCompany(company)).is_ok() {
                self.info(t("Kontostand gespeichert."));
            }
        }
        let cash = self.data.cash_balance(self.today);
        let weeks = crate::accounting::forecast(&self.data, balance + cash, self.today, self.extras.weeks);
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(1000.0));
            if cash != 0 {
                ui.label(muted(tf!("Mit Kassenbestand {}", fmt_money(cash))));
            }
            // A simple chart: balance at the end of each week.
            let h = 160.0;
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
            let p = ui.painter();
            let max = weeks.iter().map(|w| w.balance.abs()).max().unwrap_or(1).max(1) as f32;
            let zero = rect.center().y;
            p.line_segment([egui::pos2(rect.left(), zero), egui::pos2(rect.right(), zero)], Stroke::new(1.0, BORDER));
            let bw = rect.width() / weeks.len().max(1) as f32;
            for (i, w) in weeks.iter().enumerate() {
                let x = rect.left() + i as f32 * bw;
                let len = w.balance as f32 / max * (h / 2.0 - 6.0);
                let bar = egui::Rect::from_two_pos(egui::pos2(x + bw * 0.2, zero), egui::pos2(x + bw * 0.8, zero - len));
                p.rect_filled(bar, CornerRadius::same(2), if w.balance >= 0 { INCOME } else { RED });
            }
            ui.add_space(8.0);
            egui::Grid::new("forecast").num_columns(5).spacing([24.0, 5.0]).striped(true).show(ui, |ui| {
                for h in ["Woche ab", "Eingänge", "Ausgänge", "Kontostand", "Größte Posten"] {
                    ui.label(muted(t(h)));
                }
                ui.end_row();
                for w in &weeks {
                    ui.label(fmt_date(w.start));
                    right_label(ui, 110.0, RichText::new(fmt_money(w.inflow)).color(GREEN));
                    right_label(ui, 110.0, RichText::new(fmt_money(-w.outflow)).color(RED));
                    right_label(ui, 120.0, RichText::new(fmt_money(w.balance)).strong().color(if w.balance < 0 { RED } else { TEXT }));
                    let mut items = w.items.clone();
                    items.sort_by_key(|(_, a)| -a.abs());
                    let text: Vec<String> = items.iter().take(2).map(|(t, a)| format!("{} {}", tr(t), fmt_money(*a))).collect();
                    ui.label(muted(text.join(" · ")));
                    ui.end_row();
                }
            });
            if let Some(w) = weeks.iter().find(|w| w.balance < 0) {
                ui.add_space(6.0);
                ui.label(RichText::new(tf!("Achtung: In der Woche ab {} reicht das Geld voraussichtlich nicht.", fmt_date(w.start))).color(RED));
            }
        });
    }
}

/// Field names of the import mapping.
fn t_dyn(field: &str) -> String {
    tr(field)
}
