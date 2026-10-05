//! The console's HTTP transport.
//!
//! GPUI deliberately ships no HTTP client: `App::http_client()` returns a
//! `NullHttpClient` whose every request fails with "No HttpClient available". Zed
//! installs a `reqwest` client at startup for the same reason — an application has to
//! supply its own. This module is that supply, and [`install`] is the only place that
//! decides which client the application runs with.
//!
//! Two decisions worth stating, both forced by the environment:
//!
//! * **`reqwest` is built with rustls and the `ring` provider.** The binary then needs
//!   no OpenSSL and no cmake, which matters on a machine that has neither the OpenSSL nor
//!   the cmake *development* packages.
//! * **Requests use `reqwest`'s blocking client on a thread of their own.** GPUI's
//!   executor is not a reactor: `reqwest`'s async client panics inside it with "there is
//!   no reactor running, must be called from the context of a Tokio 1.x runtime". Rather
//!   than bolt a tokio runtime onto an application that has none, the blocking client
//!   owns its own runtime and the work happens off the UI thread. The console makes a
//!   handful of requests — discovery, a token exchange, `/me`, a party list — so a
//!   thread per in-flight request is not a cost worth optimising away.

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use gpui_kit::App;
use gpui_kit::http_client::{AsyncBody, HttpClient, RedirectPolicy, Request, Response, Url, http};

/// The longest a single platform request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest response the console will buffer.
const MAX_RESPONSE_BYTES: u64 = 8 << 20;

/// The `User-Agent` the console identifies with.
fn user_agent() -> String {
    format!("PartyTime/{} (desktop)", env!("CARGO_PKG_VERSION"))
}

/// Installs the console's HTTP client.
///
/// Call once, after `gpui_kit::init`, before the first window: everything after this
/// point — the OAuth flow, the PartyTime API, asset fetches — goes through it.
///
/// # Panics
///
/// If a client cannot be built. Without one the application cannot sign in, and failing
/// quietly here would surface much later as an unexplained refusal.
pub fn install(cx: &mut App) {
    cx.set_http_client(transport());
}

/// The transport, for anything that needs to make a request outside the application —
/// a command-line tool, or a development probe. [`install`] is what the app itself uses.
#[must_use]
pub fn transport() -> Arc<dyn HttpClient> {
    Arc::new(HttpTransport::new())
}

/// A [`HttpClient`] backed by `reqwest`.
///
/// Two clients, because `reqwest` takes its redirect policy at the *client* level and
/// the console needs both behaviours: discovery may follow redirects, and anything
/// carrying a grant or a bearer token must not — a redirect would hand it to whatever
/// host the redirect names.
struct HttpTransport {
    following: reqwest::blocking::Client,
    strict: reqwest::blocking::Client,
    agent: http::header::HeaderValue,
}

impl HttpTransport {
    /// Builds both clients with the console's timeout and user agent.
    fn new() -> Self {
        let agent = user_agent();
        let header: http::header::HeaderValue = agent
            .parse()
            .expect("the user agent is a valid header value");
        let build = |redirect| {
            reqwest::blocking::Client::builder()
                .user_agent(agent.clone())
                .timeout(REQUEST_TIMEOUT)
                .redirect(redirect)
                .build()
                .expect("building the HTTP client")
        };
        Self {
            following: build(reqwest::redirect::Policy::limited(5)),
            strict: build(reqwest::redirect::Policy::none()),
            agent: header,
        }
    }

    /// Whether the caller asked for redirects not to be followed.
    fn follows_redirects(request: &Request<AsyncBody>) -> bool {
        !matches!(
            request.extensions().get::<RedirectPolicy>(),
            Some(RedirectPolicy::NoFollow)
        )
    }
}

impl HttpClient for HttpTransport {
    fn user_agent(&self) -> Option<&http::header::HeaderValue> {
        Some(&self.agent)
    }

    fn proxy(&self) -> Option<&Url> {
        // `reqwest` reads the standard proxy environment variables itself.
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let client = if Self::follows_redirects(&request) {
            self.following.clone()
        } else {
            self.strict.clone()
        };
        let (tx, rx) = futures::channel::oneshot::channel();

        // Blocking work, on its own thread; the UI thread is handed a future that
        // resolves when the answer arrives.
        std::thread::spawn(move || {
            let _ = tx.send(exchange(&client, request));
        });

        Box::pin(async move {
            rx.await
                .map_err(|_| anyhow::anyhow!("the HTTP worker stopped before answering"))?
        })
    }
}

/// One blocking request, from a `http` request to a `http` response.
fn exchange(
    client: &reqwest::blocking::Client,
    request: Request<AsyncBody>,
) -> anyhow::Result<Response<AsyncBody>> {
    use futures::AsyncReadExt as _;

    let (parts, body) = request.into_parts();
    // The request body is already in memory; reading it cannot wait on anything.
    let mut bytes = Vec::new();
    futures::executor::block_on(
        futures::AsyncReadExt::take(body, MAX_RESPONSE_BYTES).read_to_end(&mut bytes),
    )?;

    let mut builder = client.request(parts.method, parts.uri.to_string());
    for (name, value) in parts.headers.iter() {
        builder = builder.header(name.as_str(), value.as_bytes());
    }

    let response = builder.body(bytes).send()?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.bytes()?;

    let mut out = Response::builder().status(gpui_kit::http_client::StatusCode::from_u16(
        status.as_u16(),
    )?);
    for (name, value) in headers.iter() {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            http::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            out = out.header(name, value);
        }
    }
    Ok(out.body(AsyncBody::from(bytes.to_vec()))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::AsyncReadExt as _;

    fn post(body: &str) -> Request<AsyncBody> {
        Request::builder()
            .method("POST")
            .uri("http://127.0.0.1:9/never")
            .header("Content-Type", "application/json")
            .extension(RedirectPolicy::NoFollow)
            .body(AsyncBody::from(body.to_string()))
            .expect("a valid request")
    }

    fn get() -> Request<AsyncBody> {
        Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:9/x")
            .body(AsyncBody::empty())
            .expect("a valid request")
    }

    #[test]
    fn a_request_asking_for_no_redirects_picks_the_strict_client() {
        assert!(!HttpTransport::follows_redirects(&post("{}")));
        assert!(HttpTransport::follows_redirects(&get()));
    }

    #[test]
    fn the_user_agent_names_the_application_and_its_version() {
        let agent = user_agent();
        assert!(agent.starts_with("PartyTime/"), "{agent}");
        assert!(agent.contains(env!("CARGO_PKG_VERSION")), "{agent}");
    }

    #[test]
    fn a_transport_advertises_its_user_agent() {
        assert!(HttpTransport::new().user_agent().is_some());
    }

    #[test]
    fn a_transport_reports_a_real_network_failure_rather_than_the_placeholder() {
        // The console used to run on GPUI's NullHttpClient, which fails every request
        // with "No HttpClient available". A real transport must produce its own error,
        // and must not panic about a missing reactor.
        let transport = HttpTransport::new();
        let message = match futures::executor::block_on(transport.send(post("{}"))) {
            Ok(_) => panic!("a closed port must not answer"),
            Err(error) => error.to_string(),
        };
        assert!(!message.contains("No HttpClient available"), "{message}");
        assert!(!message.contains("reactor"), "{message}");
    }

    #[test]
    fn a_transport_answers_a_request_it_can_reach() {
        // Served by a throwaway listener in this process, so this exercises the whole
        // path: request out, status and headers back.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binding");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::{Read, Write};
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let body = br#"{"ok":true}"#;
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Probe: yes\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.write_all(body);
            }
        });

        let request = Request::builder()
            .method("GET")
            .uri(format!("http://127.0.0.1:{port}/thing"))
            .body(AsyncBody::empty())
            .expect("a valid request");

        let transport = HttpTransport::new();
        let response = futures::executor::block_on(transport.send(request)).expect("a response");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response.headers().get("x-probe").map(|v| v.as_bytes()),
            Some(&b"yes"[..])
        );

        let mut body = Vec::new();
        futures::executor::block_on(
            futures::AsyncReadExt::take(response.into_body(), MAX_RESPONSE_BYTES)
                .read_to_end(&mut body),
        )
        .expect("reading the body");
        assert_eq!(body, br#"{"ok":true}"#);
    }
}
