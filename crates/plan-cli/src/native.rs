//! Native window around the shared editor, with distinct local/remote sessions.
use crate::{
    native_session::{LocalSession, Preferences},
    platform,
};
use serde::Deserialize;
use std::path::Path;
use tao::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    keyboard::{KeyCode, ModifiersState},
    window::{Window, WindowBuilder},
};
use wry::{NewWindowResponse, PageLoadEvent, WebContext, WebView, WebViewBuilder};
#[cfg(target_os = "linux")]
use {tao::platform::unix::WindowExtUnix, wry::WebViewBuilderExtUnix};
const HOME: &str = include_str!("native_home.html");
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Choice {
    Local {},
    Remote { url: String },
    Home {},
}
#[derive(Debug)]
enum Message {
    Choice(Choice),
    HomeReady,
}

fn editor_choice(body: &str) -> Option<Choice> {
    if body.len() > 256 {
        return None;
    }
    match serde_json::from_str::<Choice>(body) {
        Ok(Choice::Home {}) => Some(Choice::Home {}),
        _ => None,
    }
}
fn navigation_allowed(destination: &str, origin: &str, local: bool) -> bool {
    let Ok(url) = reqwest::Url::parse(destination) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if local {
        url.origin().ascii_serialization() == origin
    } else {
        url.scheme() == "https"
            || (url.scheme() == "http" && url.origin().ascii_serialization() == origin)
    }
}

fn build(window: &Window, builder: WebViewBuilder<'_>) -> Result<WebView, String> {
    #[cfg(target_os = "linux")]
    let result = builder.build_gtk(
        window
            .default_vbox()
            .ok_or("Native window has no GTK container")?,
    );
    #[cfg(not(target_os = "linux"))]
    let result = builder.build(window);
    result.map_err(|e| e.to_string())
}
fn home(
    window: &Window,
    proxy: EventLoopProxy<Message>,
    preferences: &Preferences,
    error: Option<&str>,
) -> Result<WebView, String> {
    let config = serde_json::json!({"server":preferences.server,"error":error});
    let ready = proxy.clone();
    build(
        window,
        WebViewBuilder::new()
            .with_incognito(true)
            .with_html(HOME)
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, PageLoadEvent::Finished) {
                    let _ = ready.send_event(Message::HomeReady);
                }
            })
            .with_initialization_script(format!("window.forgeplanSettings={config};"))
            .with_navigation_handler(|url| url == "about:blank")
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if request.body().len() > 8192 {
                    return;
                }
                if let Ok(choice) = serde_json::from_str::<Choice>(request.body()) {
                    let _ = proxy.send_event(Message::Choice(choice));
                }
            }),
    )
}
fn editor(
    window: &Window,
    proxy: EventLoopProxy<Message>,
    context: &mut WebContext,
    url: &str,
    token: Option<&str>,
) -> Result<WebView, String> {
    let origin = reqwest::Url::parse(url)
        .map_err(|e| e.to_string())?
        .origin()
        .ascii_serialization();
    let local = token.is_some();
    let initialization = if let Some(token) = token {
        format!("if(location.origin==={}){{sessionStorage.setItem('fp-token',{});sessionStorage.setItem('fp-org','local');}}",serde_json::to_string(&origin).unwrap(),serde_json::to_string(token).unwrap())
    } else {
        String::new()
    };
    let local_ui = if local {
        r#"window.addEventListener('DOMContentLoaded',()=>{for(const id of ['organization','api-token']){const e=document.getElementById(id);if(e)e.closest('label').style.display='none';}for(const id of ['connect-token','oidc-login','oidc-logout']){const e=document.getElementById(id);if(e)e.hidden=true;}const refresh=document.getElementById('refresh');if(refresh)refresh.textContent='Обновить проект';});"#
    } else {
        ""
    };
    let toolbar = r#"window.addEventListener('DOMContentLoaded',()=>{const b=document.createElement('button');b.id='native-home';b.textContent='← Режим работы';b.title='Вернуться к выбору режима';b.style.cssText='position:fixed;right:20px;bottom:18px;z-index:2147483647;padding:10px 16px;background:#17283f;color:white;border:1px solid #9caec5;border-radius:8px;cursor:pointer';b.addEventListener('click',()=>window.ipc.postMessage('{"action":"home"}'));document.body.appendChild(b);document.addEventListener('keydown',e=>{const apply=document.getElementById('apply');if(apply&&(e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){e.preventDefault();if(!apply.disabled)apply.click();}});});"#;
    let proxy_home = proxy.clone();
    build(
        window,
        WebViewBuilder::new_with_web_context(context)
            .with_incognito(local)
            .with_url(url)
            .with_initialization_script(initialization + local_ui + toolbar)
            .with_navigation_handler(move |destination| {
                navigation_allowed(&destination, &origin, local)
            })
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            // Remote content has exactly one action: return home. It cannot start a
            // local session, select filesystem paths or call native project APIs.
            .with_ipc_handler(move |request| {
                if let Some(choice) = editor_choice(request.body()) {
                    let _ = proxy_home.send_event(Message::Choice(choice));
                }
            }),
    )
}
fn prepare_dir(path: &Path) -> Result<(), String> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| e.to_string())
}
pub fn run() -> Result<(), String> {
    let directory = platform::data_dir()?;
    prepare_dir(&directory)?;
    let mut preferences = Preferences::read(&directory).unwrap_or_default();
    let database = directory.join("forgeplan.sqlite");
    let event_loop = EventLoopBuilder::<Message>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("ForgePlan — загрузка")
        .with_inner_size(LogicalSize::new(1440., 1000.))
        .with_min_inner_size(LogicalSize::new(960., 700.))
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    let mut local_context = WebContext::new(Some(directory.join("local-webview")));
    let mut remote_context = WebContext::new(Some(directory.join("remote-webview")));
    let mut webview = Some(home(&window, proxy.clone(), &preferences, None)?);
    let mut session: Option<LocalSession> = None;
    let mut at_home = true;
    let mut modifiers = ModifiersState::empty();
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested | WindowEvent::Destroyed,
                ..
            } => {
                webview.take();
                session.take();
                *control = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                physical_key: KeyCode::F6,
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                let _ = proxy.send_event(Message::Choice(Choice::Home {}));
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(value),
                ..
            } => modifiers = value,
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                physical_key: KeyCode::KeyQ,
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    },
                ..
            } if modifiers.control_key() || modifiers.super_key() => {
                webview.take();
                session.take();
                *control = ControlFlow::Exit;
            }
            Event::UserEvent(Message::HomeReady) => {
                if at_home {
                    window.set_title("ForgePlan — выбор режима");
                }
            }
            Event::UserEvent(Message::Choice(choice)) => {
                if !at_home && !matches!(choice, Choice::Home {}) {
                    return;
                }
                let result = (|| -> Result<(), String> {
                    match choice {
                        Choice::Home {} => {
                            webview.take();
                            session.take();
                            at_home = true;
                            window.set_title("ForgePlan — загрузка");
                            webview = Some(home(&window, proxy.clone(), &preferences, None)?);
                        }
                        Choice::Local {} => {
                            let local = LocalSession::start(&database)?;
                            eprintln!("Native local session ready: {}", local.url);
                            webview.take();
                            let view = editor(
                                &window,
                                proxy.clone(),
                                &mut local_context,
                                &local.url,
                                Some(&local.token),
                            )?;
                            webview = Some(view);
                            session = Some(local);
                            at_home = false;
                            window.set_title("ForgePlan — локальные проекты • автономно");
                        }
                        Choice::Remote { url } => {
                            let url = platform::server_url(&url)?;
                            let updated = Preferences {
                                server: url.clone(),
                            };
                            updated.write(&directory)?;
                            preferences = updated;
                            webview.take();
                            session.take();
                            webview = Some(editor(
                                &window,
                                proxy.clone(),
                                &mut remote_context,
                                &url,
                                None,
                            )?);
                            at_home = false;
                            window.set_title(&format!("ForgePlan — корпоративный сервер • {url}"));
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    webview.take();
                    session.take();
                    at_home = true;
                    window.set_title("ForgePlan — загрузка");
                    match home(&window, proxy.clone(), &preferences, Some(&error)) {
                        Ok(view) => webview = Some(view),
                        Err(error) => {
                            eprintln!("Native window error: {error}");
                            *control = ControlFlow::Exit;
                        }
                    }
                }
            }
            _ => {}
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_ipc_exposes_only_home_and_enforces_message_size() {
        assert!(matches!(
            editor_choice(r#"{"action":"home"}"#),
            Some(Choice::Home {})
        ));
        for body in [
            r#"{"action":"local"}"#,
            r#"{"action":"remote","url":"https://one.example/"}"#,
            r#"{"action":"home","path":"/tmp/secret"}"#,
            "not-json",
        ] {
            assert!(editor_choice(body).is_none());
        }
        assert!(editor_choice(&(" ".repeat(257) + r#"{"action":"home"}"#)).is_none());
    }
    #[test]
    fn navigation_keeps_local_origin_and_limits_remote_redirect_schemes() {
        let origin = "http://127.0.0.1:12345";
        assert!(navigation_allowed(
            "http://127.0.0.1:12345/?code=x",
            origin,
            true
        ));
        for url in [
            "http://127.0.0.1:54321/",
            "https://idp.example/",
            "file:///tmp/a",
            "javascript:alert(1)",
        ] {
            assert!(!navigation_allowed(url, origin, true));
        }
        assert!(navigation_allowed(
            "https://idp.example/authorize",
            "https://projects.example",
            false
        ));
        assert!(navigation_allowed("http://127.0.0.1:12345/", origin, false));
        for url in [
            "http://other.example/",
            "http://127.0.0.1:54321/",
            "https://user:secret@idp.example/",
            "file:///tmp/a",
        ] {
            assert!(!navigation_allowed(url, origin, false));
        }
    }
    #[test]
    fn launcher_commands_have_no_filesystem_or_secret_parameters() {
        assert!(matches!(
            serde_json::from_str::<Choice>(r#"{"action":"local"}"#),
            Ok(Choice::Local {})
        ));
        for input in [
            r#"{"action":"local","path":"/tmp/a"}"#,
            r#"{"action":"execute","command":"sh"}"#,
            r#"{"action":"remote","url":"https://a","token":"secret"}"#,
        ] {
            assert!(serde_json::from_str::<Choice>(input).is_err());
        }
    }
}
