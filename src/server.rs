//! The self-hosted server: keeps the data of one or more companies (Mandanten), the users
//! and their sessions. Workplaces talk to it with JSON over HTTP – or HTTPS when a
//! certificate is given – and phones use the web app it delivers at its address.

#[path = "server_routes.rs"]
mod routes;

use crate::model::{Data, Op, OpArea};
use crate::sso::{SsoConfig, Verifier};
use crate::store::{Change, JsonStore, PgStore, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tiny_http::{Header, Method, Request, Response, Server};

pub const DEFAULT_PORT: u16 = 8470;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Buchhaltung,
    #[default]
    Lesen,
    /// Offers, invoices, contacts, articles and stock; no bookkeeping.
    Vertrieb,
    /// Only hands in receipts (inbox).
    Belegerfassung,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Admin, Role::Buchhaltung, Role::Vertrieb, Role::Belegerfassung, Role::Lesen];

    pub fn label(self) -> &'static str {
        crate::i18n::t(match self {
            Role::Admin => "Administrator",
            Role::Buchhaltung => "Buchhaltung",
            Role::Lesen => "Nur lesen",
            Role::Vertrieb => "Vertrieb",
            Role::Belegerfassung => "Belegerfassung",
        })
    }

    pub fn hint(self) -> &'static str {
        crate::i18n::t(match self {
            Role::Admin => "Alles, auch Benutzer, Mandanten und Server-Einstellungen",
            Role::Buchhaltung => "Alles außer Benutzer und Server-Einstellungen",
            Role::Lesen => "Sieht alles, ändert nichts",
            Role::Vertrieb => "Kontakte, Artikel, Angebote, Rechnungen und Lager – keine Buchhaltung",
            Role::Belegerfassung => "Nur Belege einreichen (z. B. per Handy)",
        })
    }

    pub fn can_write(self) -> bool {
        self != Role::Lesen
    }

    /// Whether the role may make changes of this kind.
    pub fn allows(self, area: OpArea) -> bool {
        match self {
            Role::Admin | Role::Buchhaltung => true,
            Role::Vertrieb => matches!(area, OpArea::Sales | OpArea::Receipts),
            Role::Belegerfassung => area == OpArea::Receipts,
            Role::Lesen => false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    pub name: String,
    pub role: Role,
    pub salt: String,
    pub hash: String,
    /// Secret for sign-in codes from an authenticator app (base32); empty = off.
    pub totp: String,
    /// Workplaces that stay signed in ("beim nächsten Start merken"). Only hashes are kept;
    /// a new password removes them all.
    pub remembered: Vec<Remembered>,
}

/// A workplace that may sign in again without the password, until the date.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Remembered {
    pub hash: String,
    pub until: chrono::NaiveDate,
}

/// How long a workplace stays signed in without being used.
const REMEMBER_DAYS: i64 = 30;

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub data: Data,
    pub users: Vec<User>,
    pub revision: u64,
    /// Outgoing mail server; stays on the server (contains the password).
    pub mail: crate::mail::MailSettings,
    /// Shared mailbox for incoming receipts; stays on the server (contains the password).
    pub inbox: crate::imap::InboxSettings,
}

// ---- Messages between workplace and server --------------------------------

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub user: String,
    pub password: String,
    /// Code from the authenticator app, for users with two-factor sign-in.
    #[serde(default)]
    pub code: String,
    /// Company (Mandant) to work in; empty = the first one.
    #[serde(default)]
    pub tenant: String,
    /// Stay signed in on this workplace.
    #[serde(default)]
    pub remember: bool,
}

/// Signing in again with the token of a remembered workplace.
#[derive(Serialize, Deserialize)]
pub struct TokenLoginRequest {
    pub user: String,
    pub token: String,
    #[serde(default)]
    pub tenant: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginReply {
    pub token: String,
    pub user: String,
    pub role: Role,
    pub server_name: String,
    /// Whether the server offers Microsoft 365 sign-in.
    #[serde(default)]
    pub sso_enabled: bool,
    #[serde(default)]
    pub totp_enabled: bool,
    #[serde(default)]
    pub tenant: String,
    #[serde(default)]
    pub tenant_name: String,
    /// Token to sign in again at the next start (only when asked to remember).
    #[serde(default)]
    pub remember_token: String,
}

/// Mail server settings as an administrator sees them: without the password.
#[derive(Serialize, Deserialize)]
pub struct MailSettingsReply {
    pub settings: crate::mail::MailSettings,
    pub password_set: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SsoLoginRequest {
    pub id_token: String,
    pub nonce: String,
    #[serde(default)]
    pub tenant: String,
    #[serde(default)]
    pub remember: bool,
}

#[derive(Serialize, Deserialize)]
pub struct StateReply {
    pub revision: u64,
    pub data: Option<Data>,
}

#[derive(Serialize, Deserialize)]
pub struct OpReply {
    pub revision: u64,
    pub id: Option<u64>,
    pub data: Data,
}

#[derive(Serialize, Deserialize)]
pub struct ErrorReply {
    pub error: String,
    /// The password was right; a code from the authenticator app is still needed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub need_code: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    pub role: Role,
    /// Signs in only with Microsoft 365 (has no password).
    #[serde(default)]
    pub sso_only: bool,
    /// Uses two-factor sign-in.
    #[serde(default)]
    pub totp: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SaveUserRequest {
    pub name: String,
    pub role: Role,
    /// Empty keeps the current password.
    pub password: String,
    /// Sign in only with Microsoft 365; removes the password.
    #[serde(default)]
    pub sso_only: bool,
}

#[derive(Serialize, Deserialize)]
pub struct PasswordRequest {
    pub old: String,
    pub new: String,
}

/// A company on the server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
}

/// Starting two-factor sign-in: the secret to put into the authenticator app.
#[derive(Serialize, Deserialize)]
pub struct TotpStart {
    pub secret: String,
    pub uri: String,
}

// ---- Password hashing -------------------------------------------------------

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("Zufallszahlen nicht verfügbar");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_password(salt: &str, password: &str) -> String {
    // Many rounds of salted SHA-256 to slow down guessing.
    let mut h: Vec<u8> = format!("{salt}:{password}").into_bytes();
    for _ in 0..50_000 {
        let mut d = Sha256::new();
        d.update(&h);
        d.update(salt.as_bytes());
        h = d.finalize().to_vec();
    }
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn make_user(name: &str, role: Role, password: &str) -> User {
    let salt = random_hex(16);
    User { name: name.to_string(), role, hash: hash_password(&salt, password), salt, ..User::default() }
}

// ---- Server -------------------------------------------------------------------

pub struct ServerConfig {
    pub port: u16,
    /// Address to listen on; 0.0.0.0 means every network card.
    pub bind: String,
    /// Folder for the JSON file (without database) and the daily backups.
    pub dir: PathBuf,
    pub name: String,
    /// PostgreSQL connection, e.g. `postgres://openkontor:geheim@localhost/openkontor`.
    pub database: Option<String>,
    /// Optional Microsoft 365 sign-in.
    pub sso: Option<SsoConfig>,
    /// Certificate and key (PEM files) for HTTPS.
    pub tls: Option<(String, String)>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            bind: "0.0.0.0".into(),
            dir: default_server_dir(),
            name: "OpenKontor Server".into(),
            database: None,
            sso: None,
            tls: None,
        }
    }
}

pub fn default_server_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("OpenKontor").join("Server")
}

type InboxResult = Result<(Vec<crate::imap::Fetched>, usize), String>;
type Reply = Response<std::io::Cursor<Vec<u8>>>;

/// One company: its data, users and settings, and where they are stored.
struct Tenant {
    id: String,
    stored: Stored,
    store: Box<dyn Store>,
    dir: PathBuf,
    last_backup_day: String,
    /// Day on which due recurring invoices were last written.
    recurring_day: String,
    /// The mailbox checker runs in its own thread; these connect it to the server.
    inbox_shared: Arc<Mutex<crate::imap::InboxSettings>>,
    inbox_now: Arc<AtomicBool>,
    inbox_rx: std::sync::mpsc::Receiver<InboxResult>,
}

impl Tenant {
    fn name(&self) -> String {
        self.stored.data.company.name.clone()
    }

    fn save(&mut self, change: Change<'_>) -> Result<(), String> {
        self.store.save(&self.stored, change)?;
        // One automatic snapshot per day, readable without the database.
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        if day != self.last_backup_day {
            let bdir = self.dir.join("Sicherungen");
            let text = serde_json::to_string_pretty(&self.stored.data).map_err(|e| e.to_string())?;
            if std::fs::create_dir_all(&bdir)
                .and_then(|_| std::fs::write(bdir.join(format!("daten-{day}.json")), text))
                .is_ok()
            {
                self.last_backup_day = day;
            }
        }
        Ok(())
    }

    /// Keeps a changed copy of the data once it is safely stored.
    fn commit(&mut self, copy: Data) -> Result<(), String> {
        let old = std::mem::replace(&mut self.stored.data, copy);
        self.stored.revision += 1;
        if let Err(e) = self.save(Change::Data { old: &old }) {
            self.stored.data = old;
            self.stored.revision -= 1;
            return Err(format!("Speichern fehlgeschlagen: {e}"));
        }
        Ok(())
    }

    fn op_reply(&self, id: Option<u64>) -> Reply {
        json_response(200, &OpReply { revision: self.stored.revision, id, data: self.stored.data.clone() })
    }

    /// Saves changed users; puts the old list back if saving fails.
    fn save_users(&mut self, before: Vec<User>) -> Reply {
        match self.save(Change::Users) {
            Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
            Err(e) => {
                self.stored.users = before;
                error(500, &format!("Speichern fehlgeschlagen: {e}"))
            }
        }
    }
}

#[derive(Clone)]
struct Session {
    user: String,
    role: Role,
    tenant: usize,
}

struct State {
    tenants: Vec<Tenant>,
    sessions: HashMap<String, Session>,
    name: String,
    is_new: bool,
    sso: Option<Verifier>,
    database: Option<String>,
    dir: PathBuf,
    /// Two-factor secrets being set up, per tenant and user.
    totp_pending: HashMap<(usize, String), String>,
    /// Last code step used per user, so a code works only once.
    totp_used: HashMap<(usize, String), u64>,
    /// Failed sign-ins per user name, to slow down guessing.
    failures: HashMap<String, (u32, Instant)>,
}

/// Folder of a company: the main one in the server folder, others below "Mandanten".
fn tenant_dir(base: &Path, id: &str) -> PathBuf {
    if id.is_empty() { base.to_path_buf() } else { base.join("Mandanten").join(id) }
}

fn schema_of(id: &str) -> Option<String> {
    (!id.is_empty()).then(|| format!("ok_{id}"))
}

/// Opens (or creates) a company. A new one gets the given first user.
fn open_tenant(base: &Path, database: Option<&str>, id: &str, first_user: User) -> Result<(Tenant, bool), String> {
    let dir = tenant_dir(base, id);
    let mut store: Box<dyn Store> = match database {
        Some(url) => Box::new(PgStore::connect_in(url, schema_of(id))?),
        None => Box::new(JsonStore { file: dir.join("server.json") }),
    };
    let loaded = store.load()?;
    let is_new = loaded.is_none();
    let stored = loaded.unwrap_or_else(|| Stored {
        data: Data::default(),
        users: vec![first_user],
        revision: 1,
        mail: Default::default(),
        inbox: Default::default(),
    });
    let mut t = Tenant {
        id: id.to_string(),
        stored,
        store,
        dir,
        last_backup_day: String::new(),
        recurring_day: String::new(),
        inbox_shared: Default::default(),
        inbox_now: Default::default(),
        inbox_rx: std::sync::mpsc::channel().1,
    };
    if is_new {
        t.save(Change::All)?;
    }
    Ok((t, is_new))
}

/// The ids of the further companies stored so far.
fn known_tenants(cfg: &ServerConfig) -> Result<Vec<String>, String> {
    let mut ids = match &cfg.database {
        Some(url) => PgStore::schemas(url)?.into_iter().filter_map(|s| s.strip_prefix("ok_").map(str::to_string)).collect(),
        None => std::fs::read_dir(cfg.dir.join("Mandanten"))
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().join("server.json").exists())
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    ids.sort();
    Ok(ids)
}

/// Loads the server data. A new server gets the user `admin` with password `admin`,
/// which should be changed right away.
fn open_state(cfg: &ServerConfig) -> Result<State, String> {
    let (main, is_new) = open_tenant(&cfg.dir, cfg.database.as_deref(), "", make_user("admin", Role::Admin, "admin"))?;
    let mut tenants = vec![main];
    for id in known_tenants(cfg)? {
        let (t, _) = open_tenant(&cfg.dir, cfg.database.as_deref(), &id, make_user("admin", Role::Admin, "admin"))?;
        tenants.push(t);
    }
    Ok(State {
        tenants,
        sessions: HashMap::new(),
        name: cfg.name.clone(),
        is_new,
        sso: cfg.sso.clone().map(Verifier::new),
        database: cfg.database.clone(),
        dir: cfg.dir.clone(),
        totp_pending: HashMap::new(),
        totp_used: HashMap::new(),
        failures: HashMap::new(),
    })
}

/// Runs the server until the process ends. `on_ready` is called once it listens.
pub fn run(cfg: ServerConfig, on_ready: impl FnOnce(Result<String, String>)) {
    let mut state = match open_state(&cfg) {
        Ok(s) => s,
        Err(e) => {
            on_ready(Err(e));
            return;
        }
    };
    // With HTTPS the HTTP server listens only inside; the TLS front end takes the port.
    let tls = match &cfg.tls {
        Some((cert, key)) => match crate::tls::load_config(cert, key) {
            Ok(c) => Some(c),
            Err(e) => {
                on_ready(Err(e));
                return;
            }
        },
        None => None,
    };
    let server = match &tls {
        Some(_) => Server::http(("127.0.0.1", 0)),
        None => Server::http((cfg.bind.as_str(), cfg.port)),
    };
    let server = match server {
        Ok(s) => s,
        Err(e) => {
            on_ready(Err(format!("Port {} kann nicht geöffnet werden: {e}", cfg.port)));
            return;
        }
    };
    if let Some(tls) = tls {
        let inner = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
        if let Err(e) = crate::tls::start(&cfg.bind, cfg.port, tls, inner) {
            on_ready(Err(e));
            return;
        }
    }
    let scheme = if cfg.tls.is_some() { "https" } else { "http" };
    let mut msg = format!(
        "{} läuft auf {scheme}://{}:{} – Daten: {}",
        cfg.name,
        cfg.bind,
        cfg.port,
        state.tenants[0].store.describe()
    );
    if state.tenants.len() > 1 {
        msg.push_str(&format!("\nMandanten: {}", state.tenants.iter().map(|t| t.name()).collect::<Vec<_>>().join(", ")));
    }
    msg.push_str(&format!("\nHandy-Version: {scheme}://<Adresse dieses Servers>:{}/ im Browser öffnen", cfg.port));
    let (pdftotext, pdftoppm, tesseract) = crate::ocr::tools();
    msg.push_str(&format!(
        "\nTexterkennung für Belege: {}",
        match (tesseract, pdftoppm && pdftotext) {
            (true, true) => "bereit (Tesseract, Poppler)".to_string(),
            (true, false) => "Fotos ja; für gescannte PDFs fehlt „poppler-utils“".to_string(),
            _ => "nicht installiert – E-Rechnungen und PDFs mit Text gehen trotzdem (für Scans/Fotos: apt install tesseract-ocr tesseract-ocr-deu poppler-utils)".to_string(),
        }
    ));
    if state.is_new {
        msg.push_str("\nNeuer Datenbestand – erste Anmeldung: Benutzer „admin“, Passwort „admin“ – bitte sofort ändern.");
    }
    on_ready(Ok(msg));
    for t in &mut state.tenants {
        start_inbox_checker(t);
    }
    loop {
        match server.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(Some(request)) => handle(&mut state, request),
            Ok(None) => {}
            Err(_) => break,
        }
        for t in &mut state.tenants {
            while let Ok(result) = t.inbox_rx.try_recv() {
                take_inbox_result(t, result);
            }
            write_due_recurring(t);
        }
    }
}

/// Once a day, writes the recurring invoices that are due.
fn write_due_recurring(t: &mut Tenant) {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    if t.recurring_day == day {
        return;
    }
    t.recurring_day = day;
    let today = crate::model::today();
    if t.stored.data.due_recurring(today) == 0 {
        return;
    }
    let mut copy = t.stored.data.clone();
    if copy.apply(Op::CreateDueRecurring(today), "Automatik").is_ok() {
        let _ = t.commit(copy);
    }
}

/// Checks the shared mailbox in the background, on its interval or when asked to.
fn start_inbox_checker(t: &mut Tenant) {
    *t.inbox_shared.lock().unwrap() = t.stored.inbox.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    t.inbox_rx = rx;
    let shared = t.inbox_shared.clone();
    let now = t.inbox_now.clone();
    std::thread::spawn(move || {
        let mut last: Option<std::time::Instant> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let s = shared.lock().unwrap().clone();
            let asked = now.swap(false, Ordering::SeqCst);
            let interval = std::time::Duration::from_secs(s.interval_minutes.max(1) as u64 * 60);
            let due = s.enabled && last.is_none_or(|t| t.elapsed() >= interval);
            if !(due || asked) {
                continue;
            }
            last = Some(std::time::Instant::now());
            let result = if s.host.trim().is_empty() {
                Err("Das Sammelpostfach ist noch nicht eingerichtet.".to_string())
            } else {
                crate::imap::fetch(&s)
            };
            if tx.send(result).is_err() {
                break;
            }
        }
    });
}

/// Turns fetched attachments into receipts and notes the result.
fn take_inbox_result(t: &mut Tenant, result: InboxResult) {
    let summary = match result {
        Ok((files, mails)) => {
            let mut added = 0;
            let mut problems = Vec::new();
            for f in files {
                let from = if f.from.is_empty() { "unbekannt" } else { &f.from };
                let source = if f.subject.is_empty() {
                    format!("E-Mail von {from}")
                } else {
                    format!("E-Mail von {from}: {}", f.subject)
                };
                match store_receipt(t, &f.file_name, &f.bytes, &source, "Sammelpostfach") {
                    Ok(_) => added += 1,
                    Err(e) => problems.push(e),
                }
            }
            let mut s = format!("{mails} neue E-Mail(s), {added} Beleg(e) übernommen");
            if !problems.is_empty() {
                s.push_str(&format!(" – {}", problems.join("; ")));
            }
            s
        }
        Err(e) => format!("Fehler: {e}"),
    };
    t.stored.inbox.last_check = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    t.stored.inbox.last_result = summary;
    let _ = t.save(Change::Inbox);
}

/// Stores an incoming file and its recognised receipt. Returns the receipt's id.
fn store_receipt(t: &mut Tenant, name: &str, bytes: &[u8], source: &str, user: &str) -> Result<u64, String> {
    crate::inbox::check_new_file(&t.stored.data, name, bytes)?;
    let receipt = crate::inbox::new_receipt(&t.stored.data, name, bytes, source);
    let mut copy = t.stored.data.clone();
    let id = copy.apply(Op::AddReceipt(Box::new(receipt)), user)?.ok_or("Beleg konnte nicht angelegt werden.")?;
    t.store.save_file(id, bytes)?;
    t.commit(copy)?;
    Ok(id)
}

fn content_type_for(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.ends_with(".pdf") {
        "application/pdf"
    } else if n.ends_with(".xml") {
        "application/xml"
    } else if n.ends_with(".png") {
        "image/png"
    } else if n.ends_with(".jpg") || n.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

fn json_response<T: Serialize>(status: u16, value: &T) -> Reply {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::from_data(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
}

fn error(status: u16, msg: &str) -> Reply {
    json_response(status, &ErrorReply { error: msg.to_string(), need_code: false })
}

fn text_response(body: &str, content_type: &str) -> Reply {
    Response::from_data(body.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
}

fn handle(state: &mut State, mut request: Request) {
    let mut raw = Vec::new();
    let _ = request.as_reader().take(64 * 1024 * 1024).read_to_end(&mut raw);
    let body = String::from_utf8(raw.clone()).unwrap_or_default();
    let token = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().trim_start_matches("Bearer ").to_string());
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let method = request.method().clone();
    let response = routes::route(state, &method, path, query, &body, &raw, token.as_deref());
    let _ = request.respond(response);
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}="))).map(url_decode)
}

fn tenant_index(state: &State, id: &str) -> Option<usize> {
    if id.trim().is_empty() {
        return Some(0);
    }
    state.tenants.iter().position(|t| t.id == id.trim())
}

/// Too many wrong passwords lock the user name for a few minutes.
fn locked_out(state: &State, user: &str) -> bool {
    state
        .failures
        .get(&user.to_lowercase())
        .is_some_and(|(n, since)| *n >= 8 && since.elapsed() < std::time::Duration::from_secs(300))
}

fn note_failure(state: &mut State, user: &str) {
    let e = state.failures.entry(user.to_lowercase()).or_insert((0, Instant::now()));
    if e.1.elapsed() > std::time::Duration::from_secs(300) {
        *e = (0, Instant::now());
    }
    e.0 += 1;
    e.1 = Instant::now();
}

fn login(state: &mut State, body: &str) -> Reply {
    let Ok(req) = serde_json::from_str::<LoginRequest>(body) else {
        return error(400, "Ungültige Anfrage.");
    };
    let Some(ti) = tenant_index(state, &req.tenant) else {
        return error(404, "Diese Firma gibt es auf dem Server nicht.");
    };
    if locked_out(state, &req.user) {
        return error(429, "Zu viele falsche Anmeldungen. Bitte in fünf Minuten erneut versuchen.");
    }
    let user = state.tenants[ti].stored.users.iter().find(|u| u.name.eq_ignore_ascii_case(req.user.trim())).cloned();
    match user {
        Some(u) if u.hash.is_empty() => error(401, "Dieses Konto meldet sich mit Microsoft 365 an."),
        Some(u) if hash_password(&u.salt, &req.password) == u.hash => {
            if !u.totp.is_empty() {
                if req.code.trim().is_empty() {
                    return json_response(
                        401,
                        &ErrorReply { error: "Bitte den Bestätigungscode aus Ihrer Authenticator-App eingeben.".into(), need_code: true },
                    );
                }
                let key = (ti, u.name.clone());
                let used = state.totp_used.get(&key).copied();
                match crate::totp::verify(&u.totp, &req.code, crate::totp::current_step()) {
                    Some(step) if used.is_none_or(|s| step > s) => {
                        state.totp_used.insert(key, step);
                    }
                    _ => {
                        note_failure(state, &req.user);
                        return json_response(401, &ErrorReply { error: "Der Bestätigungscode stimmt nicht.".into(), need_code: true });
                    }
                }
            }
            state.failures.remove(&req.user.to_lowercase());
            open_session(state, ti, u.name, u.role, req.remember)
        }
        _ => {
            note_failure(state, &req.user);
            error(401, "Benutzername oder Passwort ist falsch.")
        }
    }
}

/// Signing in again from a remembered workplace. The token stands for password and code.
fn token_login(state: &mut State, body: &str) -> Reply {
    let Ok(req) = serde_json::from_str::<TokenLoginRequest>(body) else {
        return error(400, "Ungültige Anfrage.");
    };
    let Some(ti) = tenant_index(state, &req.tenant) else {
        return error(404, "Diese Firma gibt es auf dem Server nicht.");
    };
    if locked_out(state, &req.user) {
        return error(429, "Zu viele falsche Anmeldungen. Bitte in fünf Minuten erneut versuchen.");
    }
    let today = crate::model::today();
    let hash = token_hash(&req.token);
    let t = &mut state.tenants[ti];
    let Some(u) = t.stored.users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(req.user.trim())) else {
        note_failure(state, &req.user);
        return error(401, "Die gespeicherte Anmeldung ist abgelaufen. Bitte neu anmelden.");
    };
    let Some(r) = u.remembered.iter_mut().find(|r| r.hash == hash && r.until >= today) else {
        note_failure(state, &req.user);
        return error(401, "Die gespeicherte Anmeldung ist abgelaufen. Bitte neu anmelden.");
    };
    // Each use keeps the workplace signed in for another month.
    r.until = today + chrono::Duration::days(REMEMBER_DAYS);
    let (name, role) = (u.name.clone(), u.role);
    let _ = t.save(Change::Users);
    open_session(state, ti, name, role, false)
}

/// Forgets a remembered workplace (on "Abmelden").
fn forget(state: &mut State, body: &str) -> Reply {
    let Ok(req) = serde_json::from_str::<TokenLoginRequest>(body) else {
        return error(400, "Ungültige Anfrage.");
    };
    let Some(ti) = tenant_index(state, &req.tenant) else {
        return error(404, "Diese Firma gibt es auf dem Server nicht.");
    };
    let hash = token_hash(&req.token);
    let t = &mut state.tenants[ti];
    let mut changed = false;
    for u in t.stored.users.iter_mut().filter(|u| u.name.eq_ignore_ascii_case(req.user.trim())) {
        let before = u.remembered.len();
        u.remembered.retain(|r| r.hash != hash);
        changed |= u.remembered.len() != before;
    }
    if changed {
        let _ = t.save(Change::Users);
    }
    json_response(200, &serde_json::json!({ "ok": true }))
}

/// A user who signs in only with Microsoft 365: no password at all.
fn sso_user(name: &str, role: Role) -> User {
    User { name: name.to_string(), role, ..User::default() }
}

fn open_session(state: &mut State, tenant: usize, name: String, role: Role, remember: bool) -> Reply {
    let token = random_hex(24);
    state.sessions.insert(token.clone(), Session { user: name.clone(), role, tenant });
    let mut remember_token = String::new();
    if remember {
        let today = crate::model::today();
        let t = &mut state.tenants[tenant];
        if let Some(u) = t.stored.users.iter_mut().find(|u| u.name == name) {
            remember_token = random_hex(32);
            u.remembered.retain(|r| r.until >= today);
            u.remembered.push(Remembered { hash: token_hash(&remember_token), until: today + chrono::Duration::days(REMEMBER_DAYS) });
            // At most ten workplaces per user; the oldest are forgotten.
            let excess = u.remembered.len().saturating_sub(10);
            u.remembered.drain(..excess);
            if t.save(Change::Users).is_err() {
                remember_token.clear();
            }
        }
    }
    let t = &state.tenants[tenant];
    let totp_enabled = t.stored.users.iter().any(|u| u.name == name && !u.totp.is_empty());
    json_response(
        200,
        &LoginReply {
            token,
            user: name,
            role,
            server_name: state.name.clone(),
            sso_enabled: state.sso.is_some(),
            totp_enabled,
            tenant: t.id.clone(),
            tenant_name: t.name(),
            remember_token,
        },
    )
}

/// Sign-in with an ID token from Microsoft 365.
fn sso_login(state: &mut State, body: &str) -> Reply {
    let Ok(req) = serde_json::from_str::<SsoLoginRequest>(body) else {
        return error(400, "Ungültige Anfrage.");
    };
    let Some(ti) = tenant_index(state, &req.tenant) else {
        return error(404, "Diese Firma gibt es auf dem Server nicht.");
    };
    let Some(verifier) = state.sso.as_mut() else {
        return error(404, "Auf diesem Server ist die Anmeldung mit Microsoft 365 nicht eingerichtet.");
    };
    let identity = match verifier.verify(&req.id_token, &req.nonce) {
        Ok(id) => id,
        Err(e) => return error(401, &e),
    };
    let auto_role = verifier.cfg.auto_role;
    if let Some(u) = state.tenants[ti].stored.users.iter().find(|u| u.name.eq_ignore_ascii_case(&identity.username)) {
        let (name, role) = (u.name.clone(), u.role);
        return open_session(state, ti, name, role, req.remember);
    }
    let Some(role) = auto_role else {
        return error(
            403,
            &format!(
                "Ihr Microsoft-Konto {} ist in OpenKontor noch nicht freigeschaltet. Bitte wenden Sie sich an den Administrator.",
                identity.username
            ),
        );
    };
    // First sign-in of a colleague: create the user with the configured role.
    let t = &mut state.tenants[ti];
    let before = t.stored.users.clone();
    t.stored.users.push(sso_user(&identity.username, role));
    if let Err(e) = t.save(Change::Users) {
        t.stored.users = before;
        return error(500, &format!("Speichern fehlgeschlagen: {e}"));
    }
    open_session(state, ti, identity.username, role, req.remember)
}

fn users_admins(users: &[User]) -> usize {
    users.iter().filter(|u| u.role == Role::Admin).count()
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const CLI_HELP: &str = "OpenKontor Server – Rechnungen & Buchhaltung für mehrere Arbeitsplätze

Aufruf:
  openkontor-server [Optionen]

Optionen:
  --db URL        PostgreSQL-Datenbank, z. B. postgres://openkontor:geheim@localhost/openkontor
                  (oder Umgebungsvariable OPENKONTOR_DATABASE_URL). Ohne Angabe: JSON-Datei.
  --port N        Port für die Arbeitsplätze (Standard 8470, oder OPENKONTOR_PORT)
  --bind ADRESSE  Netzwerkadresse (Standard 0.0.0.0 = alle)
  --data ORDNER   Ordner für die tägliche Sicherung (und die JSON-Datei ohne Datenbank)
  --name NAME     Name, den die Arbeitsplätze anzeigen

HTTPS (optional, z. B. mit einem Zertifikat von Let's Encrypt):
  --tls-cert DATEI      Zertifikat mit Zwischenzertifikaten (PEM)   (OPENKONTOR_TLS_CERT)
  --tls-key DATEI       Privater Schlüssel (PEM)                   (OPENKONTOR_TLS_KEY)

Anmeldung mit Microsoft 365 (optional):
  --sso-tenant ID       Verzeichnis-ID (Mandanten-ID) der Organisation   (OPENKONTOR_SSO_TENANT)
  --sso-client-id ID    Anwendungs-ID der App-Registrierung              (OPENKONTOR_SSO_CLIENT_ID)
  --sso-auto-role ROLLE Kollegen beim ersten Anmelden automatisch anlegen:
                        lesen | buchhaltung | vertrieb | belegerfassung
                        (ohne Angabe dürfen nur Benutzer hinein, die ein Administrator anlegt)
  --sso-authority URL   Anmeldeadresse, Standard https://login.microsoftonline.com
  --help          Diese Hilfe

Die Tabellen in PostgreSQL werden beim ersten Start automatisch angelegt. Weitere Firmen
(Mandanten) legt ein Administrator im Arbeitsplatz unter Einstellungen → Mandanten an.
Die Handy-Version öffnet man im Browser unter der Adresse des Servers.
";

/// Console server for `openkontor-server` and `openkontor --server`.
pub fn run_cli(args: &[String]) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{CLI_HELP}");
        return;
    }
    let mut cfg = ServerConfig {
        database: std::env::var("OPENKONTOR_DATABASE_URL").ok().filter(|v| !v.is_empty()),
        ..ServerConfig::default()
    };
    if let Some(p) = std::env::var("OPENKONTOR_PORT").ok().and_then(|v| v.parse().ok()) {
        cfg.port = p;
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let mut sso_tenant = env("OPENKONTOR_SSO_TENANT");
    let mut sso_client = env("OPENKONTOR_SSO_CLIENT_ID");
    let mut sso_authority = env("OPENKONTOR_SSO_AUTHORITY");
    let mut sso_auto = env("OPENKONTOR_SSO_AUTO_ROLE");
    let mut tls_cert = env("OPENKONTOR_TLS_CERT");
    let mut tls_key = env("OPENKONTOR_TLS_KEY");
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned();
        let mut takes_value = true;
        match (args[i].as_str(), value) {
            ("--port", Some(v)) => match v.parse() {
                Ok(p) => cfg.port = p,
                Err(_) => {
                    eprintln!("Fehler: ungültiger Port „{v}“");
                    std::process::exit(2);
                }
            },
            ("--bind", Some(v)) => cfg.bind = v,
            ("--data", Some(v)) => cfg.dir = Path::new(&v).to_path_buf(),
            ("--name", Some(v)) => cfg.name = v,
            ("--db", Some(v)) => cfg.database = Some(v),
            ("--tls-cert", Some(v)) => tls_cert = Some(v),
            ("--tls-key", Some(v)) => tls_key = Some(v),
            ("--sso-tenant", Some(v)) => sso_tenant = Some(v),
            ("--sso-client-id", Some(v)) => sso_client = Some(v),
            ("--sso-authority", Some(v)) => sso_authority = Some(v),
            ("--sso-auto-role", Some(v)) => sso_auto = Some(v),
            ("--server", _) => takes_value = false,
            (other, _) => {
                eprintln!("Fehler: unbekannte Option „{other}“ (siehe --help)");
                std::process::exit(2);
            }
        }
        i += if takes_value { 2 } else { 1 };
    }
    match (tls_cert, tls_key) {
        (None, None) => {}
        (Some(c), Some(k)) => cfg.tls = Some((c, k)),
        _ => {
            eprintln!("Fehler: Für HTTPS braucht es --tls-cert und --tls-key zusammen.");
            std::process::exit(2);
        }
    }
    match (sso_tenant, sso_client) {
        (None, None) => {}
        (Some(tenant), Some(client_id)) => {
            let tenant = tenant.trim().to_lowercase();
            if !crate::sso::is_guid(&tenant) {
                eprintln!("Fehler: --sso-tenant braucht die Verzeichnis-ID (Mandanten-ID) als GUID, z. B. 11111111-2222-3333-4444-555555555555.");
                std::process::exit(2);
            }
            let auto_role = match sso_auto.as_deref().map(|s| s.trim().to_lowercase()) {
                None => None,
                Some(r) if r == "lesen" || r == "read" => Some(Role::Lesen),
                Some(r) if r == "buchhaltung" || r == "accounting" => Some(Role::Buchhaltung),
                Some(r) if r == "vertrieb" || r == "sales" => Some(Role::Vertrieb),
                Some(r) if r == "belegerfassung" || r == "receipts" => Some(Role::Belegerfassung),
                Some(other) => {
                    eprintln!("Fehler: --sso-auto-role kennt nur „lesen“, „buchhaltung“, „vertrieb“ oder „belegerfassung“, nicht „{other}“.");
                    std::process::exit(2);
                }
            };
            cfg.sso = Some(SsoConfig {
                tenant,
                client_id: client_id.trim().to_string(),
                authority: sso_authority.unwrap_or_else(|| crate::sso::DEFAULT_AUTHORITY.to_string()),
                auto_role,
            });
        }
        _ => {
            eprintln!("Fehler: Für Microsoft 365 braucht es --sso-tenant und --sso-client-id zusammen.");
            std::process::exit(2);
        }
    }
    let sso_note = cfg.sso.as_ref().map(|s| {
        format!(
            "Anmeldung mit Microsoft 365 aktiv (Mandant {}, neue Kollegen: {}).",
            s.tenant,
            s.auto_role.map(|r| r.label()).unwrap_or("nur freigeschaltete Benutzer")
        )
    });
    run(cfg, move |r| match r {
        Ok(msg) => {
            println!("{msg}");
            if let Some(note) = sso_note {
                println!("{note}");
            }
        }
        Err(e) => {
            eprintln!("Fehler: {e}");
            std::process::exit(1);
        }
    });
}
