//! Reports: profit statement (EÜR), VAT return figures, open items, revenue per customer.

use super::*;

fn money_cell(ui: &mut Ui, text: RichText) {
    right_label(ui, 150.0, text);
}

impl App {
    pub(super) fn reports(&mut self, ui: &mut Ui) {
        let years = self.years();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.report_tab, ReportTab::Euer, t("EÜR"));
            ui.selectable_value(&mut self.report_tab, ReportTab::AnlageEuer, t("Anlage EÜR"));
            ui.selectable_value(&mut self.report_tab, ReportTab::Ustva, t("Umsatzsteuer-Voranmeldung"));
            ui.selectable_value(&mut self.report_tab, ReportTab::Zm, t("Zusammenfassende Meldung"));
            ui.selectable_value(&mut self.report_tab, ReportTab::Bilanz, t("GuV & Bilanz"));
            ui.selectable_value(&mut self.report_tab, ReportTab::OffenePosten, t("Offene Posten"));
            ui.selectable_value(&mut self.report_tab, ReportTab::Kunden, t("Umsatz je Kunde"));
            ui.selectable_value(&mut self.report_tab, ReportTab::Exporte, t("Exporte"));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("r_period")
                    .selected_text(self.report_period.label())
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.report_period, Period::Year, Period::Year.label());
                        for q in 1..=4 {
                            ui.selectable_value(&mut self.report_period, Period::Quarter(q), Period::Quarter(q).label());
                        }
                        for m in 1..=12 {
                            ui.selectable_value(&mut self.report_period, Period::Month(m), Period::Month(m).label());
                        }
                    });
                egui::ComboBox::from_id_salt("r_year")
                    .selected_text(self.report_year.to_string())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for y in &years {
                            ui.selectable_value(&mut self.report_year, *y, y.to_string());
                        }
                    });
                ui.label(muted(t("Zeitraum")));
            });
        });
        ui.add_space(10.0);
        let (from, to) = self.report_period.range(self.report_year);
        let r = period_report(&self.data, from, to);
        let range = format!("{} – {}", fmt_date(from), fmt_date(to));
        let mut open = None;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.report_tab {
            ReportTab::Euer => self.report_euer(ui, &r, &range),
            ReportTab::AnlageEuer => self.report_anlage_euer(ui),
            ReportTab::Ustva => self.report_ustva(ui, &r, &range),
            ReportTab::Zm => self.report_zm(ui, from, to, &range),
            ReportTab::Bilanz => self.report_statements(ui),
            ReportTab::OffenePosten => open = self.report_open_items(ui),
            ReportTab::Kunden => self.report_customers(ui, from, to, &range),
            ReportTab::Exporte => self.report_exports(ui, from, to, &range),
        });
        if open.is_some() {
            self.open_doc(open);
        }
    }

    fn report_euer(&self, ui: &mut Ui, r: &PeriodReport, range: &str) {
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, &tf!("Einnahmen-Überschuss-Rechnung · {}", range));
            ui.label(muted(t("Beträge netto nach Zahlungsdatum; Umsatzsteuer und Vorsteuer zählen wie in der Anlage EÜR mit. Privatentnahmen und -einlagen sind nicht enthalten.")));
            if r.afa > 0 {
                ui.label(muted(t("Abschreibungen der Anlagegüter sind enthalten.")));
            } else if !self.data.assets.is_empty() {
                ui.label(muted(t("Abschreibungen der Anlagegüter erscheinen in der Jahresansicht.")));
            }
            ui.add_space(6.0);
            egui::Grid::new("euer")
                .num_columns(2)
                .spacing([40.0, 6.0])
                .striped(true)
                .min_col_width(220.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(t("Betriebseinnahmen")).strong().color(GREEN));
                    ui.end_row();
                    for (k, v) in &r.income_by_cat {
                        ui.label(k);
                        money_cell(ui, RichText::new(fmt_money(*v)));
                        ui.end_row();
                    }
                    ui.label(muted(t("Vereinnahmte Umsatzsteuer")));
                    money_cell(ui, muted(fmt_money(r.vat_collected - r.rc_tax)));
                    ui.end_row();
                    ui.label(RichText::new(t("Summe Betriebseinnahmen")).strong());
                    money_cell(ui, RichText::new(fmt_money(r.income_net + r.vat_collected - r.rc_tax)).strong());
                    ui.end_row();
                    ui.label("");
                    ui.end_row();
                    ui.label(RichText::new(t("Betriebsausgaben")).strong().color(RED));
                    ui.end_row();
                    for (k, v) in &r.expense_by_cat {
                        ui.label(k);
                        money_cell(ui, RichText::new(fmt_money(*v)));
                        ui.end_row();
                    }
                    ui.label(muted(t("Gezahlte Vorsteuer")));
                    money_cell(ui, muted(fmt_money(r.vat_paid - r.rc_tax)));
                    ui.end_row();
                    ui.label(RichText::new(t("Summe Betriebsausgaben")).strong());
                    money_cell(ui, RichText::new(fmt_money(r.expense_net + r.vat_paid - r.rc_tax)).strong());
                    ui.end_row();
                    ui.label("");
                    ui.end_row();
                    let p = r.profit();
                    ui.label(RichText::new(if p >= 0 { t("Gewinn") } else { t("Verlust") }).strong().size(16.0));
                    money_cell(
                        ui,
                        RichText::new(fmt_money(p)).strong().size(16.0).color(if p >= 0 { GREEN } else { RED }),
                    );
                    ui.end_row();
                    ui.label(muted(t("Privatentnahmen / Privateinlagen")));
                    money_cell(ui, muted(format!("{} / {}", fmt_money(r.private_out), fmt_money(r.private_in))));
                    ui.end_row();
                });
        });
    }

    fn report_ustva(&self, ui: &mut Ui, r: &PeriodReport, range: &str) {
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, &tf!("Umsatzsteuer-Voranmeldung · {}", range));
            ui.label(muted(
                t("Werte nach vereinnahmten Entgelten (Ist-Versteuerung). Übertragen Sie die Werte in Ihr ELSTER-Formular."),
            ));
            if self.data.company.small_business {
                ui.label(RichText::new(t("Als Kleinunternehmer geben Sie in der Regel keine Voranmeldung ab.")).color(ORANGE));
            }
            ui.add_space(6.0);
            egui::Grid::new("ustva")
                .num_columns(4)
                .spacing([24.0, 7.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Kennzahl", "Bezeichnung", "Bemessungsgrundlage", "Steuer"] {
                        ui.label(muted(t(h)));
                    }
                    ui.end_row();
                    let rows = [
                        ("81", "Steuerpflichtige Umsätze 19 %", Some(r.base_19), Some(r.tax_19)),
                        ("86", "Steuerpflichtige Umsätze 7 %", Some(r.base_7), Some(r.tax_7)),
                        ("41", "Innergemeinschaftliche Lieferungen an Abnehmer mit USt-IdNr.", Some(r.eu_deliveries), None),
                        ("21", "Nicht steuerbare sonstige Leistungen in anderen EU-Ländern", Some(r.eu_services), None),
                        ("43", "Weitere steuerfreie Umsätze mit Vorsteuerabzug (z. B. Ausfuhr)", Some(r.exports), None),
                        ("45", "Übrige nicht steuerbare Umsätze (Leistungsort nicht im Inland)", Some(r.non_taxable), None),
                        ("48", "Steuerfreie Umsätze ohne Vorsteuerabzug", Some(r.base_0), None),
                        ("46/47", "Leistungen ausländischer Unternehmer (§ 13b), Steuer geschuldet", Some(r.rc_base), Some(r.rc_tax)),
                        ("66", "Abziehbare Vorsteuerbeträge", None, Some(r.vat_paid - r.rc_tax)),
                        ("67", "Vorsteuer aus Leistungen nach § 13b", None, Some(r.rc_tax)),
                    ];
                    for (kz, text, base, tax) in rows {
                        if base == Some(0) && tax.is_none_or(|t| t == 0) && !matches!(kz, "81" | "86" | "66") {
                            continue;
                        }
                        ui.label(RichText::new(kz).strong().color(ACCENT));
                        ui.label(t(text));
                        money_cell(ui, RichText::new(base.map(fmt_money).unwrap_or_default()));
                        money_cell(ui, RichText::new(tax.map(fmt_money).unwrap_or_default()));
                        ui.end_row();
                    }
                    ui.label(RichText::new("83").strong().color(ACCENT));
                    ui.label(
                        RichText::new(if r.vat_due() >= 0 {
                            t("Verbleibende Vorauszahlung")
                        } else {
                            t("Verbleibender Überschuss (Erstattung)")
                        })
                        .strong(),
                    );
                    ui.label("");
                    money_cell(ui, RichText::new(fmt_money(r.vat_due())).strong().size(15.0));
                    ui.end_row();
                });
        });
    }

    fn report_open_items(&self, ui: &mut Ui) -> Option<u64> {
        let small = self.data.company.small_business;
        let mut list: Vec<&Document> = self
            .data
            .documents
            .iter()
            .filter(|d| d.kind == DocKind::Rechnung && d.status == DocStatus::Offen)
            .collect();
        list.sort_by_key(|d| d.due_date());
        let cols = [
            col("Nummer", 1.2),
            col("Kunde", 2.2),
            col("Fällig", 1.0),
            col("Mahnstufe", 1.4),
            rcol("Betrag", 1.1),
        ];
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, t("Offene Rechnungen (alle Zeiträume)"));
            table_header(ui, &cols);
            if list.is_empty() {
                empty_hint(ui, t("Keine offenen Rechnungen."));
            }
            for d in &list {
                let days = (self.today - d.due_date()).num_days();
                // A simple reminder scale: 1st reminder after due date, 2nd after 14 days, 3rd after 28.
                let stage = match days {
                    i64::MIN..=0 => Cell::Chip("Offen".into()),
                    1..=14 => Cell::T(tf!("Zahlungserinnerung · {} Tage", days)),
                    15..=28 => Cell::T(tf!("1. Mahnung · {} Tage", days)),
                    _ => Cell::T(tf!("2. Mahnung · {} Tage", days)),
                };
                let cells = [
                    Cell::B(d.number.clone()),
                    Cell::T(self.data.contact_name(d.contact_id)),
                    Cell::T(fmt_date(d.due_date())),
                    stage,
                    if days > 0 {
                        Cell::MoneyColored(d.totals(small).gross, RED)
                    } else {
                        Cell::Money(d.totals(small).gross)
                    },
                ];
                if table_row(ui, &cols, &cells, false).clicked() {
                    open = Some(d.id);
                }
            }
            let sum: Cents = list.iter().map(|d| d.totals(small).gross).sum();
            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(fmt_money(sum)).strong());
                ui.label(muted(t("Summe offene Posten:")));
            });
        });
        open
    }

    fn report_customers(&self, ui: &mut Ui, from: NaiveDate, to: NaiveDate, range: &str) {
        let small = self.data.company.small_business;
        let mut sums: Vec<(String, Cents, usize)> = Vec::new();
        for c in self.data.contacts.iter().filter(|c| c.kind == ContactKind::Kunde) {
            let docs: Vec<&Document> = self
                .data
                .documents
                .iter()
                .filter(|d| {
                    d.contact_id == Some(c.id)
                        && matches!(d.kind, DocKind::Rechnung | DocKind::Gutschrift)
                        && matches!(d.status, DocStatus::Offen | DocStatus::Bezahlt)
                        && d.date >= from
                        && d.date <= to
                })
                .collect();
            let net: Cents = docs.iter().map(|d| d.totals(small).net).sum();
            if net != 0 {
                sums.push((c.display_name(), net, docs.len()));
            }
        }
        sums.sort_by_key(|s| -s.1);
        let max = sums.first().map(|s| s.1).unwrap_or(1).max(1);
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(780.0));
            card_title(ui, &tf!("Umsatz je Kunde (netto, nach Rechnungsdatum) · {}", range));
            if sums.is_empty() {
                empty_hint(ui, t("Keine Rechnungen in diesem Zeitraum."));
            }
            for (name, net, n) in sums {
                ui.horizontal(|ui| {
                    ui.add_sized([220.0, 22.0], egui::Label::new(RichText::new(name).strong()).truncate());
                    let w = 300.0 * net.max(0) as f32 / max as f32;
                    let (r, _) = ui.allocate_exact_size(vec2(300.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(4), Color32::from_rgb(0xee, 0xf2, 0xf5));
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(r.min, vec2(w, 14.0)),
                        CornerRadius::same(4),
                        INCOME,
                    );
                    ui.add_sized([110.0, 22.0], egui::Label::new(fmt_money(net)));
                    ui.label(muted(tf!("{} Rechnungen", n)));
                });
            }
        });
    }
}
