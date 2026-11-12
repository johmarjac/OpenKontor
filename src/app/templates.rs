//! Settings for templates: how PDFs look, the texts documents start with, e-mail texts.

use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TemplatesTab {
    #[default]
    Gestaltung,
    Texte,
    Email,
    Weiteres,
}

/// What is being edited on the templates page.
#[derive(Default)]
pub struct TemplatesState {
    pub tab: TemplatesTab,
    /// "" = all documents, else a kind from TEMPLATE_KINDS.
    pub layout_key: String,
    /// The layout being edited: for which key, whether the kind has its own, the layout.
    pub layout: Option<(String, bool, PdfLayout)>,
    pub text_kind: usize,
    pub text_lang: Lang,
    /// Title, opening and closing text being edited, for (kind, language).
    pub text: Option<(usize, Lang, String, String, String)>,
    /// File name pattern, offer days, e-mail signature being edited.
    pub general: Option<(String, u32, String)>,
}

/// Lets the person pick a picture and makes it fit `max_w` × `max_h` pixels.
fn pick_picture(max_w: u32, max_h: u32) -> Option<Result<Logo, String>> {
    let path = rfd::FileDialog::new().add_filter(t("Bild"), &["png", "jpg", "jpeg"]).pick_file()?;
    Some((|| {
        let img = image::open(&path).map_err(|e| tf!("Das Bild ist nicht lesbar: {}", e))?;
        let img = img.thumbnail(max_w, max_h).to_rgba8();
        let (w, h) = img.dimensions();
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for p in img.pixels() {
            let a = p[3] as u32;
            for c in 0..3 {
                rgb.push(((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        Ok(crate::pdf::make_logo(w, h, &rgb))
    })())
}

/// A picture setting: what is set, and buttons to choose or remove it.
fn picture_row(ui: &mut Ui, label: &'static str, value: &mut Option<Logo>, max: (u32, u32), hint: &'static str, error: &mut Option<String>) {
    ui.label(muted(t(label)));
    ui.horizontal_wrapped(|ui| {
        match value {
            Some(l) => {
                ui.label(tf!("{} × {} Pixel", l.width, l.height));
                if ui.button(t("Entfernen")).clicked() {
                    *value = None;
                }
            }
            None => {
                ui.label(muted(t("keines")));
            }
        }
        if ui.button(t("Bild wählen …")).on_hover_text(t(hint)).clicked() {
            match pick_picture(max.0, max.1) {
                Some(Ok(l)) => *value = Some(l),
                Some(Err(e)) => *error = Some(e),
                None => {}
            }
        }
    });
    ui.end_row();
}

fn combo<T: Copy + PartialEq>(ui: &mut Ui, salt: &str, value: &mut T, all: &[T], name: fn(T) -> &'static str, width: f32) {
    egui::ComboBox::from_id_salt(salt).width(width).selected_text(t(name(*value))).show_ui(ui, |ui| {
        for x in all {
            ui.selectable_value(value, *x, t(name(*x)));
        }
    });
}

/// The layout form. Returns an error message when a picture could not be read.
fn layout_form(ui: &mut Ui, lay: &mut PdfLayout, width: f32) -> Option<String> {
    let mut error = None;
    let field = (width - 190.0).clamp(160.0, 320.0);
    ui.label(RichText::new(t("Briefkopf")).strong());
    egui::Grid::new("lay_head").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
        ui.label(muted(t("Kopfbereich")));
        combo(ui, "lay_header", &mut lay.header, &HeaderStyle::ALL, HeaderStyle::name, field);
        ui.end_row();
        ui.label(muted(t("Logo")));
        ui.horizontal(|ui| {
            combo(ui, "lay_logo_pos", &mut lay.logo_pos, &LogoPos::ALL, LogoPos::name, 100.0);
            ui.add(egui::Slider::new(&mut lay.logo_height, 8.0..=34.0).suffix(" mm").text(t("hoch")));
        });
        ui.end_row();
        ui.label(muted(t("Farbe")));
        ui.horizontal(|ui| {
            ui.color_edit_button_srgb(&mut lay.accent);
            ui.checkbox(&mut lay.header_rule, t("Linie unter dem Briefkopf"));
        });
        ui.end_row();
        ui.label(muted(t("Zusatzzeile")));
        ui.add(TextEdit::singleline(&mut lay.tagline).desired_width(field).hint_text(t("z. B. Ihr Slogan")));
        ui.end_row();
        picture_row(ui, "Briefpapier", &mut lay.background, (1240, 1754), "Ein Bild der ganzen Seite (A4 hochkant), das hinter dem Text liegt – z. B. Ihr gestaltetes Briefpapier.", &mut error);
        if lay.background.is_some() {
            ui.label("");
            ui.checkbox(&mut lay.background_first_page_only, t("nur auf der ersten Seite"));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t("Seite & Schrift")).strong());
    egui::Grid::new("lay_page").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
        ui.label(muted(t("Anschriftfeld")));
        combo(ui, "lay_address", &mut lay.address, &AddressPos::ALL, AddressPos::name, field);
        ui.end_row();
        ui.label(muted(t("Absenderzeile")));
        ui.add(TextEdit::singleline(&mut lay.sender_line).desired_width(field).hint_text(t("automatisch aus den Firmendaten")));
        ui.end_row();
        ui.label(muted(t("Schriftgröße")));
        ui.add(egui::Slider::new(&mut lay.font_size, 8.0..=12.0).step_by(0.5).suffix(" pt"));
        ui.end_row();
        ui.label(muted(t("Rand links")));
        ui.add(egui::Slider::new(&mut lay.margin_left, 15.0..=35.0).suffix(" mm"));
        ui.end_row();
        ui.label(muted(t("Rand rechts")));
        ui.add(egui::Slider::new(&mut lay.margin_right, 10.0..=30.0).suffix(" mm"));
        ui.end_row();
        ui.label(muted(t("Anzeigen")));
        ui.vertical(|ui| {
            ui.checkbox(&mut lay.show_position, t("Positionsnummern"));
            ui.checkbox(&mut lay.show_vat_column, t("Spalte Umsatzsteuer"));
            ui.checkbox(&mut lay.show_customer_number, t("Kundennummer im Kopf"));
            ui.checkbox(&mut lay.page_numbers, t("Seitenzahlen"));
            ui.checkbox(&mut lay.fold_marks, t("Falz- und Lochmarken"));
        });
        ui.end_row();
        picture_row(ui, "Unterschrift", &mut lay.signature, (900, 300), "Eine eingescannte Unterschrift oder ein Stempel unter dem Schlusstext.", &mut error);
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t("Fußzeile")).strong());
    combo(ui, "lay_footer", &mut lay.footer, &FooterStyle::ALL, FooterStyle::name, field);
    if lay.footer == FooterStyle::Eigener {
        lay.footer_columns.resize(4, String::new());
        let col = ((width - 24.0) / 2.0).max(140.0);
        egui::Grid::new("lay_footer_cols").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
            for (i, c) in lay.footer_columns.iter_mut().enumerate() {
                ui.add(TextEdit::multiline(c).desired_rows(3).desired_width(col).hint_text(tf!("Spalte {}", i + 1)));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.label(muted(t("Platzhalter wie {firma}, {iban}, {telefon} werden ausgefüllt.")));
    }
    error
}

/// A small copy of the data with one document to show a layout on.
fn sample(data: &Data, company: Company, key: &str, today: NaiveDate) -> (Data, Document) {
    let kinds = [DocKind::Rechnung, DocKind::Gutschrift, DocKind::Angebot, DocKind::Auftragsbestaetigung, DocKind::Lieferschein, DocKind::Bestellung];
    let kind = kinds.iter().copied().find(|k| k.name() == key).unwrap_or(DocKind::Rechnung);
    let found = data
        .documents
        .iter()
        .rev()
        .find(|d| d.kind == kind && !d.lines.is_empty() && d.contact_id.is_some())
        .or_else(|| data.documents.iter().rev().find(|d| !d.lines.is_empty() && d.contact_id.is_some()))
        .cloned();
    let mut doc = found.unwrap_or_else(|| Document {
        id: 1,
        number: String::new(),
        date: today,
        contact_id: data.contacts.first().map(|c| c.id),
        lines: vec![
            Line { text: t("Beratung und Konzeption").into(), qty: 4.0, unit: t("Std.").into(), price_net: 9500, ..Line::default() },
            Line { text: t("Umsetzung laut Angebot").into(), qty: 1.0, unit: t("Pausch.").into(), price_net: 120000, ..Line::default() },
        ],
        ..Document::default()
    });
    if doc.kind != kind {
        let lang = data.contact(doc.contact_id).map(|c| c.language).unwrap_or_default();
        let (title, intro, outro) = company.doc_text(kind.name(), lang);
        doc.kind = kind;
        doc.title = title;
        doc.intro = intro;
        doc.outro = outro;
        doc.number = String::new();
    }
    let mut small = Data { company, ..Data::default() };
    small.contacts = data.contacts.iter().filter(|c| Some(c.id) == doc.contact_id).cloned().collect();
    small.documents = data.documents.iter().filter(|d| Some(d.id) == doc.source_id).cloned().collect();
    small.documents.push(doc.clone());
    (small, doc)
}

impl App {
    pub(super) fn settings_templates(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let tab = &mut self.templates.tab;
            ui.selectable_value(tab, TemplatesTab::Gestaltung, t("PDF-Gestaltung"));
            ui.selectable_value(tab, TemplatesTab::Texte, t("Belegtexte"));
            ui.selectable_value(tab, TemplatesTab::Email, t("E-Mail-Texte"));
            ui.selectable_value(tab, TemplatesTab::Weiteres, t("Dateinamen & Weiteres"));
        });
        ui.add_space(8.0);
        match self.templates.tab {
            TemplatesTab::Gestaltung => self.templates_layout(ui),
            TemplatesTab::Texte => self.templates_texts(ui),
            TemplatesTab::Email => self.settings_mail_templates(ui),
            TemplatesTab::Weiteres => self.templates_general(ui),
        }
    }

    fn templates_layout(&mut self, ui: &mut Ui) {
        let can = self.backend.can_write();
        let key = self.templates.layout_key.clone();
        let (mut own, mut lay) = match self.templates.layout.take() {
            Some((k, own, lay)) if k == key => (own, lay),
            _ => (key.is_empty() || self.data.company.pdf_layouts.contains_key(&key), self.data.company.layout(&key)),
        };
        let mut new_key = key.clone();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut new_key, String::new(), t("Alle Belege"));
            for (k, label) in TEMPLATE_KINDS {
                ui.selectable_value(&mut new_key, k.to_string(), t(label));
            }
        });
        if new_key != key {
            self.templates.layout_key = new_key;
            return;
        }
        let total = ui.available_width();
        let wide = total >= 1100.0;
        let form_w = if wide { (total - 480.0).min(620.0) } else { total.min(760.0) };
        let mut save = false;
        let mut reset = false;
        let mut error = None;
        let standard = self.data.company.layout("");
        // The preview shows the layout being edited on a real document.
        let preview_layouts = |c: &mut Company, own: bool, lay: &PdfLayout| {
            if own {
                c.pdf_layouts.insert(key.clone(), lay.clone());
            } else {
                c.pdf_layouts.retain(|k, _| *k != key);
            }
        };
        let pages = |own: bool, lay: &PdfLayout| {
            let mut company = self.data.company.clone();
            preview_layouts(&mut company, own, lay);
            let (data, doc) = sample(&self.data, company, &key, self.today);
            if key == "Mahnung" {
                let fee = data.company.reminder_fees.get(1).copied().unwrap_or(0);
                let r = ReminderRecord { level: 1, date: self.today, fee, due: self.today + chrono::Duration::days(7) };
                crate::pdf::reminder_preview(&data, &doc, &r)
            } else {
                crate::pdf::document_preview(&data, &doc)
            }
        };
        let shown = pages(own, &lay);
        let mut form = |ui: &mut Ui| {
            card(ui, |ui| {
                ui.set_width(form_w - 30.0);
                if key.is_empty() {
                    card_title(ui, t("Gestaltung für alle Belege"));
                    ui.label(muted(t("Gilt für alle Belege, Mahnungen und Auswertungen – außer eine Belegart hat eine eigene Gestaltung.")));
                } else {
                    let label = TEMPLATE_KINDS.iter().find(|(k, _)| *k == key).map(|(_, l)| *l).unwrap_or("");
                    card_title(ui, &tf!("Gestaltung: {}", t(label)));
                    if ui.add_enabled(can, egui::Checkbox::new(&mut own, t("Eigene Gestaltung für diese Belegart"))).changed() && own {
                        lay = standard.clone();
                    }
                    if !own {
                        ui.label(muted(t("Verwendet die Gestaltung für alle Belege.")));
                    }
                }
                ui.add_space(4.0);
                if own {
                    ui.add_enabled_ui(can, |ui| error = layout_form(ui, &mut lay, form_w - 30.0));
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if primary_enabled(ui, can, t("Speichern")).clicked() {
                        save = true;
                    }
                    if ui.button(t("Verwerfen")).clicked() {
                        reset = true;
                    }
                    if own && ui.add_enabled(can, egui::Button::new(t("Standard wiederherstellen"))).clicked() {
                        lay = PdfLayout::default();
                    }
                });
            });
        };
        if wide {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(form_w, 0.0), Layout::top_down(Align::Min), |ui| form(ui));
                ui.add_space(16.0);
                let w = (ui.available_width() - 8.0).min(560.0);
                ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.label(muted(t("Vorschau")));
                    super::pdf_view::show_pages(ui, &shown[..shown.len().min(2)], w);
                });
            });
        } else {
            form(ui);
            ui.add_space(12.0);
            ui.label(muted(t("Vorschau")));
            super::pdf_view::show_pages(ui, &shown[..shown.len().min(2)], total.min(560.0) - 8.0);
        }
        if let Some(e) = error {
            self.error(e);
        }
        if save {
            let mut c = self.data.company.clone();
            preview_layouts(&mut c, own, &lay);
            if self.run(Op::SaveCompany(c)).is_ok() {
                self.info(t("Gestaltung gespeichert."));
            }
            return;
        }
        if !reset {
            self.templates.layout = Some((key, own, lay));
        }
    }

    fn templates_texts(&mut self, ui: &mut Ui) {
        let can = self.backend.can_write();
        let st = &mut self.templates;
        let (kind_i, lang) = (st.text_kind.min(TEXT_KINDS.len() - 1), st.text_lang);
        let key = TEXT_KINDS[kind_i].0;
        let (mut title, mut intro, mut outro) = match st.text.take() {
            Some((k, l, a, b, c)) if k == kind_i && l == lang => (a, b, c),
            _ => self.data.company.doc_text(key, lang),
        };
        let mut save = false;
        let mut restore = false;
        let mut reset = false;
        let mut new_kind = kind_i;
        let mut new_lang = lang;
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Belegtexte"));
            ui.label(muted(t("Titel, Einleitung und Schlusstext, mit denen neue Belege beginnen. Platzhalter werden beim Erstellen des PDFs ausgefüllt.")));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("text_kind").width(220.0).selected_text(t(TEXT_KINDS[kind_i].1)).show_ui(ui, |ui| {
                    for (i, (_, label)) in TEXT_KINDS.iter().enumerate() {
                        ui.selectable_value(&mut new_kind, i, t(label));
                    }
                });
                egui::ComboBox::from_id_salt("text_lang").selected_text(new_lang.native_name()).show_ui(ui, |ui| {
                    for x in Lang::ALL {
                        ui.selectable_value(&mut new_lang, x, x.native_name());
                    }
                });
            });
            ui.add_enabled_ui(can, |ui| {
                ui.label(muted(t("Titel")));
                ui.add(TextEdit::singleline(&mut title).desired_width(f32::INFINITY));
                ui.label(muted(t("Einleitung")));
                ui.add(TextEdit::multiline(&mut intro).desired_rows(4).desired_width(f32::INFINITY));
                ui.label(muted(t("Schlusstext")));
                ui.add(TextEdit::multiline(&mut outro).desired_rows(3).desired_width(f32::INFINITY));
                ui.horizontal_wrapped(|ui| {
                    if primary(ui, t("Text speichern")).clicked() {
                        save = true;
                    }
                    if ui.button(t("Verwerfen")).clicked() {
                        reset = true;
                    }
                    if ui.button(t("Standardtext wiederherstellen")).clicked() {
                        restore = true;
                    }
                });
            });
            placeholder_list(ui);
        });
        if new_kind != kind_i || new_lang != lang {
            self.templates.text_kind = new_kind;
            self.templates.text_lang = new_lang;
            return;
        }
        if save || restore {
            let mut c = self.data.company.clone();
            c.doc_texts.retain(|x| !(x.kind == key && x.lang == lang));
            if save {
                c.doc_texts.push(DocText { kind: key.to_string(), lang, title, intro, outro });
            }
            if self.run(Op::SaveCompany(c)).is_ok() {
                self.info(t("Belegtext gespeichert."));
            }
            return;
        }
        if !reset {
            self.templates.text = Some((kind_i, lang, title, intro, outro));
        }
    }

    fn templates_general(&mut self, ui: &mut Ui) {
        let can = self.backend.can_write();
        let c = &self.data.company;
        let (mut pattern, mut days, mut signature) =
            self.templates.general.take().unwrap_or_else(|| (c.file_name_pattern.clone(), c.offer_valid_days, c.email_signature.clone()));
        let mut save = false;
        let mut reset = false;
        let example = self.data.documents.iter().rev().find(|d| !d.number.is_empty()).cloned().unwrap_or_default();
        let mut preview_company = c.clone();
        preview_company.file_name_pattern = pattern.clone();
        let example_name = preview_company.pdf_file_name(&example, &self.data.contact_name(example.contact_id));
        card(ui, |ui| {
            ui.set_width(ui.available_width().min(840.0));
            card_title(ui, t("Dateinamen & Weiteres"));
            ui.add_enabled_ui(can, |ui| {
                egui::Grid::new("tpl_general").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    ui.label(muted(t("Dateiname der PDFs")));
                    ui.vertical(|ui| {
                        ui.add(TextEdit::singleline(&mut pattern).desired_width(320.0).hint_text("{art} {nummer}"));
                        ui.label(muted(t("Platzhalter: {art}, {nummer}, {kunde}, {datum}, {jahr}, {firma}")));
                        ui.label(muted(tf!("Beispiel: {}", example_name)));
                    });
                    ui.end_row();
                    ui.label(muted(t("Angebote gültig")));
                    ui.add(egui::DragValue::new(&mut days).range(1..=365).suffix(t(" Tage")));
                    ui.end_row();
                    ui.label(muted(t("E-Mail-Signatur")));
                    ui.vertical(|ui| {
                        ui.add(TextEdit::multiline(&mut signature).desired_rows(5).desired_width(420.0));
                        ui.label(muted(t("Erscheint in E-Mail-Texten an der Stelle von {signatur}.")));
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if primary(ui, t("Speichern")).clicked() {
                        save = true;
                    }
                    if ui.button(t("Verwerfen")).clicked() {
                        reset = true;
                    }
                });
            });
        });
        if save {
            let mut c = self.data.company.clone();
            c.file_name_pattern = pattern;
            c.offer_valid_days = days;
            c.email_signature = signature;
            if self.run(Op::SaveCompany(c)).is_ok() {
                self.info(t("Einstellungen gespeichert."));
            }
            return;
        }
        if !reset {
            self.templates.general = Some((pattern, days, signature));
        }
    }
}

/// The placeholders, folded away until needed.
pub(super) fn placeholder_list(ui: &mut Ui) {
    egui::CollapsingHeader::new(t("Platzhalter")).show(ui, |ui| {
        egui::Grid::new("placeholders").num_columns(2).show(ui, |ui| {
            for (key, what) in PLACEHOLDERS {
                ui.label(RichText::new(key).monospace());
                ui.label(muted(t(what)));
                ui.end_row();
            }
        });
    });
}
