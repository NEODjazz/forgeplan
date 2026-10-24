#![cfg_attr(
    all(target_os = "windows", feature = "native-client"),
    windows_subsystem = "windows"
)]
use plan_core::{json_string, parse, schedule, schedule_json, serialize, Editor};
mod auth;
#[cfg(feature = "native-client")]
mod native;
#[cfg(feature = "native-client")]
mod native_session;
mod platform;
mod protocol;
mod store;
mod sync;
mod workspace;
use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
const UI: &str = include_str!("web.html");
const JS: &str = include_str!("web.js");
const DEMO: &str = include_str!("../../../examples/infrastructure.fplan");
fn main() {
    if let Err(e) = run() {
        eprintln!("forgeplan: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    #[cfg(feature = "native-client")]
    if args.is_empty() || (args.len() == 1 && args[0] == "app") {
        return native::run();
    }
    match args.first().map(String::as_str) {
        Some("app") => Err(if cfg!(feature = "native-client") {
            "Usage: forgeplan app; set FORGEPLAN_DATA_DIR for a different data location"
        } else {
            "Build with --features native-client and run forgeplan app"
        }
        .into()),
        Some("pull") if args.len() == 5 => sync::pull(&args[1], &args[2], &args[3], &args[4]),
        Some("push") if args.len() == 2 => sync::push(&args[1]),
        Some("--version") | Some("version") if args.len() == 1 => {
            println!(
                "ForgePlan {} ({} {})",
                env!("CARGO_PKG_VERSION"),
                env::consts::OS,
                env::consts::ARCH
            );
            Ok(())
        }
        Some("build-info") if args.len() == 1 => {
            println!("{}", platform::build_info());
            Ok(())
        }
        Some("paths") if args.len() == 1 => platform::print_paths(),
        Some("connect") if args.len() == 2 => platform::connect(&args[1]),
        Some("desktop") => {
            let options = platform::DesktopOptions::parse(&args[1..])?;
            workspace::run_desktop(options.port, &options.database, options.open_browser)
        }
        Some("workspace") if args.len() <= 3 => {
            let port = args
                .get(1)
                .map(|p| p.parse())
                .transpose()
                .map_err(|_| "Invalid port")?
                .unwrap_or(8787);
            workspace::run(
                port,
                args.get(2)
                    .map(String::as_str)
                    .unwrap_or("forgeplan.sqlite"),
            )
        }
        Some("backup") if args.len() == 3 => {
            if !std::path::Path::new(&args[1]).is_file() {
                return Err("Backup source does not exist".into());
            }
            store::Store::open(&args[1])?.backup(&args[2])
        }
        Some("convert") if args.len() == 3 => {
            let p = parse(&fs::read_to_string(&args[1]).map_err(|e| e.to_string())?)?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[2])
                .map_err(|e| e.to_string())?;
            out.write_all(serialize(&p)?.as_bytes())
                .map_err(|e| e.to_string())
        }
        Some("demo") if args.len() == 2 => {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[1])
                .map_err(|e| e.to_string())?;
            f.write_all(serialize(&parse(DEMO)?)?.as_bytes())
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Some("check") if args.len() == 2 => {
            let p = parse(&fs::read_to_string(&args[1]).map_err(|e| e.to_string())?)?;
            let s = schedule(&p)?;
            println!("{}", schedule_json(&p, &s));
            Ok(())
        }
        Some("serve") if args.len() <= 3 => {
            let port: u16 = args
                .get(1)
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| "Invalid port")?
                .unwrap_or(8787);
            let text = if let Some(path) = args.get(2) {
                fs::read_to_string(path).map_err(|e| e.to_string())?
            } else {
                DEMO.to_string()
            };
            parse(&text)?;
            serve(port, &text)
        }
        _ => {
            println!("ForgePlan 0.11.1 alpha — scheduling workspace\n\nforgeplan demo PATH                 Create example (does not overwrite)\nforgeplan check PATH                Validate and output schedule JSON\nforgeplan serve [PORT] [PATH]        Open local web editor (default 8787)\n\nworkspace [PORT] [DB] persists projects and Undo history.\ndesktop [PORT] [DB] [--no-browser] uses the OS data directory when DB is omitted.\npaths prints platform data paths; connect HTTPS_URL opens a corporate workspace; --version prints the build version. backup DB DEST, convert INPUT OUTPUT, pull URL ORG PROJECT FILE, push FILE.");
            Ok(())
        }
    }
}
fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> std::io::Result<()> {
    let connect = auth::connect_origin()
        .map(|origin| format!(" {origin}"))
        .unwrap_or_default();
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'{connect}; img-src 'self' blob:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\n\r\n",body.len())?;
    stream.write_all(body.as_bytes())
}
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}
fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut data = Vec::new();
    let mut byte = [0; 1];
    while !data.ends_with(b"\r\n\r\n") {
        if data.len() >= 8192 {
            return Err("Headers too large".into());
        }
        stream
            .read_exact(&mut byte)
            .map_err(|_| "Incomplete HTTP request")?;
        data.push(byte[0]);
    }
    let header = std::str::from_utf8(&data).map_err(|_| "Invalid headers")?;
    let mut lines = header.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .ok_or("Missing request line")?
        .split_whitespace()
        .collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" {
        return Err("Expected HTTP/1.1".into());
    }
    let mut headers = Vec::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line.split_once(':').ok_or("Malformed header")?;
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    if headers.iter().any(|(k, _)| k == "transfer-encoding") {
        return Err("Transfer-Encoding is not supported".into());
    }
    for key in ["content-length", "host", "origin", "content-type"] {
        if headers.iter().filter(|(k, _)| k == key).count() > 1 {
            return Err(format!("Duplicate {key}"));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .map(|(_, v)| v.parse::<usize>())
        .transpose()
        .map_err(|_| "Invalid Content-Length")?
        .unwrap_or(0);
    if length > 8_000_000 {
        return Err("Body exceeds 8 MB".into());
    }
    let mut body = vec![0; length];
    stream
        .read_exact(&mut body)
        .map_err(|_| "Incomplete body")?;
    Ok(Request {
        method: first[0].into(),
        path: first[1].into(),
        headers,
        body: String::from_utf8(body).map_err(|_| "Body must be UTF-8")?,
    })
}
fn serve(port: u16, initial: &str) -> Result<(), String> {
    let mut editor = Editor::new(parse(initial)?)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let host = format!("127.0.0.1:{port}");
    let origin = format!("http://{host}");
    println!("Open {origin}\nLocal prototype only. Ctrl+C to stop. Download to save changes.");
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Connection error: {e}");
                continue;
            }
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
        let result = match read_request(&mut stream) {
            Err(e) => respond(
                &mut stream,
                "400 Bad Request",
                "application/json; charset=utf-8",
                &format!("{{\"error\":{}}}", json_string(&e)),
            ),
            Ok(req) => {
                let header = |key: &str| {
                    req.headers
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.as_str())
                };
                if header("host") != Some(host.as_str()) {
                    respond(
                        &mut stream,
                        "403 Forbidden",
                        "text/plain; charset=utf-8",
                        "Use the exact 127.0.0.1 URL printed at startup",
                    )
                } else {
                    match (req.method.as_str(), req.path.as_str()) {
                        ("GET", "/") => {
                            respond(&mut stream, "200 OK", "text/html; charset=utf-8", UI)
                        }
                        ("GET", "/api/project") => respond(
                            &mut stream,
                            "200 OK",
                            "text/plain; charset=utf-8",
                            &serialize(editor.project()).expect("validated editor"),
                        ),
                        ("GET", "/app.js") => {
                            respond(&mut stream, "200 OK", "text/javascript; charset=utf-8", JS)
                        }
                        ("GET", "/api/state") => respond(
                            &mut stream,
                            "200 OK",
                            "application/json; charset=utf-8",
                            &editor.json(),
                        ),
                        ("POST", "/api/commands") => {
                            if header("origin") != Some(origin.as_str())
                                || header("content-type")
                                    .map(|v| v.split(';').next().unwrap_or("").trim())
                                    != Some("text/plain")
                            {
                                respond(
                                    &mut stream,
                                    "403 Forbidden",
                                    "application/json",
                                    "{\"error\":\"Same-origin text/plain requests required\"}",
                                )
                            } else {
                                let result = protocol::parse_edit(&req.body).and_then(
                                    |(revision, action)| match action {
                                        protocol::Action::Batch(commands) => {
                                            editor.apply(revision, commands)
                                        }
                                        protocol::Action::Undo => editor.undo(revision),
                                        protocol::Action::Redo => editor.redo(revision),
                                    },
                                );
                                match result {
                                    Ok(()) => respond(
                                        &mut stream,
                                        "200 OK",
                                        "application/json; charset=utf-8",
                                        &editor.json(),
                                    ),
                                    Err(error) => respond(
                                        &mut stream,
                                        if error.starts_with("Revision conflict:") {
                                            "409 Conflict"
                                        } else {
                                            "422 Unprocessable Entity"
                                        },
                                        "application/json; charset=utf-8",
                                        &format!(
                                            "{{\"error\":{},\"current_revision\":{}}}",
                                            json_string(&error),
                                            editor.revision().get()
                                        ),
                                    ),
                                }
                            }
                        }
                        ("POST", "/api/schedule") => {
                            if header("origin") != Some(origin.as_str())
                                || header("content-type")
                                    .map(|v| v.split(';').next().unwrap_or("").trim())
                                    != Some("text/plain")
                            {
                                respond(
                                    &mut stream,
                                    "403 Forbidden",
                                    "application/json",
                                    "{\"error\":\"Same-origin text/plain requests required\"}",
                                )
                            } else {
                                match parse(&req.body)
                                    .and_then(|p| schedule(&p).map(|s| schedule_json(&p, &s)))
                                {
                                    Ok(body) => respond(
                                        &mut stream,
                                        "200 OK",
                                        "application/json; charset=utf-8",
                                        &body,
                                    ),
                                    Err(e) => respond(
                                        &mut stream,
                                        "422 Unprocessable Entity",
                                        "application/json; charset=utf-8",
                                        &format!("{{\"error\":{}}}", json_string(&e)),
                                    ),
                                }
                            }
                        }
                        _ => respond(
                            &mut stream,
                            "404 Not Found",
                            "text/plain; charset=utf-8",
                            "Not found",
                        ),
                    }
                }
            }
        };
        if let Err(e) = result {
            eprintln!("Response error: {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;
    fn request(raw: &[u8]) -> Result<Request, String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let raw = raw.to_vec();
        let sender = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(&raw).unwrap();
            s.shutdown(Shutdown::Write).unwrap();
        });
        let (mut s, _) = listener.accept().unwrap();
        let r = read_request(&mut s);
        sender.join().unwrap();
        r
    }
    #[test]
    fn bounded_http_parser() {
        let r = request(
            b"POST /api/schedule HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 3\r\n\r\nabc",
        )
        .unwrap();
        assert_eq!(r.body, "abc");
        for r in [
            "POST / HTTP/1.1\r\nContent-Length: 1000001\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\na",
        ] {
            assert!(request(r.as_bytes()).is_err());
        }
    }
}
