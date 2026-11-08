//! Projects and time tracking: record hours per project and bill them as an invoice.

use super::*;

#[derive(Default)]
pub struct ProjectsState {
    pub selected: Option<u64>,
    pub show_inactive: bool,
    pub edit: Option<Project>,
    /// The time entry being written or changed.
    pub entry: Option<TimeEntry>,
    /// Unbilled entries ticked for billing.
    pub ticked: std::collections::BTreeSet<u64>,
}

const PROJECT_COLS: [Col; 5] = [col("Projekt", 2.0), col("Kunde", 1.5), rcol("Stunden", 1.1), rcol("Offen", 0.9), rcol("Budget", 0.9)];
const TIME_COLS: [Col; 5] = [col("Datum", 1.15), col("Tätigkeit", 3.0), col("Wer", 0.9), rcol("Stunden", 0.7), rcol("Abgerechnet", 1.1)];

impl App {
    fn user_name(&self) -> String {
        match &self.backend {
            Backend::Remote(r) => r.user.clone(),
            _ => std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default(),
        }
    }

    pub(super) fn projects_page(&mut self, ui: &mut Ui) {
        let times = self.data.times.clone();
        let hours = move |id: u64, billed: Option<bool>| -> f64 {
            times
                .iter()
                .filter(|t| t.project_id == id && billed.is_none_or(|b| t.invoice_id.is_some() == b))
                .map(|t| t.hours)
                .sum()
        };
        toolbar(ui, "projects", |ui, side| match side {
            Side::Left => {
            ui.checkbox(&mut self.projects.show_inactive, t("Beendete anzeigen"));
            }
            Side::Right => {
                if primary(ui, t("+ Projekt")).clicked() {
                    self.projects.edit = Some(Project::default());
                }
                if !self.data.projects.is_empty() && ui.button(t("+ Zeit erfassen")).clicked() {
                    let project_id = self.projects.selected.or(self.data.projects.iter().find(|p| p.active).map(|p| p.id)).unwrap_or(0);
                    self.projects.entry = Some(TimeEntry { project_id, user: self.user_name(), date: self.today, ..TimeEntry::default() });
                }
            }
        });
        ui.add_space(8.0);
        let list: Vec<Project> = self.data.projects.iter().filter(|p| p.active || self.projects.show_inactive).cloned().collect();
        if self.projects.selected.is_none() {
            self.projects.selected = list.first().map(|p| p.id);
        }
        let mut pick = None;
        let mut edit = None;
        let customers: Vec<String> = list.iter().map(|p| self.data.contact_name(p.contact_id)).collect();
        let current = self.projects.selected;
        let mut list_card = |ui: &mut Ui, width: f32, height: f32| {
            card(ui, |ui| {
                ui.set_width(width - 34.0);
                table_header(ui, &PROJECT_COLS);
                ScrollArea::vertical().id_salt("projects").auto_shrink(false).max_height(height).show(ui, |ui| {
                    if list.is_empty() {
                        empty_hint(ui, t("Noch keine Projekte. Legen Sie ein Projekt an und erfassen Sie Ihre Stunden – abgerechnet wird mit einem Klick."));
                    }
                    for (p, customer) in list.iter().zip(&customers) {
                        let (all, open) = (hours(p.id, None), hours(p.id, Some(false)));
                        let budget = if p.budget_hours > 0.0 {
                            let pct = all / p.budget_hours * 100.0;
                            Cell::MutedColored(format!("{:.0} %", pct), if pct > 100.0 { RED } else if pct > 80.0 { ORANGE } else { MUTED })
                        } else {
                            Cell::Muted("–".into())
                        };
                        let cells = [
                            Cell::B(if p.active { p.name.clone() } else { tf!("{} (beendet)", p.name) }),
                            Cell::T(customer.clone()),
                            Cell::T(fmt_qty(all)),
                            if open > 0.0 { Cell::MutedColored(fmt_qty(open), ACCENT) } else { Cell::Muted("0".into()) },
                            budget,
                        ];
                        let r = table_row(ui, &PROJECT_COLS, &cells, current == Some(p.id));
                        if r.double_clicked() {
                            edit = Some(p.clone());
                        } else if r.clicked() {
                            pick = Some(p.id);
                        }
                    }
                });
            });
        };
        let selected = self.projects.selected.and_then(|id| self.data.projects.iter().find(|p| p.id == id)).cloned();
        let total = ui.available_width();
        if total >= 950.0 {
            // Wide: projects on the left, the chosen project's times on the right.
            let left = (total * 0.4).clamp(360.0, 520.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(left);
                    let h = ui.available_height() - 20.0;
                    list_card(ui, left, h);
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    if let Some(p) = &selected {
                        self.project_detail(ui, p);
                    }
                });
            });
        } else {
            // Narrow: one below the other.
            list_card(ui, total, 150.0);
            ui.add_space(12.0);
            if let Some(p) = &selected {
                self.project_detail(ui, p);
            }
        }
        if let Some(id) = pick {
            self.projects.selected = Some(id);
            self.projects.ticked.clear();
        }
        if edit.is_some() {
            self.projects.edit = edit;
        }
    }

    fn project_detail(&mut self, ui: &mut Ui, p: &Project) {
        let entries: Vec<TimeEntry> = self.data.times.iter().filter(|t| t.project_id == p.id).rev().cloned().collect();
        let open_hours: f64 = entries.iter().filter(|t| t.invoice_id.is_none()).map(|t| t.hours).sum();
        let mut bill = false;
        let mut open_entry = None;
        let mut open_doc = None;
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                card_title(ui, &p.name);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t("Bearbeiten")).clicked() {
                        self.projects.edit = Some(p.clone());
                    }
                });
            });
            ui.label(muted(tf!(
                "{} · {} je Stunde · {} Stunden noch nicht abgerechnet ({})",
                self.data.contact_name(p.contact_id),
                fmt_money(p.rate),
                fmt_qty(open_hours),
                fmt_money((open_hours * p.rate as f64).round() as Cents)
            )));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let unbilled: Vec<u64> = entries.iter().filter(|t| t.invoice_id.is_none()).map(|t| t.id).collect();
                if !unbilled.is_empty() && ui.button(t("Alle offenen auswählen")).clicked() {
                    self.projects.ticked = unbilled.iter().copied().collect();
                }
                let n = self.projects.ticked.len();
                if primary_enabled(ui, n > 0, &tf!("{} Zeiten abrechnen", n)).on_hover_text(t("Legt einen Rechnungsentwurf mit den ausgewählten Zeiten an.")).clicked() {
                    bill = true;
                }
            });
            ui.add_space(6.0);
            table_header(ui, &TIME_COLS);
            ScrollArea::vertical().id_salt("times").auto_shrink(false).show(ui, |ui| {
                if entries.is_empty() {
                    empty_hint(ui, t("Noch keine Zeiten erfasst."));
                }
                for e in &entries {
                    let ticked = self.projects.ticked.contains(&e.id);
                    let billed = e.invoice_id.and_then(|id| self.data.documents.iter().find(|d| d.id == id));
                    let cells = [
                        Cell::T(fmt_date(e.date)),
                        Cell::T(e.text.clone()),
                        Cell::Muted(e.user.clone()),
                        Cell::T(fmt_qty(e.hours)),
                        match billed {
                            Some(d) => Cell::Muted(if d.number.is_empty() { t("Entwurf").to_string() } else { d.number.clone() }),
                            None => Cell::Muted("–".into()),
                        },
                    ];
                    let r = table_row(ui, &TIME_COLS, &cells, ticked);
                    if r.clicked() {
                        match e.invoice_id {
                            Some(id) => open_doc = Some(id),
                            None if ui.input(|i| i.modifiers.command || i.modifiers.shift) || !self.projects.ticked.is_empty() => {
                                if !self.projects.ticked.remove(&e.id) {
                                    self.projects.ticked.insert(e.id);
                                }
                            }
                            None => {
                                self.projects.ticked.insert(e.id);
                            }
                        }
                    }
                    if r.double_clicked() && e.invoice_id.is_none() {
                        open_entry = Some(e.clone());
                    }
                }
            });
            ui.label(muted(t("Klicken wählt Zeiten zum Abrechnen aus, Doppelklick bearbeitet sie.")));
        });
        if bill {
            let ids: Vec<u64> = self.projects.ticked.iter().copied().collect();
            if let Ok(Some(id)) = self.run(Op::BillTimes(p.id, ids)) {
                self.projects.ticked.clear();
                self.info(t("Rechnungsentwurf angelegt – bitte prüfen und fertigstellen."));
                self.open_doc(Some(id));
            }
        }
        if open_entry.is_some() {
            self.projects.entry = open_entry;
        }
        if let Some(id) = open_doc {
            self.open_doc(Some(id));
        }
    }

    pub(super) fn project_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut p) = self.projects.edit.take() {
            let (mut keep, mut save, mut delete) = (true, false, false);
            let modal = egui::Modal::new(Id::unique("project")).show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(RichText::new(if p.id == 0 { t("Neues Projekt") } else { t("Projekt") }).size(18.0).strong());
                egui::Grid::new("project_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    text_row(ui, t("Projektname"), &mut p.name, 320.0);
                    ui.label(muted(t("Kunde")));
                    egui::ComboBox::from_id_salt("project_contact")
                        .width(320.0)
                        .selected_text(if p.contact_id.is_some() { self.data.contact_name(p.contact_id) } else { t("Bitte wählen …").into() })
                        .show_ui(ui, |ui| {
                            for c in self.data.contacts.iter().filter(|c| c.kind == ContactKind::Kunde) {
                                ui.selectable_value(&mut p.contact_id, Some(c.id), c.display_name());
                            }
                        });
                    ui.end_row();
                    ui.label(muted(t("Stundensatz netto")));
                    ui.horizontal(|ui| {
                        money_edit(ui, Id::unique("project_rate"), &mut p.rate, 100.0);
                        ui.label("€");
                    });
                    ui.end_row();
                    ui.label(muted(t("Budget")));
                    ui.horizontal(|ui| {
                        qty_edit(ui, Id::unique("project_budget"), &mut p.budget_hours, 70.0);
                        ui.label(muted(t("Stunden (0 = ohne)")));
                    });
                    ui.end_row();
                    ui.label(muted(t("Aktiv")));
                    ui.checkbox(&mut p.active, t("Zeiten können erfasst werden"));
                    ui.end_row();
                    ui.label(muted(t("Notiz")));
                    ui.add(TextEdit::multiline(&mut p.note).desired_rows(2).desired_width(320.0));
                    ui.end_row();
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if primary(ui, t("Speichern")).clicked() {
                        save = true;
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        keep = false;
                    }
                    if p.id != 0 {
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
            if save
                && let Ok(id) = self.run(Op::SaveProject(p.clone()))
            {
                self.projects.selected = id;
                keep = false;
            }
            if delete {
                self.confirm = Some((tf!("Projekt „{}“ löschen?", p.name), Op::DeleteProject(p.id)));
                keep = false;
            }
            if keep {
                self.projects.edit = Some(p);
            }
        }
        if let Some(mut e) = self.projects.entry.take() {
            let (mut keep, mut save, mut delete) = (true, false, false);
            let modal = egui::Modal::new(Id::unique("time_entry")).show(ctx, |ui| {
                ui.set_width(480.0);
                ui.label(RichText::new(t("Zeit erfassen")).size(18.0).strong());
                egui::Grid::new("time_grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    ui.label(muted(t("Projekt")));
                    let name = self.data.projects.iter().find(|p| p.id == e.project_id).map(|p| p.name.clone()).unwrap_or_else(|| t("Bitte wählen …").into());
                    egui::ComboBox::from_id_salt("time_project").width(300.0).selected_text(name).show_ui(ui, |ui| {
                        for p in self.data.projects.iter().filter(|p| p.active) {
                            ui.selectable_value(&mut e.project_id, p.id, &p.name);
                        }
                    });
                    ui.end_row();
                    ui.label(muted(t("Datum")));
                    date_edit(ui, Id::unique("time_date"), &mut e.date);
                    ui.end_row();
                    ui.label(muted(t("Stunden")));
                    ui.horizontal(|ui| {
                        qty_edit(ui, Id::unique("time_hours"), &mut e.hours, 60.0);
                        for h in [0.25, 0.5, 1.0, 2.0, 4.0, 8.0] {
                            if ui.small_button(fmt_qty(h)).clicked() {
                                e.hours = h;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(muted(t("Tätigkeit")));
                    ui.add(TextEdit::singleline(&mut e.text).hint_text(t("z. B. Abstimmung Entwurf")).desired_width(300.0));
                    ui.end_row();
                });
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
            if save && self.run(Op::SaveTime(e.clone())).is_ok() {
                self.projects.selected = Some(e.project_id);
                self.info(t("Zeit gespeichert."));
                keep = false;
            }
            if delete {
                self.confirm = Some((t("Diese Zeit löschen?").into(), Op::DeleteTime(e.id)));
                keep = false;
            }
            if keep {
                self.projects.entry = Some(e);
            }
        }
    }
}
