//! The OpenKontor server as a plain console program, without any window libraries,
//! so it runs on Linux servers (systemd, Docker …).
//!
//!   openkontor-server --db postgres://openkontor:geheim@localhost/openkontor

// Shares the data model with the workplace program; not every helper is used here.
#![allow(dead_code)]

#[path = "../accounting.rs"]
mod accounting;
#[path = "../bank.rs"]
mod bank;
#[path = "../einvoice.rs"]
mod einvoice;
#[path = "../exports.rs"]
mod exports;
#[path = "../filestore.rs"]
mod filestore;
#[path = "../i18n.rs"]
mod i18n;
#[path = "../imap.rs"]
mod imap;
#[path = "../inbox.rs"]
mod inbox;
#[path = "../mail.rs"]
mod mail;
#[path = "../model.rs"]
mod model;
#[path = "../oauth_mail.rs"]
mod oauth_mail;
#[path = "../ocr.rs"]
mod ocr;
#[path = "../pdf.rs"]
mod pdf;
#[path = "../pdf_metrics.rs"]
mod pdf_metrics;
#[path = "../server.rs"]
mod server;
#[path = "../sso.rs"]
mod sso;
#[path = "../store.rs"]
mod store;
#[path = "../tls.rs"]
mod tls;
#[path = "../totp.rs"]
mod totp;
#[path = "../webapp.rs"]
mod webapp;
#[path = "../xrechnung.rs"]
mod xrechnung;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    server::run_cli(&args);
}
