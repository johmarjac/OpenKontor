//! HTTPS directly in the server: a small TLS front end (rustls) that decrypts the
//! connections and passes them on to the HTTP server inside the same program.

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Loads certificate (chain) and private key from PEM files, e.g. from Let's Encrypt.
pub fn load_config(cert_file: &str, key_file: &str) -> Result<Arc<rustls::ServerConfig>, String> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_file)
        .map_err(|e| format!("Zertifikat {cert_file} nicht lesbar: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Zertifikat {cert_file} nicht lesbar: {e}"))?;
    if certs.is_empty() {
        return Err(format!("In {cert_file} wurde kein Zertifikat gefunden."));
    }
    let key = PrivateKeyDer::from_pem_file(key_file).map_err(|e| format!("Schlüssel {key_file} nicht lesbar: {e}"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("Zertifikat und Schlüssel passen nicht zusammen: {e}"))?;
    Ok(Arc::new(config))
}

/// Listens for HTTPS on `bind:port` and forwards every connection to `127.0.0.1:backend`.
pub fn start(bind: &str, port: u16, config: Arc<rustls::ServerConfig>, backend: u16) -> Result<(), String> {
    let listener =
        TcpListener::bind((bind, port)).map_err(|e| format!("Port {port} kann nicht geöffnet werden: {e}"))?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let config = config.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, config, backend);
            });
        }
    });
    Ok(())
}

fn serve(tcp: TcpStream, config: Arc<rustls::ServerConfig>, backend: u16) -> std::io::Result<()> {
    let conn = rustls::ServerConnection::new(config).map_err(std::io::Error::other)?;
    tcp.set_read_timeout(Some(Duration::from_millis(50)))?;
    let raw = tcp.try_clone()?;
    let tls = Arc::new(Mutex::new(rustls::StreamOwned::new(conn, tcp)));
    let back = TcpStream::connect(("127.0.0.1", backend))?;
    let mut back_read = back.try_clone()?;
    let done = Arc::new(AtomicBool::new(false));

    // Server → browser/workplace.
    let (tls2, done2) = (tls.clone(), done.clone());
    let writer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 32 * 1024];
        while let Ok(n) = back_read.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut s = tls2.lock().unwrap();
            if s.write_all(&buf[..n]).and_then(|_| s.flush()).is_err() {
                break;
            }
        }
        done2.store(true, Ordering::SeqCst);
        if let Ok(mut s) = tls2.lock() {
            s.conn.send_close_notify();
            let _ = s.flush();
        }
    });

    // Browser/workplace → server, in short slices so the writer can use the connection too.
    let mut back_write = back;
    let mut buf = vec![0u8; 32 * 1024];
    let mut idle = 0u32;
    while !done.load(Ordering::SeqCst) {
        let r = { tls.lock().unwrap().read(&mut buf) };
        match r {
            Ok(0) => break,
            Ok(n) => {
                idle = 0;
                if back_write.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                // Close connections that stay silent for ten minutes.
                idle += 1;
                if idle > 12_000 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = back_write.shutdown(Shutdown::Write);
    let _ = writer.join();
    let _ = raw.shutdown(Shutdown::Both);
    Ok(())
}
