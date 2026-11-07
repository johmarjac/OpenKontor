//! Recurring invoices, e.g. maintenance contracts or rents.

use super::*;

const RECURRING_COLS: [Col; 6] = [
    col("Bezeichnung", 2.2),
    col("Kontakt", 1.8),
    col("Intervall", 1.0),
    col("Nächste Rechnung", 1.1),
    rcol("Betrag netto", 1.0),
    rcol("Status", 0.9),
];

fn interval_label(months: u32) -> String {
    match months {
        1 => t("monatlich").to_string(),
        3 => t("vierteljährlich").to_string(),
        6 => t("halbjährlich").to_string(),
        12 => t("jährlich").to_string(),
        n => tf!("alle {} Monate", n),
    }
}

impl App {
    pub(super) fn recurring_list(&mut self, ui: &mut Ui) {
        let small = self.data.company.small_business;
        let due = self.data.due_recurring(self.today);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(muted(t("Rechnungen, die regelmäßig neu geschrieben werden (z. B. Wartungsverträge, Mieten). Der Server erstellt fällige Rechnungen jeden Tag selbst.")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if due > 0 && primary(ui, &tf!("{} fällige jetzt erstellen", due)).clicked() && self.run(Op::CreateDueRecurring(self.today)).is_ok() {
                    self.info(t("Die Rechnungen wurden erstellt."));
                }
                if ui.button(t("+ Wiederkehrende Rechnung")).clicked() {
                    let doc = self.new_doc(DocKind::Rechnung);
                    self.recurring_edit = Some(Recurring { template: doc, ..Recurring::default() });
                }
            });
        });
        ui.add_space(8.0);
        let mut open = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            table_header(ui, &RECURRING_COLS);
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if self.data.recurring.is_empty() {
                    empty_hint(ui, t("Noch keine wiederkehrenden Rechnungen. Öffnen Sie eine Rechnung und wählen Sie „Weitere … → Als wiederkehrende Rechnung einrichten“."));
                }
                for r in &self.data.recurring {
                    let ended = r.until.is_some_and(|u| r.next_date > u);
                    let status = if !r.active || ended {
                        Cell::MutedColored(t("beendet").into(), MUTED)
                    } else if r.next_date <= self.today {
                        Cell::MutedColored(t("fällig").into(), ORANGE)
                    } else {
                        Cell::MutedColored(t("aktiv").into(), GREEN)
                    };
                    let cells = [
                        Cell::B(r.name.clone()),
                        Cell::T(self.data.contact_name(r.template.contact_id)),
                        Cell::T(interval_label(r.every_months)),
                        Cell::T(fmt_date(r.next_date)),
                        Cell::Money(r.template.totals(small).net),
                        status,
                    ];
                    if table_row(ui, &RECURRING_COLS, &cells, false).clicked() {
                        open = Some(r.clone());
                    }
                }
            });
        });
        if open.is_some() {
            self.recurring_edit = open;
        }
    }

    pub(super) fn recurring_dialog(&mut self, ctx: &egui::Context) {
        let mut r = self.recurring_edit.take().unwrap();
        let (mut keep, mut save, mut delete) = (true, false, false);
        let small = self.data.company.small_business;
        let modal = egui::Modal::new(Id::unique("recurring")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.label(RichText::new(t("Wiederkehrende Rechnung")).size(18.0).strong());
            ui.add_space(6.0);
            egui::Grid::new("rec_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                text_row(ui, t("Bezeichnung"), &mut r.name, 380.0);
                ui.label(muted(t("Kontakt")));
                egui::ComboBox::from_id_salt("rec_contact")
                    .width(380.0)
                    .selected_text(if r.template.contact_id.is_some() { self.data.contact_name(r.template.contact_id) } else { t("Bitte wählen …").into() })
                    .show_ui(ui, |ui| {
                        for c in self.data.contacts.iter().filter(|c| c.kind == ContactKind::Kunde) {
                            if ui.selectable_label(r.template.contact_id == Some(c.id), c.display_name()).clicked() {
                                r.template.contact_id = Some(c.id);
                                r.template.vat_case = c.vat_case;
                                r.template.apply_term(&self.data.term_for(Some(c.id)));
                            }
                        }
                    });
                ui.end_row();
                text_row(ui, t("Titel"), &mut r.template.title, 380.0);
                ui.label(muted(t("Intervall")));
                ui.horizontal(|ui| {
                    for m in [1, 3, 6, 12] {
                        ui.selectable_value(&mut r.every_months, m, interval_label(m));
                    }
                });
                ui.end_row();
                ui.label(muted(t("Nächste Rechnung am")));
                date_edit(ui, Id::unique("rec_next"), &mut r.next_date);
                ui.end_row();
                ui.label(muted(t("Endet")));
                ui.horizontal(|ui| {
                    let mut ends = r.until.is_some();
                    if ui.checkbox(&mut ends, t("am")).changed() {
                        r.until = ends.then(|| r.next_date.checked_add_months(chrono::Months::new(12)).unwrap_or(r.next_date));
                    }
                    if let Some(u) = &mut r.until {
                        date_edit(ui, Id::unique("rec_until"), u);
                    } else {
                        ui.label(muted(t("unbefristet")));
                    }
                });
                ui.end_row();
                ui.label(muted(t("Erstellen als")));
                ui.vertical(|ui| {
                    ui.radio_value(&mut r.finalize, false, t("Entwurf (vor dem Versand prüfen)"));
                    ui.radio_value(&mut r.finalize, true, t("fertige Rechnung mit Nummer"));
                });
                ui.end_row();
                ui.label(muted(t("Aktiv")));
                ui.checkbox(&mut r.active, t("Rechnungen weiter erstellen"));
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(RichText::new(t("Positionen")).strong());
            let mut remove = None;
            egui::Grid::new("rec_lines").num_columns(5).spacing([6.0, 6.0]).show(ui, |ui| {
                for (i, l) in r.template.lines.iter_mut().enumerate() {
                    let id = Id::unique(("rec_line", i));
                    ui.add(TextEdit::singleline(&mut l.text).desired_width(280.0).hint_text(t("Bezeichnung")));
                    qty_edit(ui, id.with("q"), &mut l.qty, 50.0);
                    money_edit(ui, id.with("p"), &mut l.price_net, 90.0);
                    if small {
                        ui.label("");
                    } else {
                        vat_combo(ui, ("rec_vat", i), &mut l.vat);
                    }
                    if crate::icons::icon_only_button(ui, Icon::Close, t("Position entfernen")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                r.template.lines.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.button(t("+ Freie Position")).clicked() {
                    r.template.lines.push(Line::default());
                }
                let mut pick: Option<Article> = None;
                egui::ComboBox::from_id_salt("rec_article").selected_text(t("+ Artikel einfügen …")).width(220.0).show_ui(ui, |ui| {
                    for a in &self.data.articles {
                        if ui.selectable_label(false, format!("{} – {}", a.name, fmt_money(a.price_net))).clicked() {
                            pick = Some(a.clone());
                        }
                    }
                });
                if let Some(a) = pick {
                    r.template.lines.retain(|l| !(l.text.is_empty() && l.price_net == 0));
                    r.template.lines.push(Line {
                        article_id: Some(a.id),
                        text: a.name.clone(),
                        qty: 1.0,
                        unit: a.unit.clone(),
                        price_net: a.price_net,
                        vat: a.vat,
                        discount_pct: 0.0,
                    });
                }
                ui.label(RichText::new(tf!("Summe netto {}", fmt_money(r.template.totals(small).net))).strong());
            });
            if !r.created.is_empty() {
                ui.label(muted(tf!("Bisher {} Rechnungen erstellt.", r.created.len())));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, t("Speichern")).clicked() {
                    save = true;
                }
                if ui.button(t("Abbrechen")).clicked() {
                    keep = false;
                }
                if r.id != 0 {
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
        if save && self.run(Op::SaveRecurring(Box::new(r.clone()))).is_ok() {
            self.info(t("Wiederkehrende Rechnung gespeichert."));
            self.doc_view_recurring = true;
            self.doc_edit = None;
            keep = false;
        }
        if delete {
            self.confirm = Some((tf!("Wiederkehrende Rechnung „{}“ beenden und löschen? Schon erstellte Rechnungen bleiben.", r.name), Op::DeleteRecurring(r.id)));
            keep = false;
        }
        if keep {
            self.recurring_edit = Some(r);
        }
    }
}
