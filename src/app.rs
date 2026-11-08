//! The workplace window: sign-in, navigation and the overview.
//! The other sections live in the submodules.

mod bank;
mod books;
mod documents;
mod extras;
mod mailing;
mod projects;
mod receipts;
mod recurring;
mod reports;
mod sections;
mod settings;
mod settings_more;
mod statements;
mod stock;
mod storage;

use crate::backend::{Backend, ClientConfig, Remote};
use crate::export;
use crate::model::*;
use crate::server::{self, Role, UserInfo};
use crate::icons::{Icon, icon, icon_button, paint_icon};
use crate::ui_kit::*;
use crate::i18n::{Lang, t, tr};
use crate::tf;
use chrono::{Datelike, NaiveDate};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, RichText, ScrollArea, Stroke,
    TextEdit, Ui, vec2,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Start,
    Kontakte,
    Artikel,
    Belege,
    Projekte,
    Buchhaltung,
    Bank,
    Belegeingang,
    Auswertungen,
    Protokoll,
    Einstellungen,
}

impl Section {
    const ALL: [Section; 11] = [
        Section::Start,
        Section::Kontakte,
        Section::Artikel,
        Section::Belege,
        Section::Projekte,
        Section::Buchhaltung,
        Section::Bank,
        Section::Belegeingang,
        Section::Auswertungen,
        Section::Protokoll,
        Section::Einstellungen,
    ];
    fn label(self) -> &'static str {
        t(match self {
            Section::Start => "Übersicht",
            Section::Kontakte => "Kontakte",
            Section::Artikel => "Artikel & Leistungen",
            Section::Belege => "Angebote & Rechnungen",
            Section::Projekte => "Projekte & Zeiten",
            Section::Buchhaltung => "Einnahmen & Ausgaben",
            Section::Bank => "Bank",
            Section::Belegeingang => "Belegeingang",
            Section::Auswertungen => "Auswertungen",
            Section::Protokoll => "Protokoll",
            Section::Einstellungen => "Einstellungen",
        })
    }
    fn icon(self) -> Icon {
        match self {
            Section::Start => Icon::Home,
            Section::Kontakte => Icon::Users,
            Section::Artikel => Icon::Package,
            Section::Belege => Icon::Document,
            Section::Projekte => Icon::Clock,
            Section::Buchhaltung => Icon::Wallet,
            Section::Bank => Icon::Bank,
            Section::Belegeingang => Icon::Inbox,
            Section::Auswertungen => Icon::Chart,
            Section::Protokoll => Icon::History,
            Section::Einstellungen => Icon::Settings,
        }
    }
    pub fn from_name(s: &str) -> Option<Section> {
        let s = s.to_lowercase();
        Section::ALL
            .into_iter()
            .find(|x| format!("{x:?}").to_lowercase() == s)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Period {
    Year,
    Quarter(u32),
    Month(u32),
}

impl Period {
    fn label(self) -> String {
        match self {
            Period::Year => t("Gesamtes Jahr").into(),
            Period::Quarter(q) => tf!("{}. Quartal", q),
            Period::Month(m) => t([
                "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August",
                "September", "Oktober", "November", "Dezember",
            ][m as usize - 1])
                .into(),
        }
    }
    fn range(self, year: i32) -> (NaiveDate, NaiveDate) {
        let (m0, m1) = match self {
            Period::Year => (1, 12),
            Period::Quarter(q) => (q * 3 - 2, q * 3),
            Period::Month(m) => (m, m),
        };
        let from = NaiveDate::from_ymd_opt(year, m0, 1).unwrap();
        let to = if m1 == 12 {
            NaiveDate::from_ymd_opt(year, 12, 31).unwrap()
        } else {
            NaiveDate::from_ymd_opt(year, m1 + 1, 1)
                .unwrap()
                .pred_opt()
                .unwrap()
        };
        (from, to)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportTab {
    Euer,
    AnlageEuer,
    Ustva,
    Zm,
    Bilanz,
    Liquiditaet,
    OffenePosten,
    Kunden,
    Exporte,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Firma,
    Buchhaltung,
    Benutzer,
    Sicherheit,
    Mandanten,
    Ablage,
    Daten,
    Sprache,
    Email,
}

#[derive(PartialEq, Eq)]
enum Screen {
    Connect,
    Main,
}

pub struct Shot {
    pub path: PathBuf,
    pub requested: bool,
    pub done: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<String>>>,
}

pub struct App {
    data: Data,
    backend: Backend,
    screen: Screen,
    section: Section,
    cfg: ClientConfig,
    today: NaiveDate,

    password: String,
    connect_error: Option<String>,
    server_port: String,
    server_status: Option<Result<String, String>>,
    server_rx: Option<Receiver<Result<String, String>>>,

    toast: Option<(String, bool, f64)>,
    confirm: Option<(String, Op)>,

    contact_search: String,
    contact_kind: Option<ContactKind>,
    contact_edit: Option<Contact>,

    article_search: String,
    article_edit: Option<Article>,

    doc_kind: Option<DocKind>,
    doc_status: Option<DocStatus>,
    doc_search: String,
    doc_edit: Option<Document>,
    doc_paid_date: NaiveDate,
    /// The e-mail being written, if the send window is open.
    mail_draft: Option<mailing::MailDraft>,
    mail_state: mailing::MailSettingsState,
    receipts: receipts::ReceiptsState,
    /// Early-payment discount to deduct when booking the payment.
    doc_paid_discount: f64,
    /// Amount and way of the payment being booked (part payments).
    doc_pay_amount: Cents,
    doc_pay_method: String,
    /// Share of an advance invoice, in percent.
    advance_pct: f64,
    /// Showing the recurring invoices instead of the documents.
    doc_view_recurring: bool,
    recurring_edit: Option<Recurring>,

    booking_search: String,
    booking_kind: Option<BookingKind>,
    booking_year: i32,
    booking_edit: Option<Booking>,

    report_tab: ReportTab,
    report_year: i32,
    report_period: Period,

    log_search: String,

    settings_tab: SettingsTab,
    company_edit: Option<Company>,
    users: Vec<UserInfo>,
    users_error: Option<String>,
    new_user: (String, Role, String),
    pw_old: String,
    pw_new: String,
    new_user_sso: bool,

    /// A running Microsoft 365 sign-in in the browser.
    sso_rx: Option<Receiver<Result<crate::sso_client::SsoResult, String>>>,
    sso_cancel: Arc<AtomicBool>,

    /// Code from the authenticator app at sign-in, once the server asks for it.
    totp_code: String,
    need_code: bool,
    /// Companies offered by the server at the address typed in.
    server_info: Option<crate::backend::ServerInfo>,
    info_for: String,
    info_rx: Option<Receiver<(String, Result<crate::backend::ServerInfo, String>)>>,
    /// Signing in again at start with the remembered workplace (in the background).
    resume_rx: Option<Receiver<Result<(Remote, Data), String>>>,
    /// A newer version on GitHub: (version, address of the download page).
    update: Option<(String, String)>,
    update_rx: Option<Receiver<Option<(String, String)>>>,

    bank: bank::BankState,
    books: books::BooksState,
    stock: stock::StockState,
    security: settings::SecurityState,
    storage: storage::StorageState,
    projects: projects::ProjectsState,
    extras: extras::ExtrasState,

    shot: Option<Shot>,
    frames: u64,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shot: Option<Shot>,
        section: Option<Section>,
        lang: Option<Lang>,
    ) -> Self {
        setup_style(&cc.egui_ctx);
        let today = today();
        let cfg = ClientConfig::load();
        // Pictures are taken in German unless another language is asked for.
        let ui_lang = if shot.is_some() {
            lang.unwrap_or_default()
        } else {
            lang.or(cfg.language).unwrap_or_else(Lang::from_system)
        };
        crate::i18n::set_lang(ui_lang);
        let mut app = App {
            data: Data::default(),
            backend: Backend::Demo,
            screen: Screen::Connect,
            section: Section::Start,
            server_port: server::DEFAULT_PORT.to_string(),
            cfg,
            today,
            password: String::new(),
            connect_error: None,
            server_status: None,
            server_rx: None,
            toast: None,
            confirm: None,
            contact_search: String::new(),
            contact_kind: None,
            contact_edit: None,
            article_search: String::new(),
            article_edit: None,
            doc_kind: None,
            doc_status: None,
            doc_search: String::new(),
            doc_edit: None,
            doc_paid_date: today,
            mail_draft: None,
            mail_state: Default::default(),
            receipts: receipts::ReceiptsState { filter: Some(ReceiptStatus::Neu), ..Default::default() },
            doc_paid_discount: 0.0,
            doc_pay_amount: 0,
            doc_pay_method: "Bank".into(),
            advance_pct: 30.0,
            doc_view_recurring: false,
            recurring_edit: None,
            booking_search: String::new(),
            booking_kind: None,
            booking_year: today.year(),
            booking_edit: None,
            report_tab: ReportTab::Euer,
            report_year: today.year(),
            report_period: Period::Year,
            log_search: String::new(),
            settings_tab: SettingsTab::Firma,
            company_edit: None,
            users: Vec::new(),
            users_error: None,
            new_user: (String::new(), Role::Buchhaltung, String::new()),
            pw_old: String::new(),
            pw_new: String::new(),
            new_user_sso: false,
            sso_rx: None,
            sso_cancel: Arc::new(AtomicBool::new(false)),
            totp_code: String::new(),
            need_code: false,
            server_info: None,
            info_for: String::new(),
            info_rx: None,
            update: None,
            update_rx: None,
            resume_rx: None,
            bank: Default::default(),
            books: Default::default(),
            stock: Default::default(),
            security: Default::default(),
            storage: Default::default(),
            projects: Default::default(),
            extras: Default::default(),
            shot,
            frames: 0,
        };
        if app.shot.is_none() && app.cfg.check_updates {
            app.update_rx = Some(crate::export::check_for_update());
        }
        // "Beim nächsten Start merken": open what was open last time.
        if app.shot.is_none() && app.cfg.remember {
            match app.cfg.resume {
                crate::backend::Resume::Local => app.open_local(),
                crate::backend::Resume::Server if !app.cfg.remember_token.is_empty() => {
                    let (url, user, token, tenant) =
                        (app.cfg.server_url.clone(), app.cfg.user.clone(), app.cfg.remember_token.clone(), app.cfg.tenant.clone());
                    let ctx = cc.egui_ctx.clone();
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        let r = Remote::connect_token(&url, &user, &token, &tenant, ctx.clone());
                        let _ = tx.send(r);
                        ctx.request_repaint();
                    });
                    app.resume_rx = Some(rx);
                }
                _ => {}
            }
        }
        if app.shot.is_some() {
            // Pictures always show the example company, unless the sign-in screen is asked for.
            if std::env::var("OPENKONTOR_SHOT_LOGIN").is_err() {
                app.open_demo();
            }
            if let Some(s) = section {
                app.section = s;
            }
            // OPENKONTOR_SHOT_TAB chooses a tab of the section for the picture.
            if let Ok(tab) = std::env::var("OPENKONTOR_SHOT_TAB") {
                match tab.as_str() {
                    "euer" => app.report_tab = ReportTab::Euer,
                    "anlage" => app.report_tab = ReportTab::AnlageEuer,
                    "ustva" => app.report_tab = ReportTab::Ustva,
                    "zm" => app.report_tab = ReportTab::Zm,
                    "bilanz" => app.report_tab = ReportTab::Bilanz,
                    "liquiditaet" => app.report_tab = ReportTab::Liquiditaet,
                    "op" => app.report_tab = ReportTab::OffenePosten,
                    "kunden" => app.report_tab = ReportTab::Kunden,
                    "exporte" => app.report_tab = ReportTab::Exporte,
                    "firma" => app.settings_tab = SettingsTab::Firma,
                    "buchhaltung" => app.settings_tab = SettingsTab::Buchhaltung,
                    "benutzer" => app.settings_tab = SettingsTab::Benutzer,
                    "sicherheit" => app.settings_tab = SettingsTab::Sicherheit,
                    "mandanten" => app.settings_tab = SettingsTab::Mandanten,
                    "ablage" => app.settings_tab = SettingsTab::Ablage,
                    "daten" => app.settings_tab = SettingsTab::Daten,
                    "sprache" => app.settings_tab = SettingsTab::Sprache,
                    "email" => app.settings_tab = SettingsTab::Email,
                    "kasse" => app.books.tab = books::BooksTab::Kasse,
                    "anlagen" => app.books.tab = books::BooksTab::Anlagen,
                    "umbuchungen" => app.books.tab = books::BooksTab::Umbuchungen,
                    "lager" => app.stock.tab = stock::StockTab::Lager,
                    "wiederkehrend" => app.doc_view_recurring = true,
                    _ => {}
                }
            }
            // OPENKONTOR_SHOT_TILES shows the receipts as thumbnails.
            app.cfg.receipt_tiles = std::env::var("OPENKONTOR_SHOT_TILES").is_ok();
            // OPENKONTOR_SHOT_DOC=beleg opens the newest receipt of the inbox.
            if app.section == Section::Belegeingang
                && std::env::var("OPENKONTOR_SHOT_DOC").is_ok_and(|v| v == "beleg")
                && let Some(id) = app.data.receipts.last().map(|r| r.id)
            {
                app.open_receipt(id);
            }
            if app.section == Section::Belege
                && !app.doc_view_recurring
                && std::env::var("OPENKONTOR_SHOT_DOC").map_or(true, |v| v != "liste")
            {
                let id = app
                    .data
                    .documents
                    .iter()
                    .rev()
                    .find(|d| {
                        // OPENKONTOR_SHOT_DOC=rechnung shows the newest invoice instead of the offer.
                        let want = if std::env::var("OPENKONTOR_SHOT_DOC").is_ok_and(|v| v == "rechnung") {
                            DocKind::Rechnung
                        } else {
                            DocKind::Angebot
                        };
                        d.kind == want
                    })
                    .map(|d| d.id);
                app.open_doc(id);
            }
        }
        app
    }

    // ---- Changing data ----------------------------------------------------

    fn info(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), false, 0.0));
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), true, 0.0));
    }

    /// Runs a change locally or on the server and reports the result.
    fn run(&mut self, op: Op) -> Result<Option<u64>, ()> {
        if !self.backend.can_write() {
            self.error(t("Sie haben nur Leserechte."));
            return Err(());
        }
        match self.backend.run(&mut self.data, op) {
            Ok(id) => Ok(id),
            Err(e) => {
                if e.contains("neu anmelden") {
                    self.screen = Screen::Connect;
                    self.connect_error = Some(e.clone());
                }
                self.error(tr(&e));
                Err(())
            }
        }
    }

    /// Notes what to open at the next start.
    fn remember_session(&mut self, resume: crate::backend::Resume, token: &str) {
        if self.cfg.remember {
            self.cfg.resume = resume;
            if !token.is_empty() {
                self.cfg.remember_token = token.to_string();
            }
        } else {
            self.cfg.resume = crate::backend::Resume::Nothing;
            self.cfg.remember_token.clear();
        }
        self.cfg.save();
    }

    /// Signed in again with the remembered workplace (or not).
    fn finish_resume(&mut self, result: Result<(Remote, Data), String>) {
        match result {
            Ok((remote, data)) => {
                self.data = data;
                self.info(tf!("Angemeldet bei {} als {}", remote.tenant_name, remote.user));
                self.backend = Backend::Remote(Box::new(remote));
                self.screen = Screen::Main;
                self.section = self.first_section();
                self.connect_error = None;
            }
            Err(e) => {
                // An expired sign-in is forgotten; an unreachable server is tried again next time.
                if e.contains("abgelaufen") {
                    self.cfg.remember_token.clear();
                    self.cfg.resume = crate::backend::Resume::Nothing;
                    self.cfg.save();
                }
                self.connect_error = Some(tr(&e));
            }
        }
    }

    fn open_demo(&mut self) {
        // Pictures taken with --screenshot leave the remembered sign-in alone.
        if self.shot.is_none() {
            self.cfg.resume = crate::backend::Resume::Nothing;
            self.cfg.save();
        }
        self.data = sample_data();
        self.backend = Backend::Demo;
        // Two example supplier invoices for the inbox: an e-invoice and a PDF.
        for (name, bytes) in crate::inbox::demo_files() {
            let _ = self.backend.add_receipt(&mut self.data, &name, &bytes);
        }
        self.screen = Screen::Main;
        self.section = Section::Start;
    }

    fn open_local(&mut self) {
        self.data = load().unwrap_or_default();
        self.backend = Backend::Local;
        self.screen = Screen::Main;
        self.section = Section::Start;
        self.cfg.use_server = false;
        self.remember_session(crate::backend::Resume::Local, "");
    }

    fn connect(&mut self, ctx: &egui::Context) {
        let tenant = self.cfg.tenant.clone();
        match Remote::connect(&self.cfg.server_url, &self.cfg.user, &self.password, &self.totp_code, &tenant, self.cfg.remember, ctx.clone()) {
            Ok((remote, data)) => {
                let token = remote.remember_token.clone();
                self.data = data;
                self.info(tf!("Angemeldet bei {} als {}", remote.tenant_name, remote.user));
                self.backend = Backend::Remote(Box::new(remote));
                self.screen = Screen::Main;
                self.section = self.first_section();
                self.password.clear();
                self.totp_code.clear();
                self.need_code = false;
                self.connect_error = None;
                self.cfg.use_server = true;
                self.remember_session(crate::backend::Resume::Server, &token);
            }
            Err(e) => {
                if let Some(msg) = e.strip_prefix(crate::backend::NEED_CODE) {
                    self.need_code = true;
                    self.connect_error = Some(tr(msg));
                } else {
                    self.connect_error = Some(tr(&e));
                }
            }
        }
    }

    /// Where a user starts: people who only hand in receipts go straight to the inbox.
    fn first_section(&self) -> Section {
        if self.backend.allows(OpArea::Sales) || !self.backend.can_write() { Section::Start } else { Section::Belegeingang }
    }

    /// Sections the signed-in user may open.
    fn section_visible(&self, s: Section) -> bool {
        match s {
            Section::Buchhaltung | Section::Bank => self.backend.allows(OpArea::Accounting) || !self.backend.can_write(),
            Section::Belegeingang | Section::Einstellungen => true,
            _ => self.backend.allows(OpArea::Sales) || !self.backend.can_write(),
        }
    }

    /// Asks the server at the typed address for its name and companies (in the background).
    fn probe_server(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.info_rx
            && let Ok((url, r)) = rx.try_recv()
        {
            self.info_rx = None;
            self.info_for = url;
            self.server_info = r.ok();
            if let Some(info) = &self.server_info
                && !info.tenants.iter().any(|t| t.id == self.cfg.tenant)
            {
                self.cfg.tenant.clear();
            }
        }
        if self.info_rx.is_none() && self.info_for != self.cfg.server_url && self.shot.is_none() {
            let url = self.cfg.server_url.clone();
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let r = crate::backend::server_info(&url);
                let _ = tx.send((url, r));
                ctx.request_repaint();
            });
            self.info_rx = Some(rx);
        }
    }

    /// Starts the Microsoft 365 sign-in in the browser (in the background).
    fn start_sso(&mut self, ctx: &egui::Context) {
        let base = crate::backend::normalize_url(&self.cfg.server_url);
        let cancel = Arc::new(AtomicBool::new(false));
        self.sso_cancel = cancel.clone();
        self.connect_error = None;
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::sso_client::fetch_info(&base).and_then(|info| match info {
                Some(info) => crate::sso_client::browser_login(&info, cancel),
                None => Err(t("Auf diesem Server ist die Anmeldung mit Microsoft 365 nicht eingerichtet.").into()),
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.sso_rx = Some(rx);
    }

    fn finish_sso(&mut self, ctx: &egui::Context, result: Result<crate::sso_client::SsoResult, String>) {
        let tenant = self.cfg.tenant.clone();
        let remember = self.cfg.remember;
        let connected = result.and_then(|proof| Remote::connect_sso(&self.cfg.server_url, &proof, &tenant, remember, ctx.clone()));
        match connected {
            Ok((remote, data)) => {
                let token = remote.remember_token.clone();
                self.data = data;
                self.info(tf!("Angemeldet bei {} als {}", remote.tenant_name, remote.user));
                self.backend = Backend::Remote(Box::new(remote));
                self.screen = Screen::Main;
                self.section = self.first_section();
                self.connect_error = None;
                self.cfg.use_server = true;
                self.remember_session(crate::backend::Resume::Server, &token);
            }
            Err(e) => self.connect_error = Some(tr(&e)),
        }
    }

    fn start_server(&mut self) {
        let port: u16 = self
            .server_port
            .trim()
            .parse()
            .unwrap_or(server::DEFAULT_PORT);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let cfg = server::ServerConfig {
                port,
                ..Default::default()
            };
            server::run(cfg, move |r| {
                let _ = tx.send(r);
            });
        });
        self.server_rx = Some(rx);
        self.cfg.server_url = format!("http://localhost:{port}");
    }

    fn sign_out(&mut self) {
        // "Abmelden" means: do not sign in by yourself next time.
        if let Backend::Remote(r) = &self.backend
            && !self.cfg.remember_token.is_empty()
        {
            let (url, user, token, tenant) = (r.base.clone(), r.user.clone(), self.cfg.remember_token.clone(), r.tenant.clone());
            std::thread::spawn(move || Remote::forget(&url, &user, &token, &tenant));
        }
        self.cfg.resume = crate::backend::Resume::Nothing;
        self.cfg.remember_token.clear();
        self.cfg.save();
        self.backend = Backend::Demo;
        self.data = Data::default();
        self.screen = Screen::Connect;
        self.doc_edit = None;
        self.contact_edit = None;
        self.article_edit = None;
        self.booking_edit = None;
        self.company_edit = None;
        self.users.clear();
    }

    fn years(&self) -> Vec<i32> {
        let mut y: Vec<i32> = self
            .data
            .bookings
            .iter()
            .map(|b| b.date.year())
            .chain(self.data.documents.iter().map(|d| d.date.year()))
            .chain(std::iter::once(self.today.year()))
            .collect();
        y.sort_unstable();
        y.dedup();
        y.reverse();
        y
    }

    fn new_doc(&self, kind: DocKind) -> Document {
        let due = match kind {
            DocKind::Angebot => 30,
            _ => 0,
        };
        let (title, intro, outro) = doc_texts(kind, crate::i18n::lang(), due);
        let mut doc = Document {
            kind,
            title,
            intro,
            outro,
            due_days: due,
            date: self.today,
            ..Document::default()
        };
        if kind == DocKind::Rechnung {
            doc.apply_term(&self.data.term_for(None));
        }
        doc
    }

    fn open_doc(&mut self, id: Option<u64>) {
        self.doc_edit = id.and_then(|id| self.data.documents.iter().find(|d| d.id == id).cloned());
        self.doc_paid_date = self.today;
        self.doc_paid_discount = self.doc_edit.as_ref().map(|d| d.discount_on(self.today)).unwrap_or(0.0);
        self.doc_pay_amount = self.doc_edit.as_ref().map(|d| d.open_amount(self.data.company.small_business).abs()).unwrap_or(0);
        self.doc_view_recurring = false;
        self.section = Section::Belege;
    }

    /// Writes the document as PDF to a temporary file and opens it in the PDF viewer.
    /// A menu to switch the interface language; the choice is remembered.
    fn language_picker(&mut self, ui: &mut Ui) {
        let mut current = crate::i18n::lang();
        egui::ComboBox::from_id_salt("ui_lang")
            .selected_text(current.native_name())
            .width(110.0)
            .show_ui(ui, |ui| {
                for l in Lang::ALL {
                    ui.selectable_value(&mut current, l, l.native_name());
                }
            });
        if current != crate::i18n::lang() {
            crate::i18n::set_lang(current);
            self.cfg.language = Some(current);
            self.cfg.save();
        }
    }

    fn print_doc(&mut self, doc: &Document) {
        let bytes = crate::pdf::document_pdf(&self.data, doc);
        let path = std::env::temp_dir().join(format!("OpenKontor {}", crate::pdf::file_name(doc)));
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                export::open_file(&path);
                self.info(t("PDF geöffnet – von dort drucken oder versenden."));
            }
            Err(e) => self.error(tf!("Konnte das PDF nicht schreiben: {}", e)),
        }
    }

    fn save_doc_as(&mut self, doc: &Document) {
        // Finished invoices for ZUGFeRD customers are saved as ZUGFeRD PDF (looks the same).
        let zugferd = doc.status != DocStatus::Entwurf
            && matches!(doc.kind, DocKind::Rechnung | DocKind::Gutschrift)
            && self.data.einvoice_format(doc.contact_id) == crate::xrechnung::EInvoiceFormat::Zugferd;
        let bytes = if zugferd {
            match crate::pdf::zugferd_pdf(&self.data, doc) {
                Ok(b) => b,
                Err(problems) => {
                    self.error(tf!("Ohne E-Rechnungsdaten gespeichert, weil noch etwas fehlt: {}", problems.iter().map(|p| tr(p)).collect::<Vec<_>>().join(" ")));
                    crate::pdf::document_pdf(&self.data, doc)
                }
            }
        } else {
            crate::pdf::document_pdf(&self.data, doc)
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(crate::pdf::file_name(doc))
            .add_filter(t("PDF-Dokument"), &["pdf"])
            .save_file()
        {
            match std::fs::write(&path, bytes) {
                Ok(()) => self.info(tf!("PDF gespeichert: {}", path.display())),
                Err(e) => self.error(e.to_string()),
            }
        }
    }

    /// Saves the invoice as XRechnung (XML), e.g. to upload it to a portal of a public authority.
    fn save_xrechnung(&mut self, doc: &Document) {
        match crate::xrechnung::build(&self.data, doc, crate::xrechnung::Profile::XRechnung) {
            Err(problems) => self.error(problems.iter().map(|p| tr(p)).collect::<Vec<_>>().join("\n")),
            Ok(xml) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(crate::xrechnung::file_name(doc))
                    .add_filter("XRechnung", &["xml"])
                    .save_file()
                {
                    match std::fs::write(&path, xml) {
                        Ok(()) => self.info(tf!("XRechnung gespeichert: {}", path.display())),
                        Err(e) => self.error(e.to_string()),
                    }
                }
            }
        }
    }
}

/// Default title, opening and closing text of a new document in a language.
fn doc_texts(kind: DocKind, lang: Lang, _days: u32) -> (String, String, String) {
    let l = |de| crate::i18n::t_in(lang, de);
    match kind {
        DocKind::Angebot => (
            l("Angebot").into(),
            l("Gerne unterbreiten wir Ihnen folgendes Angebot:").into(),
            l("Wir freuen uns auf Ihren Auftrag.").into(),
        ),
        DocKind::Rechnung => (
            l("Rechnung").into(),
            l("Vielen Dank für Ihren Auftrag. Wir berechnen Ihnen folgende Leistungen:").into(),
            l("Vielen Dank für die gute Zusammenarbeit.").into(),
        ),
        DocKind::Gutschrift => (l("Gutschrift").into(), l("Wir schreiben Ihnen gut:").into(), String::new()),
        DocKind::Auftragsbestaetigung => (
            l("Auftragsbestätigung").into(),
            l("Vielen Dank für Ihren Auftrag. Wir bestätigen Ihnen folgende Leistungen:").into(),
            l("Wir freuen uns auf die weitere Zusammenarbeit.").into(),
        ),
        DocKind::Lieferschein => (
            l("Lieferschein").into(),
            l("Wir liefern Ihnen:").into(),
            l("Ware vollständig und in einwandfreiem Zustand erhalten:").into(),
        ),
        DocKind::Bestellung => (
            l("Bestellung").into(),
            l("Hiermit bestellen wir:").into(),
            l("Bitte bestätigen Sie uns den Liefertermin.").into(),
        ),
    }
}

/// When the contact of a draft changes, untouched default texts switch to the contact's language.
fn follow_contact_language(doc: &mut Document, lang: Lang) {
    let unchanged = Lang::ALL.iter().any(|l| {
        let (title, intro, outro) = doc_texts(doc.kind, *l, doc.due_days);
        doc.title == title && doc.intro == intro && doc.outro == outro
    });
    if unchanged {
        let (title, intro, outro) = doc_texts(doc.kind, lang, doc.due_days);
        doc.title = title;
        doc.intro = intro;
        doc.outro = outro;
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if let Backend::Remote(r) = &mut self.backend {
            if let Some(d) = r.poll() {
                self.data = d;
                // A finished document on screen follows the server (e.g. paid elsewhere).
                if let Some(doc) = &self.doc_edit
                    && doc.status != DocStatus::Entwurf
                {
                    let id = doc.id;
                    self.doc_edit = self.data.documents.iter().find(|d| d.id == id).cloned();
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        if let Some(rx) = &self.server_rx
            && let Ok(r) = rx.try_recv()
        {
            self.server_status = Some(r);
            self.server_rx = None;
        }
        if let Some(rx) = &self.sso_rx
            && let Ok(r) = rx.try_recv()
        {
            self.sso_rx = None;
            self.finish_sso(ctx, r);
        }
        if let Some(rx) = &self.update_rx
            && let Ok(r) = rx.try_recv()
        {
            self.update_rx = None;
            self.update = r;
        }
        if let Some(rx) = &self.resume_rx
            && let Ok(r) = rx.try_recv()
        {
            self.resume_rx = None;
            self.finish_resume(r);
        }
        if self.screen == Screen::Connect {
            self.probe_server(ctx);
        }
        if let Some(shot) = &mut self.shot {
            ctx.request_repaint();
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            if shot.done.load(Ordering::SeqCst) {
                if let Some(e) = shot.error.lock().unwrap().take() {
                    eprintln!("Bild konnte nicht gespeichert werden: {e}");
                    std::process::exit(1);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frames >= 120 && !shot.requested {
                shot.requested = true;
                let (path, done, err) = (shot.path.clone(), shot.done.clone(), shot.error.clone());
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(e) = image::save_buffer(
                        &path,
                        &bytes,
                        w as u32,
                        h as u32,
                        image::ExtendedColorType::Rgba8,
                    ) {
                        *err.lock().unwrap() = Some(e.to_string());
                    }
                    done.store(true, Ordering::SeqCst);
                });
            } else if self.frames > 3000 {
                eprintln!("Bildschirmfoto kam nicht zustande.");
                std::process::exit(1);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        if let Some(t) = &mut self.toast
            && t.2 == 0.0
        {
            t.2 = now;
        }
        match self.screen {
            Screen::Connect => self.connect_screen(ui),
            Screen::Main => self.main_screen(ui),
        }
        self.take_dropped_files(ui);
        self.drop_hint(ui);
        self.dialogs(ui);
        self.show_toast(ui, now);
    }
}

// ============================================================================
// Sign-in

impl App {
    fn connect_screen(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(Frame::new().fill(NAV_BG))
            .show(ui, |ui| {
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.add_space(16.0);
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.add_space(16.0);
                        self.language_picker(ui);
                        icon(ui, Icon::Globe, 18.0, NAV_TEXT);
                    });
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(560.0);
                        ui.horizontal(|ui| {
                            logo(ui, 44.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(t("OpenKontor")).size(26.0).strong().color(Color32::WHITE));
                                ui.label(
                                    RichText::new(t("Rechnungen & Buchhaltung – frei und selbst gehostet"))
                                        .color(NAV_TEXT),
                                );
                            });
                        });
                        ui.add_space(20.0);
                        card(ui, |ui| {
                            ui.set_width(520.0);
                            ui.horizontal(|ui| {
                                icon(ui, Icon::SignIn, 18.0, ACCENT);
                                card_title(ui, t("Mit dem Firmenserver verbinden"));
                            });
                            ui.label(muted(t("Alle Arbeitsplätze teilen sich einen Datenbestand auf dem Server.")));
                            ui.add_space(6.0);
                            egui::Grid::new("login")
                                .num_columns(2)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    text_row(ui, t("Serveradresse"), &mut self.cfg.server_url, 320.0);
                                    if let Some(info) = &self.server_info
                                        && info.tenants.len() > 1
                                    {
                                        ui.label(muted(t("Firma")));
                                        let current = info
                                            .tenants
                                            .iter()
                                            .find(|x| x.id == self.cfg.tenant)
                                            .map(|x| x.name.clone())
                                            .unwrap_or_else(|| info.tenants[0].name.clone());
                                        egui::ComboBox::from_id_salt("login_tenant").width(320.0).selected_text(current).show_ui(ui, |ui| {
                                            for x in &info.tenants {
                                                ui.selectable_value(&mut self.cfg.tenant, x.id.clone(), &x.name);
                                            }
                                        });
                                        ui.end_row();
                                    }
                                    text_row(ui, t("Benutzer"), &mut self.cfg.user, 320.0);
                                    ui.label(muted(t("Passwort")));
                                    let r = ui.add(
                                        TextEdit::singleline(&mut self.password)
                                            .password(true)
                                            .desired_width(320.0),
                                    );
                                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.need_code {
                                        self.connect(&ctx);
                                    }
                                    ui.end_row();
                                    if self.need_code {
                                        ui.label(muted(t("Bestätigungscode")));
                                        let r = ui.add(
                                            TextEdit::singleline(&mut self.totp_code)
                                                .hint_text(t("6 Ziffern aus der Authenticator-App"))
                                                .desired_width(320.0),
                                        );
                                        r.request_focus();
                                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                            self.connect(&ctx);
                                        }
                                        ui.end_row();
                                    }
                                });
                            if self.resume_rx.is_some() {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(muted(tf!("Verbinde wieder mit {} …", self.cfg.server_url)));
                                    if ui.button(t("Abbrechen")).clicked() {
                                        self.resume_rx = None;
                                    }
                                });
                            }
                            if let Some(e) = &self.connect_error {
                                ui.label(RichText::new(e).color(RED));
                            }
                            if ui
                                .checkbox(&mut self.cfg.remember, t("Beim nächsten Start merken"))
                                .on_hover_text(t("OpenKontor verbindet sich beim nächsten Start von selbst wieder mit diesem Server bzw. öffnet den Einzelplatz. Das Passwort wird nicht gespeichert; „Abmelden“ beendet das."))
                                .changed()
                            {
                                self.cfg.save();
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if primary(ui, t("Anmelden")).clicked() {
                                    self.connect(&ctx);
                                }
                                ui.add_space(8.0);
                                if self.sso_rx.is_some() {
                                    ui.spinner();
                                    ui.label(muted(t("Bitte im Browser bei Microsoft anmelden …")));
                                    if ui.button(t("Abbrechen")).clicked() {
                                        self.sso_cancel.store(true, Ordering::Relaxed);
                                    }
                                } else if icon_button(ui, Icon::SignIn, t("Mit Microsoft 365 anmelden"))
                                    .on_hover_text(t("Nur wenn auf dem Server eingerichtet. Öffnet die Anmeldung von Microsoft im Browser."))
                                    .clicked()
                                {
                                    self.start_sso(&ctx);
                                }
                            });
                        });
                        ui.add_space(14.0);
                        card(ui, |ui| {
                            ui.set_width(520.0);
                            ui.horizontal(|ui| {
                                icon(ui, Icon::Server, 18.0, ACCENT);
                                card_title(ui, t("Diesen Rechner als Server nutzen"));
                            });
                            ui.label(muted(t(
                                "Startet den Server in diesem Programm; er läuft, solange das Fenster offen ist. \
                                 Andere Arbeitsplätze verbinden sich über die Adresse dieses Rechners. \
                                 Für den Dauerbetrieb (z. B. auf einem Linux-Server mit PostgreSQL) \
                                 das Konsolenprogramm openkontor-server verwenden.",
                            )));
                            ui.horizontal(|ui| {
                                ui.label(muted(t("Port")));
                                ui.add(TextEdit::singleline(&mut self.server_port).desired_width(70.0));
                                let running = matches!(self.server_status, Some(Ok(_))) || self.server_rx.is_some();
                                if ui.add_enabled(!running, egui::Button::new(t("Server starten"))).clicked() {
                                    self.start_server();
                                }
                            });
                            match &self.server_status {
                                Some(Ok(m)) => {
                                    ui.label(RichText::new(format!("● {m}")).color(GREEN));
                                    ui.label(muted(
                                        t("Neuer Server: Benutzer „admin“, Passwort „admin“ – bitte gleich unter Einstellungen ändern."),
                                    ));
                                }
                                Some(Err(e)) => {
                                    ui.label(RichText::new(e).color(RED));
                                }
                                None if self.server_rx.is_some() => {
                                    ui.label(muted(t("Server startet …")));
                                }
                                None => {}
                            }
                        });
                        ui.add_space(14.0);
                        card(ui, |ui| {
                            ui.set_width(520.0);
                            card_title(ui, t("Ohne Server arbeiten"));
                            ui.horizontal(|ui| {
                                if ui.button(t("Einzelplatz (Daten auf diesem Rechner)")).clicked() {
                                    self.open_local();
                                }
                                if ui.button(t("Demo mit Beispielfirma")).clicked() {
                                    self.open_demo();
                                }
                            });
                            if ui.checkbox(&mut self.cfg.remember, t("Beim nächsten Start merken")).changed() {
                                self.cfg.save();
                            }
                        });
                    });
                });
            });
    }
}

// ============================================================================
// Main window frame

impl App {
    fn main_screen(&mut self, ui: &mut Ui) {
        egui::Panel::left("nav")
            .exact_size(224.0)
            .resizable(false)
            .frame(Frame::new().fill(NAV_BG).inner_margin(Margin::symmetric(12, 16)))
            .show(ui, |ui| self.nav(ui));
        egui::Panel::top("top")
            .exact_size(58.0)
            .frame(
                Frame::new()
                    .fill(CARD)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::symmetric(22, 10)),
            )
            .show(ui, |ui| self.top_bar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(20)))
            .show(ui, |ui| match self.section {
                Section::Start => self.dashboard(ui),
                Section::Kontakte => self.contacts(ui),
                Section::Artikel => self.articles(ui),
                Section::Belege => {
                    if self.doc_edit.is_some() {
                        self.document_page(ui)
                    } else {
                        self.documents(ui)
                    }
                }
                Section::Buchhaltung => self.books_page(ui),
                Section::Bank => self.bank_page(ui),
                Section::Projekte => self.projects_page(ui),
                Section::Belegeingang => self.receipts_page(ui),
                Section::Auswertungen => self.reports(ui),
                Section::Protokoll => self.log(ui),
                Section::Einstellungen => self.settings(ui),
            });
    }

    fn nav(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            logo(ui, 34.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(t("OpenKontor")).size(18.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(t("Rechnung & Buchhaltung")).size(11.5).color(NAV_TEXT));
            });
        });
        ui.add_space(22.0);
        for s in Section::ALL {
            if !self.section_visible(s) {
                continue;
            }
            if s == Section::Protokoll {
                ui.add_space(10.0);
                ui.label(RichText::new(t("SYSTEM")).size(11.0).color(Color32::from_rgb(0x7f, 0x95, 0xab)));
            }
            let selected = self.section == s;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 36.0), egui::Sense::click());
            let p = ui.painter();
            if selected {
                p.rect_filled(rect, CornerRadius::same(7), NAV_SEL);
                p.rect_filled(
                    egui::Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, 20.0)),
                    CornerRadius::same(2),
                    TEAL,
                );
            } else if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(7), Color32::from_rgb(0x1c, 0x3b, 0x5a));
            }
            let color = if selected { Color32::WHITE } else { NAV_TEXT };
            paint_icon(
                p,
                egui::Rect::from_center_size(rect.left_center() + vec2(19.0, 0.0), egui::Vec2::splat(18.0)),
                s.icon(),
                if selected { TEAL } else { color },
            );
            p.text(
                rect.left_center() + vec2(36.0, 0.0),
                egui::Align2::LEFT_CENTER,
                s.label(),
                egui::FontId::proportional(14.0),
                color,
            );
            if s == Section::Bank || s == Section::Belegeingang {
                let open = if s == Section::Bank {
                    self.data.bank.iter().filter(|t| t.status == BankStatus::Offen).count()
                } else {
                    self.data.receipts.iter().filter(|r| r.status == ReceiptStatus::Neu).count()
                };
                if open > 0 {
                    let center = rect.right_center() - vec2(20.0, 0.0);
                    p.circle_filled(center, 10.0, TEAL);
                    p.text(center, egui::Align2::CENTER_CENTER, open.to_string(), egui::FontId::proportional(11.5), Color32::WHITE);
                }
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.section = s;
                if s == Section::Belege {
                    self.doc_edit = None;
                }
            }
            ui.add_space(2.0);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new(tf!("Freie Software · Version {}", env!("CARGO_PKG_VERSION")))
                    .size(11.0)
                    .color(Color32::from_rgb(0x7f, 0x95, 0xab)),
            );
            if let Some((version, url)) = self.update.clone()
                && ui
                    .link(RichText::new(tf!("Neue Version {} herunterladen", version)).size(11.5).color(TEAL))
                    .clicked()
            {
                export::open_url(&url);
            }
            ui.label(RichText::new(&self.data.company.name).size(12.5).color(NAV_TEXT));
        });
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new(self.section.label()).size(20.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(t("Abmelden")).clicked() {
                    self.sign_out();
                    return;
                }
                let (text, color) = match &self.backend {
                    Backend::Remote(r) if r.online => (
                        format!("● {} · {} ({})", r.tenant_name, r.user, r.role.label()),
                        GREEN,
                    ),
                    Backend::Remote(r) => (tf!("● {} nicht erreichbar", r.server_name), RED),
                    Backend::Local => (t("● Einzelplatz").to_string(), ACCENT),
                    Backend::Demo => (
                        t("● Demo – Änderungen werden nicht gespeichert").to_string(),
                        ORANGE,
                    ),
                };
                ui.label(RichText::new(text).color(color).size(13.0));
                ui.add_space(12.0);
                if (self.section != Section::Belege || self.doc_edit.is_none())
                    && self.backend.allows(OpArea::Sales)
                    && primary(ui, t("+ Rechnung")).clicked()
                {
                    self.doc_edit = Some(self.new_doc(DocKind::Rechnung));
                    self.section = Section::Belege;
                }
            });
        });
    }

    fn show_toast(&mut self, ui: &mut Ui, now: f64) {
        let Some((msg, is_err, t)) = self.toast.clone() else {
            return;
        };
        if t > 0.0 && now - t > if is_err { 7.0 } else { 3.5 } {
            self.toast = None;
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        egui::Area::new(Id::unique("toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-24.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(if is_err {
                        Color32::from_rgb(0xfd, 0xec, 0xec)
                    } else {
                        Color32::from_rgb(0x1d, 0x34, 0x4c)
                    })
                    .stroke(Stroke::new(1.0, if is_err { RED } else { NAV_BG }))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(RichText::new(msg).color(if is_err { RED } else { Color32::WHITE }));
                    });
            });
    }

    fn dialogs(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some((question, _)) = &self.confirm {
            let question = question.clone();
            let mut answer = None;
            egui::Modal::new(Id::unique("confirm")).show(&ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(t("Bitte bestätigen")).strong().size(16.0));
                ui.add_space(4.0);
                ui.label(question);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if primary(ui, t("Ja, fortfahren")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(t("Abbrechen")).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if let Some(yes) = answer {
                let (_, op) = self.confirm.take().unwrap();
                if yes {
                    let closes_doc = matches!(op, Op::DeleteDocument(_));
                    let closes_receipt = matches!(op, Op::DeleteReceipt(_));
                    if self.run(op).is_ok() {
                        self.info(t("Erledigt."));
                        if closes_receipt {
                            self.receipts.edit = None;
                            self.receipts.preview = None;
                        }
                        if closes_doc {
                            self.doc_edit = None;
                        } else if let Some(d) = &self.doc_edit {
                            let id = d.id;
                            if let Some(fresh) = self.data.documents.iter().find(|x| x.id == id) {
                                self.doc_edit = Some(fresh.clone());
                            }
                        }
                        self.contact_edit = None;
                        self.article_edit = None;
                        self.booking_edit = None;
                        self.company_edit = None;
                    }
                }
            }
        }
        if self.contact_edit.is_some() {
            self.contact_dialog(&ctx);
        }
        if self.article_edit.is_some() {
            self.article_dialog(&ctx);
        }
        if self.booking_edit.is_some() {
            self.booking_dialog(&ctx);
        }
        self.books_dialogs(&ctx);
        self.stock_dialogs(&ctx);
        self.bank_dialogs(&ctx);
        self.project_dialogs(&ctx);
        self.extras_dialogs(&ctx);
        if self.mail_draft.is_some() {
            self.mail_dialog(&ctx);
        }
        if self.recurring_edit.is_some() {
            self.recurring_dialog(&ctx);
        }
    }
}

// ============================================================================
// Overview

impl App {
    fn dashboard(&mut self, ui: &mut Ui) {
        let year = self.today.year();
        let (from, to) = Period::Year.range(year);
        let rep = period_report(&self.data, from, to);
        let small = self.data.company.small_business;
        let open: Vec<Document> = self
            .data
            .documents
            .iter()
            .filter(|d| d.kind == DocKind::Rechnung && d.status == DocStatus::Offen)
            .cloned()
            .collect();
        let open_sum: Cents = open.iter().map(|d| d.to_eur(d.open_amount(small))).sum();
        let overdue = open.iter().filter(|d| d.is_overdue(self.today)).count();

        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let greeting = match &self.backend {
                Backend::Remote(r) => tf!("Guten Tag, {}!", r.user),
                _ => t("Guten Tag!").to_string(),
            };
            ui.label(RichText::new(greeting).size(17.0).strong());
            ui.label(muted(tf!(
                "{} · Stand {}",
                self.data.company.name,
                fmt_date(self.today)
            )));
            ui.add_space(10.0);

            let due = self.data.due_recurring(self.today);
            if due > 0 && self.backend.allows(OpArea::Sales) {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        icon(ui, Icon::Repeat, 18.0, ORANGE);
                        ui.label(RichText::new(tf!("{} wiederkehrende Rechnung(en) sind fällig.", due)).strong());
                        if primary(ui, t("Jetzt erstellen")).clicked() && self.run(Op::CreateDueRecurring(self.today)).is_ok() {
                            self.info(t("Die Rechnungen wurden erstellt. Sie finden sie unter Angebote & Rechnungen."));
                        }
                    });
                });
                ui.add_space(14.0);
            }
            if self.data.documents.is_empty() && self.data.bookings.is_empty() {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    card_title(ui, t("Willkommen bei OpenKontor"));
                    ui.label(t("Noch sind keine Daten vorhanden. Tragen Sie zuerst Ihre Firmendaten ein oder laden Sie die Beispielfirma, um alles auszuprobieren."));
                    ui.horizontal(|ui| {
                        if primary(ui, t("Firmendaten eintragen")).clicked() {
                            self.section = Section::Einstellungen;
                            self.settings_tab = SettingsTab::Firma;
                        }
                        if ui.button(t("Beispielfirma laden")).clicked() {
                            self.confirm = Some((
                                t("Den Datenbestand durch die Beispielfirma ersetzen?").into(),
                                Op::ReplaceAll(Box::new(sample_data())),
                            ));
                        }
                    });
                });
                ui.add_space(14.0);
            }

            let w = (ui.available_width() - 3.0 * 14.0) / 4.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                kpi(ui, w, &tf!("Umsatz {} (netto)", year), &fmt_money(rep.income_net), t("bezahlte Rechnungen & Einnahmen"), INCOME);
                kpi(ui, w, &tf!("Ausgaben {} (netto)", year), &fmt_money(rep.expense_net), t("gebuchte Ausgaben"), EXPENSE);
                kpi(
                    ui,
                    w,
                    &tf!("Gewinn {}", year),
                    &fmt_money(rep.profit()),
                    t("vor Steuern (EÜR)"),
                    if rep.profit() >= 0 { GREEN } else { RED },
                );
                kpi(
                    ui,
                    w,
                    t("Offene Rechnungen"),
                    &fmt_money(open_sum),
                    &tf!("{} offen · {} überfällig", open.len(), overdue),
                    if overdue > 0 { ORANGE } else { ACCENT },
                );
            });
            ui.add_space(14.0);

            let total = ui.available_width();
            let left_w = (total * 0.62).floor();
            let right_w = total - left_w - 14.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    card(ui, |ui| {
                        ui.set_width(left_w - 34.0);
                        ui.horizontal(|ui| {
                            card_title(ui, &tf!("Einnahmen und Ausgaben {}", year));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                legend(ui, &[(t("Ausgaben"), EXPENSE), (t("Einnahmen"), INCOME)]);
                            });
                        });
                        bar_chart(ui, &monthly(&self.data, year), 230.0);
                    });
                    ui.add_space(14.0);
                    card(ui, |ui| {
                        ui.set_width(left_w - 34.0);
                        card_title(ui, t("Schnellzugriff"));
                        ui.horizontal_wrapped(|ui| {
                            if icon_button(ui, Icon::Document, t("Neue Rechnung")).clicked() {
                                self.doc_edit = Some(self.new_doc(DocKind::Rechnung));
                                self.section = Section::Belege;
                            }
                            if icon_button(ui, Icon::Document, t("Neues Angebot")).clicked() {
                                self.doc_edit = Some(self.new_doc(DocKind::Angebot));
                                self.section = Section::Belege;
                            }
                            if icon_button(ui, Icon::Receipt, t("Ausgabe erfassen")).clicked() {
                                self.section = Section::Buchhaltung;
                                self.booking_edit = Some(Booking::default());
                            }
                            if icon_button(ui, Icon::UserPlus, t("Neuer Kunde")).clicked() {
                                self.section = Section::Kontakte;
                                self.contact_edit = Some(Contact::default());
                            }
                            if icon_button(ui, Icon::Percent, t("Umsatzsteuer")).clicked() {
                                self.section = Section::Auswertungen;
                                self.report_tab = ReportTab::Ustva;
                                self.report_year = year;
                                self.report_period = Period::Quarter(self.today.month0() / 3 + 1);
                            }
                        });
                    });
                });
                ui.vertical(|ui| {
                    ui.set_width(right_w);
                    card(ui, |ui| {
                        ui.set_width(right_w - 34.0);
                        card_title(ui, t("Fällige Rechnungen"));
                        let mut list = open.clone();
                        list.sort_by_key(|d| d.due_date());
                        if list.is_empty() {
                            empty_hint(ui, t("Alle Rechnungen sind bezahlt."));
                        }
                        let mut clicked = None;
                        for d in list.iter().take(6) {
                            let overdue = d.is_overdue(self.today);
                            let r = ui
                                .vertical(|ui| {
                                    let amount = fmt_money_cur(crate::i18n::lang(), d.open_amount(small), d.currency());
                                    ui.horizontal(|ui| {
                                        let w = ui.available_width() - ui.spacing().item_spacing.x;
                                        let name_w = (w - 105.0).max(60.0);
                                        ui.allocate_ui_with_layout(vec2(name_w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                                            ui.set_min_width(name_w);
                                            ui.add(egui::Label::new(RichText::new(self.data.contact_name(d.contact_id)).strong()).truncate());
                                        });
                                        right_label(ui, 100.0, RichText::new(amount).strong());
                                    });
                                    ui.vertical(|ui| {
                                        let days = (d.due_date() - self.today).num_days();
                                        let when = if days < 0 {
                                            tf!("{} · seit {} Tagen überfällig", d.number, -days)
                                        } else if days == 0 {
                                            tf!("{} · heute fällig", d.number)
                                        } else {
                                            tf!("{} · fällig in {} Tagen", d.number, days)
                                        };
                                        ui.label(RichText::new(when).size(12.5).color(if overdue { RED } else { MUTED }));
                                    });
                                })
                                .response
                                .interact(egui::Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                clicked = Some(d.id);
                            }
                            ui.separator();
                        }
                        if let Some(id) = clicked {
                            self.open_doc(Some(id));
                        }
                    });
                    ui.add_space(14.0);
                    let q = self.today.month0() / 3 + 1;
                    let (qf, qt) = Period::Quarter(q).range(year);
                    let qr = period_report(&self.data, qf, qt);
                    card(ui, |ui| {
                        ui.set_width(right_w - 34.0);
                        card_title(ui, &tf!("Umsatzsteuer {}. Quartal {}", q, year));
                        egui::Grid::new("vat_q").num_columns(2).spacing([20.0, 6.0]).show(ui, |ui| {
                            ui.label(muted(t("Umsatzsteuer")));
                            ui.label(fmt_money(qr.vat_collected));
                            ui.end_row();
                            ui.label(muted(t("Vorsteuer")));
                            ui.label(fmt_money(-qr.vat_paid));
                            ui.end_row();
                            ui.label(RichText::new(if qr.vat_due() >= 0 { t("Zahllast") } else { t("Erstattung") }).strong());
                            ui.label(RichText::new(fmt_money(qr.vat_due().abs())).strong());
                            ui.end_row();
                        });
                    });
                });
            });
        });
    }
}
