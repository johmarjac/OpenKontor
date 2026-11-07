//! Where the server keeps its data: a JSON file (quick start on one machine)
//! or a PostgreSQL database with one table per kind of record.

use crate::model::{Article, Booking, Contact, Data, Document, Line, LogEntry};
use crate::server::{Stored, User};
use postgres::{Client, NoTls, Transaction};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::PathBuf;

/// What changed since the last save, so a database only writes what it must.
pub enum Change<'a> {
    Data { old: &'a Data },
    Users,
    /// The outgoing mail server settings.
    Mail,
    /// The shared mailbox settings.
    Inbox,
    All,
}

pub trait Store: Send {
    /// A short description for the start message, e.g. "PostgreSQL openkontor@db".
    fn describe(&self) -> String;
    /// The saved state, or `None` for a brand-new store.
    fn load(&mut self) -> Result<Option<Stored>, String>;
    fn save(&mut self, stored: &Stored, change: Change<'_>) -> Result<(), String>;
    /// Keeps the original file of a receipt. Files are written once and never changed.
    fn save_file(&mut self, id: u64, bytes: &[u8]) -> Result<(), String>;
    fn load_file(&mut self, id: u64) -> Result<Vec<u8>, String>;
    /// Removes the file of a receipt that was deleted.
    fn delete_file(&mut self, id: u64) -> Result<(), String>;
}

/// Where receipt files live without a database: a folder next to the data file.
fn file_path(dir: &std::path::Path, id: u64) -> PathBuf {
    dir.join("Belege").join(format!("{id}.bin"))
}

// ---- JSON file ------------------------------------------------------------------

pub struct JsonStore {
    pub file: PathBuf,
}

impl Store for JsonStore {
    fn describe(&self) -> String {
        format!("Datei {}", self.file.display())
    }

    fn save_file(&mut self, id: u64, bytes: &[u8]) -> Result<(), String> {
        let path = file_path(self.file.parent().unwrap_or(std::path::Path::new(".")), id);
        if path.exists() {
            return Err("Diese Belegdatei gibt es schon.".into());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|e| e.to_string())
    }

    fn delete_file(&mut self, id: u64) -> Result<(), String> {
        let path = file_path(self.file.parent().unwrap_or(std::path::Path::new(".")), id);
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }

    fn load_file(&mut self, id: u64) -> Result<Vec<u8>, String> {
        let path = file_path(self.file.parent().unwrap_or(std::path::Path::new(".")), id);
        std::fs::read(path).map_err(|_| "Die Belegdatei wurde nicht gefunden.".to_string())
    }

    fn load(&mut self) -> Result<Option<Stored>, String> {
        if !self.file.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&self.file).map_err(|e| e.to_string())?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{} ist beschädigt: {e}", self.file.display()))
    }

    fn save(&mut self, stored: &Stored, _change: Change<'_>) -> Result<(), String> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(stored).map_err(|e| e.to_string())?;
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.file).map_err(|e| e.to_string())
    }
}

// ---- PostgreSQL -----------------------------------------------------------------

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS users (
    name TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    salt TEXT NOT NULL,
    hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS contacts (
    id       BIGINT PRIMARY KEY,
    number   TEXT NOT NULL,
    kind     TEXT NOT NULL,
    company  TEXT NOT NULL,
    person   TEXT NOT NULL,
    street   TEXT NOT NULL,
    zip_city TEXT NOT NULL,
    country  TEXT NOT NULL,
    email    TEXT NOT NULL,
    phone    TEXT NOT NULL,
    vat_id   TEXT NOT NULL,
    note     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS articles (
    id              BIGINT PRIMARY KEY,
    number          TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL,
    unit            TEXT NOT NULL,
    price_net_cents BIGINT NOT NULL,
    vat_percent     SMALLINT NOT NULL,
    service         BOOLEAN NOT NULL
);
CREATE TABLE IF NOT EXISTS documents (
    id           BIGINT PRIMARY KEY,
    kind         TEXT NOT NULL,
    number       TEXT NOT NULL,
    contact_id   BIGINT,
    doc_date     DATE NOT NULL,
    service_date TEXT NOT NULL,
    due_days     INTEGER NOT NULL,
    title        TEXT NOT NULL,
    intro        TEXT NOT NULL,
    outro        TEXT NOT NULL,
    status       TEXT NOT NULL,
    paid_date    DATE,
    source_id    BIGINT
);
CREATE TABLE IF NOT EXISTS document_lines (
    document_id     BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    pos             INTEGER NOT NULL,
    article_id      BIGINT,
    text            TEXT NOT NULL,
    qty             DOUBLE PRECISION NOT NULL,
    unit            TEXT NOT NULL,
    price_net_cents BIGINT NOT NULL,
    vat_percent     SMALLINT NOT NULL,
    discount_pct    DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (document_id, pos)
);
CREATE TABLE IF NOT EXISTS bookings (
    id           BIGINT PRIMARY KEY,
    booking_date DATE NOT NULL,
    kind         TEXT NOT NULL,
    category     TEXT NOT NULL,
    text         TEXT NOT NULL,
    partner      TEXT NOT NULL,
    receipt      TEXT NOT NULL,
    gross_cents  BIGINT NOT NULL,
    vat_percent  SMALLINT NOT NULL,
    document_id  BIGINT,
    payment      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS change_log (
    id         BIGSERIAL PRIMARY KEY,
    logged_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    time_text  TEXT NOT NULL,
    user_name  TEXT NOT NULL,
    action     TEXT NOT NULL
);
ALTER TABLE contacts ADD COLUMN IF NOT EXISTS language TEXT NOT NULL DEFAULT 'de';
ALTER TABLE contacts ADD COLUMN IF NOT EXISTS payment_term_id BIGINT;
ALTER TABLE contacts ADD COLUMN IF NOT EXISTS buyer_reference TEXT NOT NULL DEFAULT '';
ALTER TABLE contacts ADD COLUMN IF NOT EXISTS einvoice TEXT;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS term_name TEXT NOT NULL DEFAULT '';
ALTER TABLE documents ADD COLUMN IF NOT EXISTS discounts TEXT NOT NULL DEFAULT '[]';
ALTER TABLE documents ADD COLUMN IF NOT EXISTS paid_discount_pct DOUBLE PRECISION NOT NULL DEFAULT 0;
ALTER TABLE documents ADD COLUMN IF NOT EXISTS emails TEXT NOT NULL DEFAULT '[]';
ALTER TABLE documents ADD COLUMN IF NOT EXISTS reminders TEXT NOT NULL DEFAULT '[]';
CREATE INDEX IF NOT EXISTS documents_contact ON documents(contact_id);
CREATE INDEX IF NOT EXISTS bookings_date ON bookings(booking_date);
ALTER TABLE bookings ADD COLUMN IF NOT EXISTS receipt_id BIGINT;
CREATE TABLE IF NOT EXISTS receipts (
    id      BIGINT PRIMARY KEY,
    status  TEXT NOT NULL,
    content TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS receipt_files (
    id      BIGINT PRIMARY KEY,
    content BYTEA NOT NULL
);
"#;

/// The change log is never deleted from the database; the workplaces see the newest entries.
const LOG_SHOWN: i64 = 5000;

pub struct PgStore {
    url: String,
    client: Client,
}

fn pg_err(e: postgres::Error) -> String {
    format!("Datenbank: {e}")
}

/// Enum values are stored as their names ("Kunde", "Offen" …).
fn name_of<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn from_name<T: DeserializeOwned>(s: String) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(s.clone()))
        .map_err(|_| format!("Unbekannter Wert „{s}“ in der Datenbank"))
}

fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

/// Ids of `new` items that are new or changed, and ids that disappeared.
fn diff<T: Serialize>(old: &[T], new: &[T], id: impl Fn(&T) -> u64) -> (Vec<u64>, Vec<u64>) {
    let old_by_id: HashMap<u64, &T> = old.iter().map(|x| (id(x), x)).collect();
    let new_ids: std::collections::HashSet<u64> = new.iter().map(&id).collect();
    let changed = new
        .iter()
        .filter(|x| old_by_id.get(&id(x)).is_none_or(|o| !same(*o, *x)))
        .map(&id)
        .collect();
    let removed = old.iter().map(&id).filter(|i| !new_ids.contains(i)).collect();
    (changed, removed)
}

fn same_entry(a: &LogEntry, b: &LogEntry) -> bool {
    a.time == b.time && a.user == b.user && a.action == b.action
}

impl PgStore {
    pub fn connect(url: &str) -> Result<PgStore, String> {
        let mut client = Client::connect(url, NoTls).map_err(pg_err)?;
        client.batch_execute(SCHEMA).map_err(pg_err)?;
        Ok(PgStore { url: url.to_string(), client })
    }

    /// Reconnects once if the database connection was lost (e.g. after a database restart).
    fn ensure_connected(&mut self) -> Result<(), String> {
        if self.client.is_closed() {
            self.client = Client::connect(&self.url, NoTls).map_err(pg_err)?;
        }
        Ok(())
    }

    fn put_setting(tx: &mut Transaction<'_>, key: &str, value: &str) -> Result<(), String> {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
            &[&key, &value],
        )
        .map_err(pg_err)?;
        Ok(())
    }

    fn put_contact(tx: &mut Transaction<'_>, c: &Contact) -> Result<(), String> {
        tx.execute(
            "INSERT INTO contacts (id, number, kind, company, person, street, zip_city, country, email, phone, vat_id, note,
               language, payment_term_id, buyer_reference, einvoice)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
             ON CONFLICT (id) DO UPDATE SET number=EXCLUDED.number, kind=EXCLUDED.kind, company=EXCLUDED.company,
               person=EXCLUDED.person, street=EXCLUDED.street, zip_city=EXCLUDED.zip_city, country=EXCLUDED.country,
               email=EXCLUDED.email, phone=EXCLUDED.phone, vat_id=EXCLUDED.vat_id, note=EXCLUDED.note,
               language=EXCLUDED.language, payment_term_id=EXCLUDED.payment_term_id,
               buyer_reference=EXCLUDED.buyer_reference, einvoice=EXCLUDED.einvoice",
            &[
                &(c.id as i64), &c.number, &name_of(&c.kind), &c.company, &c.person, &c.street,
                &c.zip_city, &c.country, &c.email, &c.phone, &c.vat_id, &c.note, &c.language.code(),
                &c.payment_term_id.map(|v| v as i64), &c.buyer_reference, &c.einvoice.map(|f| name_of(&f)),
            ],
        )
        .map_err(pg_err)?;
        Ok(())
    }

    fn put_article(tx: &mut Transaction<'_>, a: &Article) -> Result<(), String> {
        tx.execute(
            "INSERT INTO articles (id, number, name, description, unit, price_net_cents, vat_percent, service)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (id) DO UPDATE SET number=EXCLUDED.number, name=EXCLUDED.name, description=EXCLUDED.description,
               unit=EXCLUDED.unit, price_net_cents=EXCLUDED.price_net_cents, vat_percent=EXCLUDED.vat_percent,
               service=EXCLUDED.service",
            &[
                &(a.id as i64), &a.number, &a.name, &a.description, &a.unit, &a.price_net,
                &(a.vat as i16), &a.service,
            ],
        )
        .map_err(pg_err)?;
        Ok(())
    }

    fn put_document(tx: &mut Transaction<'_>, d: &Document) -> Result<(), String> {
        tx.execute(
            "INSERT INTO documents (id, kind, number, contact_id, doc_date, service_date, due_days, title, intro, outro,
               status, paid_date, source_id, term_name, discounts, paid_discount_pct, emails, reminders)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
             ON CONFLICT (id) DO UPDATE SET kind=EXCLUDED.kind, number=EXCLUDED.number, contact_id=EXCLUDED.contact_id,
               doc_date=EXCLUDED.doc_date, service_date=EXCLUDED.service_date, due_days=EXCLUDED.due_days,
               title=EXCLUDED.title, intro=EXCLUDED.intro, outro=EXCLUDED.outro, status=EXCLUDED.status,
               paid_date=EXCLUDED.paid_date, source_id=EXCLUDED.source_id, term_name=EXCLUDED.term_name,
               discounts=EXCLUDED.discounts, paid_discount_pct=EXCLUDED.paid_discount_pct,
               emails=EXCLUDED.emails, reminders=EXCLUDED.reminders",
            &[
                &(d.id as i64), &name_of(&d.kind), &d.number, &d.contact_id.map(|v| v as i64),
                &d.date, &d.service_date, &(d.due_days as i32), &d.title, &d.intro, &d.outro,
                &name_of(&d.status), &d.paid_date, &d.source_id.map(|v| v as i64), &d.term_name,
                &serde_json::to_string(&d.discounts).unwrap_or_else(|_| "[]".into()), &d.paid_discount_pct,
                &serde_json::to_string(&d.emails).unwrap_or_else(|_| "[]".into()),
                &serde_json::to_string(&d.reminders).unwrap_or_else(|_| "[]".into()),
            ],
        )
        .map_err(pg_err)?;
        tx.execute("DELETE FROM document_lines WHERE document_id = $1", &[&(d.id as i64)])
            .map_err(pg_err)?;
        for (pos, l) in d.lines.iter().enumerate() {
            tx.execute(
                "INSERT INTO document_lines (document_id, pos, article_id, text, qty, unit, price_net_cents,
                   vat_percent, discount_pct)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[
                    &(d.id as i64), &(pos as i32), &l.article_id.map(|v| v as i64), &l.text, &l.qty,
                    &l.unit, &l.price_net, &(l.vat as i16), &l.discount_pct,
                ],
            )
            .map_err(pg_err)?;
        }
        Ok(())
    }

    fn put_booking(tx: &mut Transaction<'_>, b: &Booking) -> Result<(), String> {
        tx.execute(
            "INSERT INTO bookings (id, booking_date, kind, category, text, partner, receipt, gross_cents, vat_percent,
               document_id, payment, receipt_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (id) DO UPDATE SET booking_date=EXCLUDED.booking_date, kind=EXCLUDED.kind,
               category=EXCLUDED.category, text=EXCLUDED.text, partner=EXCLUDED.partner, receipt=EXCLUDED.receipt,
               gross_cents=EXCLUDED.gross_cents, vat_percent=EXCLUDED.vat_percent, document_id=EXCLUDED.document_id,
               payment=EXCLUDED.payment, receipt_id=EXCLUDED.receipt_id",
            &[
                &(b.id as i64), &b.date, &name_of(&b.kind), &b.category, &b.text, &b.partner,
                &b.receipt, &b.gross, &(b.vat as i16), &b.doc_id.map(|v| v as i64), &b.payment,
                &b.receipt_id.map(|v| v as i64),
            ],
        )
        .map_err(pg_err)?;
        Ok(())
    }

    fn delete_ids(tx: &mut Transaction<'_>, table: &str, ids: &[u64]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<i64> = ids.iter().map(|v| *v as i64).collect();
        tx.execute(&format!("DELETE FROM {table} WHERE id = ANY($1)"), &[&ids])
            .map_err(pg_err)?;
        Ok(())
    }

    fn write_data(tx: &mut Transaction<'_>, old: &Data, new: &Data, revision: u64) -> Result<(), String> {
        let company = serde_json::to_string(&new.company).map_err(|e| e.to_string())?;
        Self::put_setting(tx, "company", &company)?;
        Self::put_setting(tx, "next_id", &new.next_id.to_string())?;
        Self::put_setting(tx, "revision", &revision.to_string())?;

        let (changed, removed) = diff(&old.contacts, &new.contacts, |c| c.id);
        for c in new.contacts.iter().filter(|c| changed.contains(&c.id)) {
            Self::put_contact(tx, c)?;
        }
        Self::delete_ids(tx, "contacts", &removed)?;

        let (changed, removed) = diff(&old.articles, &new.articles, |a| a.id);
        for a in new.articles.iter().filter(|a| changed.contains(&a.id)) {
            Self::put_article(tx, a)?;
        }
        Self::delete_ids(tx, "articles", &removed)?;

        let (changed, removed) = diff(&old.documents, &new.documents, |d| d.id);
        for d in new.documents.iter().filter(|d| changed.contains(&d.id)) {
            Self::put_document(tx, d)?;
        }
        Self::delete_ids(tx, "documents", &removed)?;

        let (changed, removed) = diff(&old.receipts, &new.receipts, |r| r.id);
        for r in new.receipts.iter().filter(|r| changed.contains(&r.id)) {
            let json = serde_json::to_string(r).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO receipts (id, status, content) VALUES ($1, $2, $3)
                 ON CONFLICT (id) DO UPDATE SET status = EXCLUDED.status, content = EXCLUDED.content",
                &[&(r.id as i64), &r.status.name(), &json],
            )
            .map_err(pg_err)?;
        }
        Self::delete_ids(tx, "receipts", &removed)?;

        let (changed, removed) = diff(&old.bookings, &new.bookings, |b| b.id);
        for b in new.bookings.iter().filter(|b| changed.contains(&b.id)) {
            Self::put_booking(tx, b)?;
        }
        Self::delete_ids(tx, "bookings", &removed)?;

        // New log entries are the ones after the last entry the old state knew.
        let start = match old.log.last() {
            Some(last) => new
                .log
                .iter()
                .rposition(|e| same_entry(e, last))
                .map(|i| i + 1)
                .unwrap_or(0),
            None => 0,
        };
        for e in &new.log[start..] {
            tx.execute(
                "INSERT INTO change_log (time_text, user_name, action) VALUES ($1, $2, $3)",
                &[&e.time, &e.user, &e.action],
            )
            .map_err(pg_err)?;
        }
        Ok(())
    }

    fn write_users(tx: &mut Transaction<'_>, users: &[User]) -> Result<(), String> {
        tx.execute("DELETE FROM users", &[]).map_err(pg_err)?;
        for u in users {
            tx.execute(
                "INSERT INTO users (name, role, salt, hash) VALUES ($1, $2, $3, $4)",
                &[&u.name, &name_of(&u.role), &u.salt, &u.hash],
            )
            .map_err(pg_err)?;
        }
        Ok(())
    }

    fn write_mail(tx: &mut Transaction<'_>, mail: &crate::mail::MailSettings) -> Result<(), String> {
        let text = serde_json::to_string(mail).map_err(|e| e.to_string())?;
        Self::put_setting(tx, "mail", &text)
    }

    fn read_all(&mut self) -> Result<Stored, String> {
        let c = &mut self.client;
        let mut settings: HashMap<String, String> = HashMap::new();
        for r in c.query("SELECT key, value FROM settings", &[]).map_err(pg_err)? {
            settings.insert(r.get(0), r.get(1));
        }
        let mut data = Data {
            company: settings
                .get("company")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
            next_id: settings.get("next_id").and_then(|s| s.parse().ok()).unwrap_or(0),
            ..Data::default()
        };
        let revision = settings.get("revision").and_then(|s| s.parse().ok()).unwrap_or(1);

        for r in c
            .query(
                "SELECT id, number, kind, company, person, street, zip_city, country, email, phone, vat_id, note,
                   language, payment_term_id, buyer_reference, einvoice
                 FROM contacts ORDER BY id",
                &[],
            )
            .map_err(pg_err)?
        {
            data.contacts.push(Contact {
                id: r.get::<_, i64>(0) as u64,
                number: r.get(1),
                kind: from_name(r.get(2))?,
                company: r.get(3),
                person: r.get(4),
                street: r.get(5),
                zip_city: r.get(6),
                country: r.get(7),
                email: r.get(8),
                phone: r.get(9),
                vat_id: r.get(10),
                note: r.get(11),
                language: crate::i18n::Lang::from_code(r.get(12)),
                payment_term_id: r.get::<_, Option<i64>>(13).map(|v| v as u64),
                buyer_reference: r.get(14),
                einvoice: r.get::<_, Option<String>>(15).and_then(|s| from_name(s).ok()),
            });
        }
        for r in c
            .query(
                "SELECT id, number, name, description, unit, price_net_cents, vat_percent, service
                 FROM articles ORDER BY id",
                &[],
            )
            .map_err(pg_err)?
        {
            data.articles.push(Article {
                id: r.get::<_, i64>(0) as u64,
                number: r.get(1),
                name: r.get(2),
                description: r.get(3),
                unit: r.get(4),
                price_net: r.get(5),
                vat: r.get::<_, i16>(6) as u8,
                service: r.get(7),
            });
        }
        let mut lines: HashMap<u64, Vec<Line>> = HashMap::new();
        for r in c
            .query(
                "SELECT document_id, article_id, text, qty, unit, price_net_cents, vat_percent, discount_pct
                 FROM document_lines ORDER BY document_id, pos",
                &[],
            )
            .map_err(pg_err)?
        {
            lines.entry(r.get::<_, i64>(0) as u64).or_default().push(Line {
                article_id: r.get::<_, Option<i64>>(1).map(|v| v as u64),
                text: r.get(2),
                qty: r.get(3),
                unit: r.get(4),
                price_net: r.get(5),
                vat: r.get::<_, i16>(6) as u8,
                discount_pct: r.get(7),
            });
        }
        for r in c
            .query(
                "SELECT id, kind, number, contact_id, doc_date, service_date, due_days, title, intro, outro,
                   status, paid_date, source_id, term_name, discounts, paid_discount_pct, emails, reminders
                 FROM documents ORDER BY id",
                &[],
            )
            .map_err(pg_err)?
        {
            let id = r.get::<_, i64>(0) as u64;
            data.documents.push(Document {
                id,
                kind: from_name(r.get(1))?,
                number: r.get(2),
                contact_id: r.get::<_, Option<i64>>(3).map(|v| v as u64),
                date: r.get(4),
                service_date: r.get(5),
                due_days: r.get::<_, i32>(6) as u32,
                title: r.get(7),
                intro: r.get(8),
                outro: r.get(9),
                status: from_name(r.get(10))?,
                paid_date: r.get(11),
                source_id: r.get::<_, Option<i64>>(12).map(|v| v as u64),
                term_name: r.get(13),
                discounts: serde_json::from_str(r.get::<_, &str>(14)).unwrap_or_default(),
                paid_discount_pct: r.get(15),
                emails: serde_json::from_str(r.get::<_, &str>(16)).unwrap_or_default(),
                reminders: serde_json::from_str(r.get::<_, &str>(17)).unwrap_or_default(),
                lines: lines.remove(&id).unwrap_or_default(),
            });
        }
        for r in c
            .query(
                "SELECT id, booking_date, kind, category, text, partner, receipt, gross_cents, vat_percent,
                   document_id, payment, receipt_id
                 FROM bookings ORDER BY booking_date, id",
                &[],
            )
            .map_err(pg_err)?
        {
            data.bookings.push(Booking {
                id: r.get::<_, i64>(0) as u64,
                date: r.get(1),
                kind: from_name(r.get(2))?,
                category: r.get(3),
                text: r.get(4),
                partner: r.get(5),
                receipt: r.get(6),
                gross: r.get(7),
                vat: r.get::<_, i16>(8) as u8,
                doc_id: r.get::<_, Option<i64>>(9).map(|v| v as u64),
                payment: r.get(10),
                receipt_id: r.get::<_, Option<i64>>(11).map(|v| v as u64),
            });
        }
        for r in c.query("SELECT content FROM receipts ORDER BY id", &[]).map_err(pg_err)? {
            if let Ok(receipt) = serde_json::from_str(r.get::<_, &str>(0)) {
                data.receipts.push(receipt);
            }
        }
        let mut log: Vec<LogEntry> = c
            .query(
                "SELECT time_text, user_name, action FROM change_log ORDER BY id DESC LIMIT $1",
                &[&LOG_SHOWN],
            )
            .map_err(pg_err)?
            .into_iter()
            .map(|r| LogEntry { time: r.get(0), user: r.get(1), action: r.get(2) })
            .collect();
        log.reverse();
        data.log = log;

        let mut users = Vec::new();
        for r in c
            .query("SELECT name, role, salt, hash FROM users ORDER BY name", &[])
            .map_err(pg_err)?
        {
            users.push(User {
                name: r.get(0),
                role: from_name(r.get(1))?,
                salt: r.get(2),
                hash: r.get(3),
            });
        }
        let mail = settings.get("mail").and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        let inbox = settings.get("inbox").and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        Ok(Stored { data, users, revision, mail, inbox })
    }
}

impl Store for PgStore {
    fn save_file(&mut self, id: u64, bytes: &[u8]) -> Result<(), String> {
        self.ensure_connected()?;
        self.client
            .execute("INSERT INTO receipt_files (id, content) VALUES ($1, $2)", &[&(id as i64), &bytes])
            .map_err(pg_err)?;
        Ok(())
    }

    fn delete_file(&mut self, id: u64) -> Result<(), String> {
        self.ensure_connected()?;
        self.client
            .execute("DELETE FROM receipt_files WHERE id = $1", &[&(id as i64)])
            .map_err(pg_err)?;
        Ok(())
    }

    fn load_file(&mut self, id: u64) -> Result<Vec<u8>, String> {
        self.ensure_connected()?;
        let row = self
            .client
            .query_opt("SELECT content FROM receipt_files WHERE id = $1", &[&(id as i64)])
            .map_err(pg_err)?
            .ok_or("Die Belegdatei wurde nicht gefunden.")?;
        Ok(row.get(0))
    }

    fn describe(&self) -> String {
        // Show where the database is, without the password.
        let shown = match self.url.split_once('@') {
            Some((_, host)) => host.to_string(),
            None => self.url.clone(),
        };
        format!("PostgreSQL {shown}")
    }

    fn load(&mut self) -> Result<Option<Stored>, String> {
        self.ensure_connected()?;
        let known = self
            .client
            .query_opt("SELECT value FROM settings WHERE key = 'revision'", &[])
            .map_err(pg_err)?;
        if known.is_none() {
            return Ok(None);
        }
        self.read_all().map(Some)
    }

    fn save(&mut self, stored: &Stored, change: Change<'_>) -> Result<(), String> {
        self.ensure_connected()?;
        let mut tx = self.client.transaction().map_err(pg_err)?;
        let empty = Data::default();
        match change {
            Change::Data { old } => Self::write_data(&mut tx, old, &stored.data, stored.revision)?,
            Change::Users => Self::write_users(&mut tx, &stored.users)?,
            Change::Mail => Self::write_mail(&mut tx, &stored.mail)?,
            Change::Inbox => Self::put_setting(&mut tx, "inbox", &serde_json::to_string(&stored.inbox).map_err(|e| e.to_string())?)?,
            Change::All => {
                Self::write_data(&mut tx, &empty, &stored.data, stored.revision)?;
                Self::write_users(&mut tx, &stored.users)?;
                Self::write_mail(&mut tx, &stored.mail)?;
                Self::put_setting(&mut tx, "inbox", &serde_json::to_string(&stored.inbox).map_err(|e| e.to_string())?)?;
            }
        }
        tx.commit().map_err(pg_err)
    }
}
