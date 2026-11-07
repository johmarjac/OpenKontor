//! Year-end reports and exports: Anlage EÜR, EU sales list (ZM), profit and loss with
//! balance sheet, DATEV and GoBD exports, procedure documentation.

use super::*;
use crate::accounting;
use crate::pdf::Block;

fn amount_cell(ui: &mut Ui, text: RichText) {
    right_label(ui, 150.0, text);
}

impl App {
    fn save_bytes(&mut self, name: &str, filter: &str, ext: &str, bytes: Vec<u8>) {
        if let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter(filter, &[ext]).save_file() {
            match std::fs::write(&path, bytes) {
                Ok(()) => self.info(tf!("Gespeichert: {}", path.display())),
                Err(e) => self.error(e.to_string()),
            }
        }
    }

    pub(super) fn report_anlage_euer(&mut self, ui: &mut Ui) {
        let year = self.report_year;
        let f = accounting::euer_form(&self.data, year);
        let mut pdf = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            ui.horizontal(|ui| {
                card_title(ui, &tf!("Anlage EÜR {}", year));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t("Als PDF speichern …")).clicked() {
                        pdf = true;
                    }
                });
            });
            ui.label(muted(t("Gruppiert wie das amtliche Formular. Übertragen Sie die Summen in ELSTER (Mein ELSTER → Formulare → Anlage EÜR) oder geben Sie sie Ihrer Steuerberatung.")));
            if self.data.company.accounting == Accounting::Bilanz {
                ui.label(RichText::new(t("Ihre Firma bilanziert – statt der Anlage EÜR gelten GuV und Bilanz.")).color(ORANGE));
            }
            ui.add_space(6.0);
            egui::Grid::new("anlage_euer").num_columns(2).spacing([40.0, 6.0]).striped(true).min_col_width(260.0).show(ui, |ui| {
                ui.label(RichText::new(t("Betriebseinnahmen")).strong().color(GREEN));
                ui.end_row();
                for (k, v) in &f.income {
                    ui.label(t(k));
                    amount_cell(ui, RichText::new(fmt_money(*v)));
                    ui.end_row();
                }
                ui.label(RichText::new(t("Summe Betriebseinnahmen")).strong());
                amount_cell(ui, RichText::new(fmt_money(f.total_income)).strong());
                ui.end_row();
                ui.label("");
                ui.end_row();
                ui.label(RichText::new(t("Betriebsausgaben")).strong().color(RED));
                ui.end_row();
                for (k, v) in &f.expenses {
                    ui.label(t(k));
                    amount_cell(ui, RichText::new(fmt_money(*v)));
                    ui.end_row();
                }
                ui.label(RichText::new(t("Summe Betriebsausgaben")).strong());
                amount_cell(ui, RichText::new(fmt_money(f.total_expenses)).strong());
                ui.end_row();
                if f.entertainment_not_deductible > 0 {
                    ui.label(muted(t("Hinzurechnung: nicht abziehbare Bewirtungskosten (30 %)")));
                    amount_cell(ui, muted(fmt_money(f.entertainment_not_deductible)));
                    ui.end_row();
                }
                ui.label("");
                ui.end_row();
                ui.label(RichText::new(if f.profit >= 0 { t("Gewinn") } else { t("Verlust") }).strong().size(16.0));
                amount_cell(ui, RichText::new(fmt_money(f.profit)).strong().size(16.0).color(if f.profit >= 0 { GREEN } else { RED }));
                ui.end_row();
            });
        });
        if pdf {
            let mut rows: Vec<(String, String, bool)> = vec![(t("Betriebseinnahmen").into(), String::new(), true)];
            rows.extend(f.income.iter().map(|(k, v)| (t(k).to_string(), fmt_money(*v), false)));
            rows.push((t("Summe Betriebseinnahmen").into(), fmt_money(f.total_income), true));
            rows.push((String::new(), String::new(), false));
            rows.push((t("Betriebsausgaben").into(), String::new(), true));
            rows.extend(f.expenses.iter().map(|(k, v)| (t(k).to_string(), fmt_money(*v), false)));
            rows.push((t("Summe Betriebsausgaben").into(), fmt_money(f.total_expenses), true));
            if f.entertainment_not_deductible > 0 {
                rows.push((t("Hinzurechnung: nicht abziehbare Bewirtungskosten (30 %)").into(), fmt_money(f.entertainment_not_deductible), false));
            }
            rows.push((String::new(), String::new(), false));
            rows.push((if f.profit >= 0 { t("Gewinn") } else { t("Verlust") }.into(), fmt_money(f.profit), true));
            let bytes = crate::pdf::report_pdf(&self.data, &tf!("Anlage EÜR {}", year), &tf!("Gewinnermittlung nach § 4 Abs. 3 EStG · erstellt am {}", fmt_date(self.today)), &[Block::Rows(rows)]);
            self.save_bytes(&format!("Anlage-EUER-{year}.pdf"), "PDF", "pdf", bytes);
        }
    }

    pub(super) fn report_zm(&mut self, ui: &mut Ui, from: NaiveDate, to: NaiveDate, range: &str) {
        let lines = accounting::zm_report(&self.data, from, to);
        let mut csv = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            ui.horizontal(|ui| {
                card_title(ui, &tf!("Zusammenfassende Meldung · {}", range));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !lines.is_empty() && ui.button(t("Als CSV speichern …")).clicked() {
                        csv = true;
                    }
                });
            });
            ui.label(muted(t("Steuerfreie Lieferungen (L) und Leistungen (S) an Unternehmen in anderen EU-Ländern, nach Rechnungsdatum. Meldung an das Bundeszentralamt für Steuern (BZStOnline-Portal), meist quartalsweise.")));
            ui.add_space(6.0);
            if lines.is_empty() {
                empty_hint(ui, t("Im Zeitraum gibt es keine EU-Umsätze ohne Umsatzsteuer."));
                return;
            }
            egui::Grid::new("zm").num_columns(4).spacing([30.0, 6.0]).striped(true).show(ui, |ui| {
                for h in ["Land", "USt-IdNr.", "Art", "Betrag"] {
                    ui.label(muted(t(h)));
                }
                ui.end_row();
                for l in &lines {
                    ui.label(&l.country);
                    ui.label(&l.vat_id);
                    ui.label(match l.kind {
                        'L' => t("L – Lieferung"),
                        _ => t("S – sonstige Leistung"),
                    });
                    amount_cell(ui, RichText::new(fmt_money(l.amount)));
                    ui.end_row();
                }
            });
        });
        if csv {
            let mut text = String::from("Laenderkennzeichen;USt-IdNr.;Betrag (EUR);Art der Leistung\r\n");
            for l in &lines {
                text.push_str(&format!("{};{};{};{}\r\n", l.country, l.vat_id, (l.amount as f64 / 100.0).round() as i64, l.kind));
            }
            self.save_bytes(&format!("ZM-{}-{}.csv", from.format("%Y%m%d"), to.format("%Y%m%d")), "CSV", "csv", text.into_bytes());
        }
    }

    pub(super) fn report_statements(&mut self, ui: &mut Ui) {
        let year = self.report_year;
        let s = accounting::statements(&self.data, year);
        let chart = self.data.company.chart;
        let name = |acc: &str| {
            if acc.chars().all(|c| c.is_ascii_digit()) {
                let n = accounting::account_name(acc);
                format!("{} {}", accounting::account_in(chart, acc), tr(&n))
            } else {
                t_dyn(acc)
            }
        };
        let mut pdf = false;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(900.0));
            ui.horizontal(|ui| {
                card_title(ui, &tf!("Gewinn- und Verlustrechnung und Bilanz {}", year));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t("Als PDF speichern …")).clicked() {
                        pdf = true;
                    }
                });
            });
            ui.label(muted(match self.data.company.accounting {
                Accounting::Bilanz => t("Doppelte Buchführung: Rechnungen zählen ab Rechnungsdatum (Forderungen), Zahlungen gleichen sie aus. Anfangsbestände über Umbuchungen gegen 9000 erfassen."),
                Accounting::Euer => t("Aus den Buchungen abgeleitet (Ist-Prinzip). Für bilanzierende Firmen unter Einstellungen → Buchhaltung „Bilanz“ wählen."),
            }));
            ui.add_space(8.0);
            let w = (ui.available_width() - 30.0) / 2.0;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.label(RichText::new(t("Gewinn- und Verlustrechnung")).strong());
                    egui::Grid::new("guv").num_columns(2).spacing([20.0, 5.0]).striped(true).show(ui, |ui| {
                        ui.label(RichText::new(t("Erträge")).color(GREEN));
                        ui.end_row();
                        for (a, v) in &s.revenue {
                            ui.label(name(a));
                            amount_cell(ui, RichText::new(fmt_money(*v)));
                            ui.end_row();
                        }
                        ui.label(RichText::new(t("Aufwendungen")).color(RED));
                        ui.end_row();
                        for (a, v) in &s.expenses {
                            ui.label(name(a));
                            amount_cell(ui, RichText::new(fmt_money(*v)));
                            ui.end_row();
                        }
                        ui.label(RichText::new(if s.profit >= 0 { t("Jahresüberschuss") } else { t("Jahresfehlbetrag") }).strong());
                        amount_cell(ui, RichText::new(fmt_money(s.profit)).strong());
                        ui.end_row();
                    });
                });
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.label(RichText::new(tf!("Bilanz zum 31.12.{}", year)).strong());
                    egui::Grid::new("bilanz").num_columns(2).spacing([20.0, 5.0]).striped(true).show(ui, |ui| {
                        ui.label(RichText::new(t("Aktiva")).color(ACCENT));
                        ui.end_row();
                        for (a, v) in &s.assets {
                            ui.label(name(a));
                            amount_cell(ui, RichText::new(fmt_money(*v)));
                            ui.end_row();
                        }
                        ui.label(RichText::new(t("Summe Aktiva")).strong());
                        amount_cell(ui, RichText::new(fmt_money(s.total_assets)).strong());
                        ui.end_row();
                        ui.label(RichText::new(t("Passiva")).color(ACCENT));
                        ui.end_row();
                        for (a, v) in s.equity.iter().chain(&s.liabilities) {
                            ui.label(name(a));
                            amount_cell(ui, RichText::new(fmt_money(*v)));
                            ui.end_row();
                        }
                        ui.label(RichText::new(t("Summe Passiva")).strong());
                        amount_cell(ui, RichText::new(fmt_money(s.total_liabilities)).strong());
                        ui.end_row();
                    });
                    if s.total_assets != s.total_liabilities {
                        ui.label(RichText::new(t("Aktiva und Passiva weichen ab – bitte Umbuchungen prüfen.")).color(RED));
                    }
                });
            });
        });
        if pdf {
            let mut guv = vec![(t("Erträge").to_string(), String::new(), true)];
            guv.extend(s.revenue.iter().map(|(a, v)| (name(a), fmt_money(*v), false)));
            guv.push((t("Aufwendungen").into(), String::new(), true));
            guv.extend(s.expenses.iter().map(|(a, v)| (name(a), fmt_money(*v), false)));
            guv.push((String::new(), String::new(), false));
            guv.push((if s.profit >= 0 { t("Jahresüberschuss") } else { t("Jahresfehlbetrag") }.into(), fmt_money(s.profit), true));
            let mut bil = vec![(t("Aktiva").to_string(), String::new(), true)];
            bil.extend(s.assets.iter().map(|(a, v)| (name(a), fmt_money(*v), false)));
            bil.push((t("Summe Aktiva").into(), fmt_money(s.total_assets), true));
            bil.push((String::new(), String::new(), false));
            bil.push((t("Passiva").into(), String::new(), true));
            bil.extend(s.equity.iter().chain(&s.liabilities).map(|(a, v)| (name(a), fmt_money(*v), false)));
            bil.push((t("Summe Passiva").into(), fmt_money(s.total_liabilities), true));
            let blocks = [
                Block::Heading(t("Gewinn- und Verlustrechnung").into()),
                Block::Rows(guv),
                Block::Heading(tf!("Bilanz zum 31.12.{}", year)),
                Block::Rows(bil),
            ];
            let bytes = crate::pdf::report_pdf(&self.data, &tf!("Jahresabschluss {}", year), &tf!("erstellt am {} mit OpenKontor", fmt_date(self.today)), &blocks);
            self.save_bytes(&format!("Jahresabschluss-{year}.pdf"), "PDF", "pdf", bytes);
        }
    }

    pub(super) fn report_exports(&mut self, ui: &mut Ui, from: NaiveDate, to: NaiveDate, range: &str) {
        let year = self.report_year;
        let mut job: Option<u8> = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, t("DATEV für die Steuerberatung"));
            ui.label(muted(tf!(
                "Buchungsstapel im DATEV-Format mit den Originalbelegen des Belegeingangs, Zeitraum {}. Berater- und Mandantennummer unter Einstellungen → Buchhaltung.",
                range
            )));
            if self.data.company.datev_advisor.is_empty() || self.data.company.datev_client.is_empty() {
                ui.label(RichText::new(t("Berater- oder Mandantennummer fehlt noch.")).color(ORANGE));
            }
            if icon_button(ui, Icon::Download, t("DATEV-Export speichern …")).clicked() {
                job = Some(1);
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, t("Datenexport für die Betriebsprüfung (GoBD)"));
            ui.label(muted(tf!(
                "Alle steuerlich relevanten Daten des Jahres {} als CSV-Tabellen mit Beschreibung (index.xml) nach dem Beschreibungsstandard der Finanzverwaltung, lesbar z. B. mit IDEA.",
                year
            )));
            if icon_button(ui, Icon::Download, t("GoBD-Export speichern …")).clicked() {
                job = Some(2);
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, t("Verfahrensdokumentation"));
            ui.label(muted(t("Die GoBD verlangen eine Beschreibung, wie Belege entstehen, gebucht und aufbewahrt werden. OpenKontor erstellt eine Vorlage mit Ihren Firmendaten; die Stellen mit „…“ ergänzen Sie.")));
            if icon_button(ui, Icon::Document, t("Vorlage als PDF speichern …")).clicked() {
                job = Some(3);
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, t("ELSTER"));
            ui.label(muted(t("Die direkte Übermittlung an das Finanzamt braucht die amtliche ELSTER-Schnittstelle (ERiC) und eine Zulassung als Hersteller. Bis dahin: Werte der Umsatzsteuer-Voranmeldung und der Anlage EÜR in „Mein ELSTER“ eintragen – die Kennzahlen stehen in den Auswertungen.")));
        });
        match job {
            Some(1) => match self.backend.datev_package(&self.data, from, to) {
                Ok(bytes) => self.save_bytes(&format!("DATEV-{}-{}.zip", from.format("%Y%m%d"), to.format("%Y%m%d")), "ZIP", "zip", bytes),
                Err(e) => self.error(tr(&e)),
            },
            Some(2) => {
                let bytes = crate::exports::gobd_export(&self.data, year);
                self.save_bytes(&format!("GoBD-Export-{year}.zip"), "ZIP", "zip", bytes);
            }
            Some(3) => {
                let server = matches!(self.backend, Backend::Remote(_));
                let bytes = crate::exports::procedure_documentation(&self.data, server);
                self.save_bytes("Verfahrensdokumentation.pdf", "PDF", "pdf", bytes);
            }
            _ => {}
        }
    }
}

/// Labels made in the accounting module ("Gewinnvortrag", "Jahresergebnis").
fn t_dyn(key: &str) -> String {
    match key {
        "Gewinnvortrag" => t("Gewinnvortrag").into(),
        "Jahresergebnis" => t("Jahresergebnis").into(),
        other => tr(other),
    }
}
