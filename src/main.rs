//! OpenKontor – free invoicing and bookkeeping for small businesses,
//! as a self-hosted server with desktop workplaces.
//!
//! Start without options for the workplace window.
//!   openkontor --server [--port 8470] [--data DIR] [--name NAME]   headless server
//!   openkontor --screenshot bild.png [--section belege]            save a picture and exit

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounting;
mod app;
mod backend;
mod bank;
mod einvoice;
mod export;
mod exports;
mod filestore;
mod i18n;
mod icons;
mod import;
mod imap;
mod inbox;
mod mail;
mod model;
mod oauth_mail;
mod ocr;
mod pdf;
mod pdf_metrics;
mod server;
mod sso;
mod sso_client;
mod store;
mod tls;
mod totp;
mod ui_kit;
mod webapp;
mod xrechnung;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--server") {
        server::run_cli(&args);
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("OpenKontor – Rechnungen & Buchhaltung\n");
        println!("  openkontor                                  Arbeitsplatz öffnen");
        println!("  openkontor --server [--db URL] [--port 8470] [--data ORDNER] [--name NAME]");
        println!("                                              Server ohne Fenster starten");
        println!("  openkontor --screenshot bild.png [--section NAME] [--lang de|en]");
        return Ok(());
    }

    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let shot = value("--screenshot").map(|p| app::Shot {
        path: p.into(),
        requested: false,
        done: Arc::new(AtomicBool::new(false)),
        error: Arc::new(Mutex::new(None)),
    });
    let section = value("--section").and_then(|s| app::Section::from_name(&s));
    let lang = value("--lang").map(|s| i18n::Lang::from_code(&s));

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OpenKontor – Rechnungen & Buchhaltung")
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([1040.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenKontor",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, shot, section, lang)))),
    )
}
