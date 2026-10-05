//! The loopback listener the OAuth redirect points at.
//!
//! The platform's registered redirect for this client is a loopback address, so the
//! authorization code comes back to this process over HTTP. The listener is deliberately
//! minimal: bind, take exactly one request, answer it, stop. There is no route table, no
//! session, and nothing left listening afterwards.
//!
//! A custom URL scheme would avoid the listener, but it needs per-platform registration
//! (a desktop entry on Linux, `CFBundleURLTypes` on macOS) and the platform has no such
//! redirect registered. Loopback is what is registered and what was verified.

use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use crate::oauth::{CallbackError, parse_callback};

/// A listener bound to loopback, waiting for the authorization code.
pub struct CallbackListener {
    server: tiny_http::Server,
    address: SocketAddr,
}

/// What the listener is bound to, and what the platform must have registered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
    /// The host:port could not be taken.
    #[error("could not listen on {address}: {reason}")]
    Unavailable {
        /// What was tried.
        address: String,
        /// Why it failed.
        reason: String,
    },
    /// The redirect URI does not name a loopback address this listener could serve.
    #[error("{0} is not a loopback address")]
    NotLoopback(String),
    /// The redirect URI has no port to listen on.
    #[error("{0} has no port")]
    NoPort(String),
}

impl CallbackListener {
    /// Binds the listener to the address named by `redirect_uri`.
    ///
    /// Only loopback is accepted. A redirect to a routable address would be an authorization
    /// server sending the code somewhere the console does not control, and a listener that
    /// accepted one would expose the flow on the network.
    pub fn bind(redirect_uri: &str) -> Result<Self, BindError> {
        let parsed = url::Url::parse(redirect_uri).map_err(|error| BindError::Unavailable {
            address: redirect_uri.to_string(),
            reason: error.to_string(),
        })?;

        // Loopback is checked before the port, so a routable address is refused as
        // routable rather than as a missing port.
        let host = parsed
            .host_str()
            .ok_or_else(|| BindError::NoPort(redirect_uri.to_string()))?;
        if !(host == "127.0.0.1" || host == "localhost" || host == "::1") {
            return Err(BindError::NotLoopback(host.to_string()));
        }
        let port = parsed
            .port()
            .ok_or_else(|| BindError::NoPort(redirect_uri.to_string()))?;

        let address: SocketAddr = (host, port)
            .to_socket_addrs()
            .map_err(|error| BindError::Unavailable {
                address: format!("{host}:{port}"),
                reason: error.to_string(),
            })?
            .next()
            .ok_or_else(|| BindError::Unavailable {
                address: format!("{host}:{port}"),
                reason: "no address resolved".into(),
            })?;

        let server = tiny_http::Server::http(address).map_err(|error| BindError::Unavailable {
            address: address.to_string(),
            reason: error.to_string(),
        })?;

        Ok(Self { server, address })
    }

    /// The address actually bound, which is what the callback will arrive on.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Waits for the callback and returns what it carried.
    ///
    /// Answers the browser either way, so the user sees a page rather than a hung tab —
    /// including when the callback is refused, where the page says why.
    pub fn wait(&self, expected_state: &str, timeout: Duration) -> Result<String, CallbackError> {
        match self.server.recv_timeout(timeout) {
            Ok(Some(request)) => self.answer(request, expected_state),
            // A timeout is a user who closed the tab, not a failure to explain.
            Ok(None) | Err(_) => Err(CallbackError::Cancelled),
        }
    }

    fn answer(
        &self,
        request: tiny_http::Request,
        expected_state: &str,
    ) -> Result<String, CallbackError> {
        let query = request.url().split_once('?').map_or("", |(_, query)| query);
        let outcome = parse_callback(query, expected_state);
        let (status, title, detail) = match &outcome {
            Ok(_) => (
                200,
                "Signed in",
                "You can close this tab and return to PartyTime.".to_string(),
            ),
            Err(error) => (400, "Sign-in did not finish", error.to_string()),
        };
        let body = page(title, &detail);
        let _ = request.respond(
            tiny_http::Response::from_string(body)
                .with_status_code(status)
                .with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    )
                    .expect("a valid header"),
                ),
        );
        outcome
    }
}

/// The page the browser lands on after the callback.
///
/// Says what happened rather than showing a blank tab, and carries no script, no remote
/// content and no reference to a token.
fn page(title: &str, detail: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>{title}</title></head>\
         <body style=\"font-family:system-ui,sans-serif;margin:4rem auto;max-width:32rem\">\
         <h1 style=\"font-size:1.25rem\">{title}</h1>\
         <p style=\"color:#555\">{detail}</p></body></html>"
    )
}

/// The address a redirect URI names, for a caller that only wants to know where to listen.
#[must_use]
pub fn loopback_address(redirect_uri: &str) -> Option<SocketAddr> {
    let parsed = url::Url::parse(redirect_uri).ok()?;
    let host = parsed.host_str()?;
    let port = parsed.port()?;
    if !(host == "127.0.0.1" || host == "localhost" || host == "::1") {
        return None;
    }
    (host, port).to_socket_addrs().ok()?.next()
}

/// The address used when a redirect names the wildcard loopback.
pub const WILDCARD: Ipv4Addr = Ipv4Addr::LOCALHOST;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registered_redirect_resolves_to_a_loopback_address() {
        let address = loopback_address("http://127.0.0.1:1420/oauth/callback")
            .expect("the dev redirect is a loopback address");
        assert_eq!(address.port(), 1420);
        assert!(address.ip().is_loopback());
    }

    #[test]
    fn a_non_loopback_redirect_is_refused() {
        assert!(loopback_address("https://openparty.example/oauth/callback").is_none());
        assert!(matches!(
            CallbackListener::bind("https://openparty.example/oauth/callback"),
            Err(BindError::NotLoopback(_))
        ));
    }

    #[test]
    fn a_redirect_without_a_port_cannot_be_served() {
        assert!(loopback_address("http://127.0.0.1/oauth/callback").is_none());
        assert!(matches!(
            CallbackListener::bind("http://127.0.0.1/oauth/callback"),
            Err(BindError::NoPort(_))
        ));
    }

    #[test]
    fn binding_says_so_when_the_port_is_taken() {
        let first = CallbackListener::bind("http://127.0.0.1:1420/oauth/callback");
        // Skip when the fixed dev port is not available in this environment.
        let Ok(first) = first else {
            return;
        };
        let second = CallbackListener::bind("http://127.0.0.1:1420/oauth/callback");
        assert!(
            matches!(second, Err(BindError::Unavailable { .. })),
            "a second listener on the same port must not silently succeed"
        );
        drop(first);
    }

    #[test]
    fn waiting_gives_up_rather_than_listening_forever() {
        let Ok(listener) = CallbackListener::bind("http://127.0.0.1:1420/oauth/callback") else {
            return;
        };
        assert_eq!(
            listener.wait("s1", Duration::from_millis(50)),
            Err(CallbackError::Cancelled),
            "an abandoned sign-in must end, not hang"
        );
    }

    #[test]
    fn the_landing_page_names_what_happened_without_referencing_a_token() {
        let page = page("Signed in", "Return to PartyTime.");
        assert!(page.contains("Signed in"));
        assert!(page.contains("Return to PartyTime."));
        assert!(!page.contains("pt_"), "no token may appear on the page");
        assert!(!page.contains("<script"), "the page carries no script");
    }
}
