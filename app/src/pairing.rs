//! Pairing without typing: the panel shows a QR code that opens Home
//! Assistant's normal login page on a phone. After the login Home Assistant
//! redirects the phone to the panel's small web server, which exchanges the
//! one-time code for tokens (IndieAuth: client_id and redirect_uri on the
//! panel's own address, so Home Assistant accepts them without extra setup).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;

use crate::auth::Auth;

pub struct Pairing {
    pub authorize_url: String,
    pub panel_url: String,
}

/// The panel's own address on the route to Home Assistant.
pub fn local_ip_towards(host_port: &str) -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(host_port).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut buf);
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| url_decode(v))
    })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
         <title>NSPanel Pro</title></head><body style=\"font-family:sans-serif;padding:2em\">{body}</body></html>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
}

/// Starts the web server and returns the login URL for the QR code.
/// `on_paired` runs (on the server thread) once the login is stored.
pub fn start(
    ha_url: &str,
    ha_host_port: &str,
    port: u16,
    auth: Arc<Auth>,
    on_paired: impl Fn() + Send + 'static,
) -> Result<Pairing, String> {
    let ip = local_ip_towards(ha_host_port).ok_or("cannot determine the panel's IP address")?;
    let panel_url = format!("http://{ip}:{port}");
    let client_id = format!("{panel_url}/");
    let redirect_uri = format!("{panel_url}/auth/callback");
    let state = random_hex(16);
    let authorize_url = format!(
        "{ha_url}/auth/authorize?response_type=code&client_id={}&redirect_uri={}&state={}",
        url_encode(&client_id),
        url_encode(&redirect_uri),
        state
    );

    let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| format!("port {port}: {e}"))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let mut line = String::new();
            if BufReader::new((&stream).take(8192)).read_line(&mut line).is_err() {
                continue;
            }
            let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (path, query) = target.split_once('?').unwrap_or((&target, ""));
            match path {
                "/" => respond(
                    &mut stream,
                    "200 OK",
                    &format!("<link rel=\"redirect_uri\" href=\"{redirect_uri}\"><h1>NSPanel Pro</h1>"),
                ),
                "/auth/callback" => {
                    let code = query_param(query, "code");
                    let got_state = query_param(query, "state");
                    match (code, got_state) {
                        (Some(code), Some(s)) if s == state => match auth.exchange_code(&client_id, &code) {
                            Ok(()) => {
                                respond(&mut stream, "200 OK", "<h1>Panel connected</h1><p>You can close this page.</p>");
                                on_paired();
                                // Paired: the login web server is no longer needed.
                                return;
                            }
                            Err(e) => {
                                eprintln!("nspanel-app: pairing failed: {e}");
                                respond(&mut stream, "500 Internal Server Error", "<h1>Login failed</h1><p>Scan the code again.</p>");
                            }
                        },
                        _ => respond(&mut stream, "400 Bad Request", "<h1>Invalid login link</h1><p>Scan the code on the panel again.</p>"),
                    }
                }
                _ => respond(&mut stream, "404 Not Found", "<h1>Not found</h1>"),
            }
        }
    });
    Ok(Pairing { authorize_url, panel_url })
}

/// QR code as a black-on-white image, one module = `scale` pixels, with a quiet zone.
pub fn qr_image(text: &str, target_px: u32) -> Option<slint::Image> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let modules = code.width() as u32;
    let quiet = 2;
    let total = modules + 2 * quiet;
    let scale = (target_px / total).max(1);
    let size = total * scale;
    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(size, size);
    let pixels = buf.make_mut_slice();
    for p in pixels.iter_mut() {
        *p = slint::Rgb8Pixel { r: 255, g: 255, b: 255 };
    }
    for y in 0..modules {
        for x in 0..modules {
            if code[(x as usize, y as usize)] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + quiet) * scale + dx;
                        let py = (y + quiet) * scale + dy;
                        pixels[(py * size + px) as usize] = slint::Rgb8Pixel { r: 0, g: 0, b: 0 };
                    }
                }
            }
        }
    }
    Some(slint::Image::from_rgb8(buf))
}
