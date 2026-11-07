//! The self-hosted server: keeps one shared data set, the users and their sessions.
//! Workplaces talk to it over plain HTTP with JSON (put it behind a TLS proxy for use
//! over the internet).

use crate::model::{Data, Op};
use crate::sso::{SsoConfig, Verifier};
use crate::store::{Change, JsonStore, PgStore, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Request, Response, Server};

pub const DEFAULT_PORT: u16 = 8470;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Buchhaltung,
    Lesen,
}

impl Role {
    pub fn label(self) -> &'static str {
        crate::i18n::t(match self {
            Role::Admin => "Administrator",
            Role::Buchhaltung => "Buchhaltung",
            Role::Lesen => "Nur lesen",
        })
    }
    pub fn can_write(self) -> bool {
        self != Role::Lesen
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    pub role: Role,
    pub salt: String,
    pub hash: String,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    pub role: Role,
    /// Signs in only with Microsoft 365 (has no password).
    #[serde(default)]
    pub sso_only: bool,
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
    User {
        name: name.to_string(),
        role,
        hash: hash_password(&salt, password),
        salt,
    }
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
        }
    }
}

pub fn default_server_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OpenKontor")
        .join("Server")
}

struct State {
    stored: Stored,
    sessions: HashMap<String, (String, Role)>,
    store: Box<dyn Store>,
    dir: PathBuf,
    name: String,
    last_backup_day: String,
    is_new: bool,
    sso: Option<Verifier>,
    /// The mailbox checker runs in its own thread; these connect it to the server.
    inbox_shared: std::sync::Arc<std::sync::Mutex<crate::imap::InboxSettings>>,
    inbox_now: std::sync::Arc<std::sync::atomic::AtomicBool>,
    inbox_rx: std::sync::mpsc::Receiver<InboxResult>,
}

impl State {
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

    /// Saves changed users; puts the old list back if saving fails.
    fn save_users(&mut self, before: Vec<User>) -> Response<std::io::Cursor<Vec<u8>>> {
        match self.save(Change::Users) {
            Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
            Err(e) => {
                self.stored.users = before;
                error(500, &format!("Speichern fehlgeschlagen: {e}"))
            }
        }
    }
}

/// Loads (or creates) the server data. A new server gets the user `admin`
/// with password `admin`, which should be changed right away.
fn open_state(cfg: &ServerConfig) -> Result<State, String> {
    let mut store: Box<dyn Store> = match &cfg.database {
        Some(url) => Box::new(PgStore::connect(url)?),
        None => Box::new(JsonStore { file: cfg.dir.join("server.json") }),
    };
    let loaded = store.load()?;
    let is_new = loaded.is_none();
    let stored = loaded.unwrap_or_else(|| Stored {
        data: Data::default(),
        users: vec![make_user("admin", Role::Admin, "admin")],
        revision: 1,
        mail: Default::default(),
        inbox: Default::default(),
    });
    let mut st = State {
        stored,
        sessions: HashMap::new(),
        store,
        dir: cfg.dir.clone(),
        name: cfg.name.clone(),
        last_backup_day: String::new(),
        is_new,
        sso: cfg.sso.clone().map(Verifier::new),
        inbox_shared: Default::default(),
        inbox_now: Default::default(),
        inbox_rx: std::sync::mpsc::channel().1,
    };
    if is_new {
        st.save(Change::All)?;
    }
    Ok(st)
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
    let server = match Server::http((cfg.bind.as_str(), cfg.port)) {
        Ok(s) => s,
        Err(e) => {
            on_ready(Err(format!("Port {} kann nicht geöffnet werden: {e}", cfg.port)));
            return;
        }
    };
    let mut msg = format!(
        "{} läuft auf {}:{} – Daten: {}",
        cfg.name,
        cfg.bind,
        cfg.port,
        state.store.describe()
    );
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
    start_inbox_checker(&mut state);
    loop {
        match server.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(Some(request)) => handle(&mut state, request),
            Ok(None) => {}
            Err(_) => break,
        }
        while let Ok(result) = state.inbox_rx.try_recv() {
            take_inbox_result(&mut state, result);
        }
    }
}

type InboxResult = Result<(Vec<crate::imap::Fetched>, usize), String>;

/// Checks the shared mailbox in the background, on its interval or when asked to.
fn start_inbox_checker(state: &mut State) {
    *state.inbox_shared.lock().unwrap() = state.stored.inbox.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    state.inbox_rx = rx;
    let shared = state.inbox_shared.clone();
    let now = state.inbox_now.clone();
    std::thread::spawn(move || {
        let mut last: Option<std::time::Instant> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let s = shared.lock().unwrap().clone();
            let asked = now.swap(false, std::sync::atomic::Ordering::SeqCst);
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
fn take_inbox_result(state: &mut State, result: InboxResult) {
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
                match store_receipt(state, &f.file_name, &f.bytes, &source, "Sammelpostfach") {
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
    state.stored.inbox.last_check = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    state.stored.inbox.last_result = summary;
    let _ = state.save(Change::Inbox);
}

/// Stores an incoming file and its recognised receipt. Returns the receipt's id.
fn store_receipt(state: &mut State, name: &str, bytes: &[u8], source: &str, user: &str) -> Result<u64, String> {
    crate::inbox::check_new_file(&state.stored.data, name, bytes)?;
    let receipt = crate::inbox::new_receipt(&state.stored.data, name, bytes, source);
    let mut copy = state.stored.data.clone();
    let id = copy.apply(Op::AddReceipt(Box::new(receipt)), user)?.ok_or("Beleg konnte nicht angelegt werden.")?;
    state.store.save_file(id, bytes)?;
    let old = std::mem::replace(&mut state.stored.data, copy);
    state.stored.revision += 1;
    if let Err(e) = state.save(Change::Data { old: &old }) {
        state.stored.data = old;
        state.stored.revision -= 1;
        return Err(format!("Speichern fehlgeschlagen: {e}"));
    }
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

fn json_response<T: Serialize>(status: u16, value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::from_data(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
}

fn error(status: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(status, &ErrorReply { error: msg.to_string() })
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
    let response = route(state, &method, path, query, &body, &raw, token.as_deref());
    let _ = request.respond(response);
}

fn route(
    state: &mut State,
    method: &Method,
    path: &str,
    query: &str,
    body: &str,
    raw: &[u8],
    token: Option<&str>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    if (method, path) == (&Method::Get, "/api/info") {
        let sso = state.sso.as_ref().map(|v| v.cfg.info());
        return json_response(200, &serde_json::json!({ "name": state.name, "app": "OpenKontor", "sso": sso }));
    }
    if (method, path) == (&Method::Post, "/api/login/sso") {
        return sso_login(state, body);
    }
    if (method, path) == (&Method::Post, "/api/login") {
        let Ok(req) = serde_json::from_str::<LoginRequest>(body) else {
            return error(400, "Ungültige Anfrage.");
        };
        let user = state
            .stored
            .users
            .iter()
            .find(|u| u.name.eq_ignore_ascii_case(req.user.trim()));
        return match user {
            Some(u) if u.hash.is_empty() => {
                error(401, "Dieses Konto meldet sich mit Microsoft 365 an.")
            }
            Some(u) if hash_password(&u.salt, &req.password) == u.hash => {
                let (name, role) = (u.name.clone(), u.role);
                open_session(state, name, role)
            }
            _ => error(401, "Benutzername oder Passwort ist falsch."),
        };
    }

    let Some((user, role)) = token.and_then(|t| state.sessions.get(t).cloned()) else {
        return error(401, "Bitte neu anmelden.");
    };

    match (method, path) {
        (&Method::Get, "/api/state") => {
            let since: u64 = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("since="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let rev = state.stored.revision;
            let data = (since != rev).then(|| state.stored.data.clone());
            json_response(200, &StateReply { revision: rev, data })
        }
        (&Method::Post, "/api/op") => {
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            let op: Op = match serde_json::from_str(body) {
                Ok(op) => op,
                Err(e) => return error(400, &format!("Ungültige Änderung: {e}")),
            };
            if op.admin_only() && role != Role::Admin {
                return error(403, "Nur Administratoren dürfen das.");
            }
            let deleted_receipt = match op {
                Op::DeleteReceipt(id) => Some(id),
                _ => None,
            };
            let mut copy = state.stored.data.clone();
            match copy.apply(op, &user) {
                Ok(id) => {
                    // Only keep the change once it is safely stored.
                    let old = std::mem::replace(&mut state.stored.data, copy);
                    state.stored.revision += 1;
                    if let Err(e) = state.save(Change::Data { old: &old }) {
                        state.stored.data = old;
                        state.stored.revision -= 1;
                        return error(500, &format!("Speichern fehlgeschlagen: {e}"));
                    }
                    if let Some(rid) = deleted_receipt {
                        let _ = state.store.delete_file(rid);
                    }
                    json_response(
                        200,
                        &OpReply { revision: state.stored.revision, id, data: state.stored.data.clone() },
                    )
                }
                Err(e) => error(409, &e),
            }
        }
        (&Method::Post, "/api/receipts") => {
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            let name = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("name="))
                .map(url_decode)
                .unwrap_or_else(|| "beleg.pdf".into());
            let source = format!("Hochgeladen von {user}");
            match store_receipt(state, &name, raw, &source, &user) {
                Ok(id) => json_response(
                    200,
                    &OpReply { revision: state.stored.revision, id: Some(id), data: state.stored.data.clone() },
                ),
                Err(e) => error(409, &e),
            }
        }
        (&Method::Get, p) if p.starts_with("/api/receipts/") && p.ends_with("/file") => {
            let id: u64 = p["/api/receipts/".len()..p.len() - "/file".len()].parse().unwrap_or(0);
            let Some(r) = state.stored.data.receipts.iter().find(|r| r.id == id).cloned() else {
                return error(404, "Beleg nicht gefunden.");
            };
            match state.store.load_file(id) {
                Ok(bytes) => Response::from_data(bytes)
                    .with_header(Header::from_bytes("Content-Type", content_type_for(&r.file_name)).unwrap()),
                Err(e) => error(404, &e),
            }
        }
        (&Method::Post, p) if p.starts_with("/api/receipts/") && p.ends_with("/recognize") => {
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            let id: u64 = p["/api/receipts/".len()..p.len() - "/recognize".len()].parse().unwrap_or(0);
            let Some(mut r) = state.stored.data.receipts.iter().find(|r| r.id == id).cloned() else {
                return error(404, "Beleg nicht gefunden.");
            };
            let bytes = match state.store.load_file(id) {
                Ok(b) => b,
                Err(e) => return error(404, &e),
            };
            r.category.clear();
            crate::inbox::recognize(&state.stored.data, &mut r, &bytes);
            let mut copy = state.stored.data.clone();
            if let Err(e) = copy.apply(Op::SaveReceipt(Box::new(r)), &user) {
                return error(409, &e);
            }
            let old = std::mem::replace(&mut state.stored.data, copy);
            state.stored.revision += 1;
            if let Err(e) = state.save(Change::Data { old: &old }) {
                state.stored.data = old;
                state.stored.revision -= 1;
                return error(500, &format!("Speichern fehlgeschlagen: {e}"));
            }
            json_response(200, &OpReply { revision: state.stored.revision, id: Some(id), data: state.stored.data.clone() })
        }
        (&Method::Get, "/api/inbox/settings") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen das Sammelpostfach einrichten.");
            }
            let mut settings = state.stored.inbox.clone();
            let password_set = !settings.password.is_empty();
            settings.password.clear();
            json_response(200, &serde_json::json!({ "settings": settings, "password_set": password_set }))
        }
        (&Method::Post, "/api/inbox/settings") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen das Sammelpostfach einrichten.");
            }
            let Ok(mut settings) = serde_json::from_str::<crate::imap::InboxSettings>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            if settings.password.is_empty() {
                settings.password = state.stored.inbox.password.clone();
            }
            settings.last_check = state.stored.inbox.last_check.clone();
            settings.last_result = state.stored.inbox.last_result.clone();
            let before = std::mem::replace(&mut state.stored.inbox, settings);
            match state.save(Change::Inbox) {
                Ok(()) => {
                    *state.inbox_shared.lock().unwrap() = state.stored.inbox.clone();
                    json_response(200, &serde_json::json!({ "ok": true }))
                }
                Err(e) => {
                    state.stored.inbox = before;
                    error(500, &format!("Speichern fehlgeschlagen: {e}"))
                }
            }
        }
        (&Method::Post, "/api/inbox/check") => {
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            state.inbox_now.store(true, std::sync::atomic::Ordering::SeqCst);
            json_response(200, &serde_json::json!({ "ok": true }))
        }
        (&Method::Get, "/api/mail/settings") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen den E-Mail-Versand einrichten.");
            }
            let mut settings = state.stored.mail.clone();
            let password_set = !settings.password.is_empty();
            settings.password.clear();
            json_response(200, &MailSettingsReply { settings, password_set })
        }
        (&Method::Post, "/api/mail/settings") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen den E-Mail-Versand einrichten.");
            }
            let Ok(mut settings) = serde_json::from_str::<crate::mail::MailSettings>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            if settings.password.is_empty() {
                settings.password = state.stored.mail.password.clone();
            }
            let before = std::mem::replace(&mut state.stored.mail, settings);
            match state.save(Change::Mail) {
                Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
                Err(e) => {
                    state.stored.mail = before;
                    error(500, &format!("Speichern fehlgeschlagen: {e}"))
                }
            }
        }
        (&Method::Post, "/api/mail/test") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen den E-Mail-Versand einrichten.");
            }
            let Ok(req) = serde_json::from_str::<crate::mail::TestRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let text = format!(
                "Diese Test-E-Mail wurde vom OpenKontor-Server „{}“ verschickt. Der E-Mail-Versand funktioniert.",
                state.name
            );
            match crate::mail::send(&state.stored.mail, &req.to, "", "OpenKontor: Test-E-Mail", &text, vec![]) {
                Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
                Err(e) => error(502, &e),
            }
        }
        (&Method::Post, "/api/mail/send") => {
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            let req: crate::mail::SendRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => return error(400, &format!("Ungültige Anfrage: {e}")),
            };
            let mut copy = state.stored.data.clone();
            if let Err(e) = crate::mail::send_document(&mut copy, &state.stored.mail, &req, &user) {
                return error(502, &e);
            }
            // The e-mail is out; now keep the record of it.
            let old = std::mem::replace(&mut state.stored.data, copy);
            state.stored.revision += 1;
            if let Err(e) = state.save(Change::Data { old: &old }) {
                state.stored.data = old;
                state.stored.revision -= 1;
                return error(500, &format!("E-Mail gesendet, aber der Eintrag konnte nicht gespeichert werden: {e}"));
            }
            json_response(200, &OpReply { revision: state.stored.revision, id: Some(req.doc_id), data: state.stored.data.clone() })
        }
        (&Method::Post, "/api/password") => {
            let Ok(req) = serde_json::from_str::<PasswordRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            if req.new.len() < 6 {
                return error(400, "Das neue Passwort braucht mindestens 6 Zeichen.");
            }
            let before = state.stored.users.clone();
            let Some(u) = state.stored.users.iter_mut().find(|u| u.name == user) else {
                return error(404, "Benutzer nicht gefunden.");
            };
            if u.hash.is_empty() {
                return error(409, "Dieses Konto meldet sich mit Microsoft 365 an.");
            }
            if hash_password(&u.salt, &req.old) != u.hash {
                return error(403, "Das bisherige Passwort stimmt nicht.");
            }
            *u = make_user(&u.name.clone(), u.role, &req.new);
            state.save_users(before)
        }
        (&Method::Get, "/api/users") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen Benutzer verwalten.");
            }
            let list: Vec<UserInfo> = state
                .stored
                .users
                .iter()
                .map(|u| UserInfo { name: u.name.clone(), role: u.role, sso_only: u.hash.is_empty() })
                .collect();
            json_response(200, &list)
        }
        (&Method::Post, "/api/users") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen Benutzer verwalten.");
            }
            let Ok(req) = serde_json::from_str::<SaveUserRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let name = req.name.trim().to_string();
            if name.is_empty() {
                return error(400, "Bitte einen Benutzernamen angeben.");
            }
            if req.sso_only && state.sso.is_none() {
                return error(400, "Auf diesem Server ist die Anmeldung mit Microsoft 365 nicht eingerichtet.");
            }
            let before = state.stored.users.clone();
            let users = &mut state.stored.users;
            if let Some(u) = users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(&name)) {
                if u.role == Role::Admin && req.role != Role::Admin
                    && users_admins(users) <= 1
                {
                    return error(409, "Es muss mindestens einen Administrator geben.");
                }
                let u = users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(&name)).unwrap();
                u.role = req.role;
                if req.sso_only {
                    *u = sso_user(&u.name.clone(), req.role);
                } else if !req.password.is_empty() {
                    if req.password.len() < 6 {
                        return error(400, "Das Passwort braucht mindestens 6 Zeichen.");
                    }
                    *u = make_user(&u.name.clone(), req.role, &req.password);
                }
            } else if req.sso_only {
                users.push(sso_user(&name, req.role));
            } else {
                if req.password.len() < 6 {
                    return error(400, "Das Passwort braucht mindestens 6 Zeichen.");
                }
                users.push(make_user(&name, req.role, &req.password));
            }
            // Sessions keep the role they logged in with; make changes take effect now.
            for (n, r) in state.sessions.values_mut() {
                if n.eq_ignore_ascii_case(&name) {
                    *r = req.role;
                }
            }
            state.save_users(before)
        }
        (&Method::Delete, p) if p.starts_with("/api/users/") => {
            if role != Role::Admin {
                return error(403, "Nur Administratoren dürfen Benutzer verwalten.");
            }
            let name = url_decode(&p["/api/users/".len()..]);
            if name.eq_ignore_ascii_case(&user) {
                return error(409, "Sie können sich nicht selbst löschen.");
            }
            let before = state.stored.users.clone();
            state.stored.users.retain(|u| !u.name.eq_ignore_ascii_case(&name));
            state.sessions.retain(|_, (n, _)| !n.eq_ignore_ascii_case(&name));
            state.save_users(before)
        }
        _ => error(404, "Unbekannte Adresse."),
    }
}

/// A user who signs in only with Microsoft 365: no password at all.
fn sso_user(name: &str, role: Role) -> User {
    User { name: name.to_string(), role, salt: String::new(), hash: String::new() }
}

fn open_session(state: &mut State, name: String, role: Role) -> Response<std::io::Cursor<Vec<u8>>> {
    let token = random_hex(24);
    state.sessions.insert(token.clone(), (name.clone(), role));
    json_response(
        200,
        &LoginReply { token, user: name, role, server_name: state.name.clone(), sso_enabled: state.sso.is_some() },
    )
}

/// Sign-in with an ID token from Microsoft 365.
fn sso_login(state: &mut State, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let Some(verifier) = state.sso.as_mut() else {
        return error(404, "Auf diesem Server ist die Anmeldung mit Microsoft 365 nicht eingerichtet.");
    };
    let Ok(req) = serde_json::from_str::<SsoLoginRequest>(body) else {
        return error(400, "Ungültige Anfrage.");
    };
    let identity = match verifier.verify(&req.id_token, &req.nonce) {
        Ok(id) => id,
        Err(e) => return error(401, &e),
    };
    let auto_role = verifier.cfg.auto_role;
    if let Some(u) = state.stored.users.iter().find(|u| u.name.eq_ignore_ascii_case(&identity.username)) {
        let (name, role) = (u.name.clone(), u.role);
        return open_session(state, name, role);
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
    let before = state.stored.users.clone();
    state.stored.users.push(sso_user(&identity.username, role));
    if let Err(e) = state.save(Change::Users) {
        state.stored.users = before;
        return error(500, &format!("Speichern fehlgeschlagen: {e}"));
    }
    open_session(state, identity.username, role)
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
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
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

Anmeldung mit Microsoft 365 (optional):
  --sso-tenant ID       Verzeichnis-ID (Mandanten-ID) der Organisation   (OPENKONTOR_SSO_TENANT)
  --sso-client-id ID    Anwendungs-ID der App-Registrierung              (OPENKONTOR_SSO_CLIENT_ID)
  --sso-auto-role ROLLE Kollegen beim ersten Anmelden automatisch anlegen: lesen | buchhaltung
                        (ohne Angabe dürfen nur Benutzer hinein, die ein Administrator anlegt)
  --sso-authority URL   Anmeldeadresse, Standard https://login.microsoftonline.com
  --help          Diese Hilfe

Die Tabellen in PostgreSQL werden beim ersten Start automatisch angelegt.
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
                Some(other) => {
                    eprintln!("Fehler: --sso-auto-role kennt nur „lesen“ oder „buchhaltung“, nicht „{other}“.");
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
