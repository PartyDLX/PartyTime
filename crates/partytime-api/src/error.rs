//! What can go wrong, and what the console says about it.
//!
//! The platform's own sentence is kept verbatim as [`PartyError::server_text`] and shown in
//! a technical-detail disclosure; [`PartyError`]'s own message is the copy a creator acts
//! on. Neither is written at the call site, and a refusal is never retried in a loop — a
//! permission or consent problem needs a person, another attempt, or a different party.

/// A failure talking to OpenParty.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PartyError {
    /// The access token is missing, expired or revoked, and refreshing did not fix it.
    #[error("Sign in again to continue.")]
    Unauthenticated,

    /// The token does not carry the scope this endpoint needs.
    #[error("This app is not allowed to do that. Reconnect the app to grant the permission.")]
    InsufficientScope,

    /// The user declined consent on the consent screen.
    #[error("You chose not to let PartyTime sign in.")]
    SignInDenied,

    /// The browser closed before the sign-in finished.
    #[error("Sign-in was cancelled.")]
    SignInCancelled,

    /// The sign-in itself was refused, or a token could not be kept.
    #[error("{0}")]
    SignInRefused(String),

    /// The caller is not on the roster.
    #[error("You're not on this party's roster.")]
    NotAMember {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The caller has been removed from the party.
    #[error("You've been removed from this party.")]
    Banned {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The owner has not started the party.
    #[error("The owner hasn't started this party yet.")]
    NoLiveSession {
        /// The platform's own sentence.
        server_text: String,
    },

    /// Only the owner or the director may do this.
    #[error("Only the owner or director can do that.")]
    NotPermitted {
        /// The platform's own sentence.
        server_text: String,
    },

    /// No worker is bound to the session.
    #[error("No media worker is assigned to this session.")]
    NoWorker {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The worker exists but has not published its endpoint.
    #[error("The worker for this session hasn't published its endpoint yet.")]
    WorkerEndpointMissing {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The party does not exist.
    #[error("That party doesn't exist.")]
    PartyNotFound {
        /// The platform's own sentence.
        server_text: String,
    },

    /// There is no live session to end.
    #[error("This party isn't live right now.")]
    NotLive {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The request body was rejected.
    #[error("{message}")]
    InvalidRequest {
        /// The platform's own sentence.
        server_text: String,
        /// Human copy.
        message: String,
    },

    /// The platform failed to prepare media.
    #[error("The platform couldn't prepare media. Try again.")]
    Server {
        /// The platform's own sentence.
        server_text: String,
    },

    /// The network or the platform itself failed. The only retryable class.
    #[error("{0}")]
    Transport(String),

    /// The response was not the shape the contract promises.
    #[error("The platform sent something this app could not read.")]
    UnexpectedResponse {
        /// What was wrong.
        detail: String,
    },
}

impl PartyError {
    /// The platform's verbatim sentence, for the technical-detail disclosure.
    #[must_use]
    pub fn server_text(&self) -> &str {
        match self {
            Self::NotAMember { server_text }
            | Self::Banned { server_text }
            | Self::NoLiveSession { server_text }
            | Self::NotPermitted { server_text }
            | Self::NoWorker { server_text }
            | Self::WorkerEndpointMissing { server_text }
            | Self::PartyNotFound { server_text }
            | Self::NotLive { server_text }
            | Self::InvalidRequest { server_text, .. }
            | Self::Server { server_text } => server_text,
            Self::SignInRefused(message)
            | Self::Transport(message)
            | Self::UnexpectedResponse { detail: message } => message,
            Self::Unauthenticated => "invalid_token",
            Self::InsufficientScope => "insufficient_scope",
            Self::SignInDenied => "access_denied",
            Self::SignInCancelled => "",
        }
    }

    /// Whether retrying the same request could plausibly succeed.
    ///
    /// Only a transport fault qualifies. A refusal needs a person, a role change, a started
    /// session, or a different party — retrying is how a console turns a permission problem
    /// into a spinner that never stops.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Transport(_))
    }

    /// Whether this failure means the session is over and the browser flow must be re-run.
    #[must_use]
    pub const fn needs_sign_in(&self) -> bool {
        matches!(self, Self::Unauthenticated)
    }

    /// Maps a status and the platform's plain-text body onto a typed failure.
    ///
    /// An unrecognised body becomes a generic class rather than being swallowed, and the
    /// body is always preserved as the server text.
    #[must_use]
    pub fn from_response(status: u16, body: &str) -> Self {
        let text = body.trim().to_string();
        match (status, text.as_str()) {
            (403, "insufficient_scope") => Self::InsufficientScope,
            (403, "Members only.") => Self::NotAMember { server_text: text },
            (403, "You are banned from this party.") => Self::Banned { server_text: text },
            (403, _) => Self::NotPermitted { server_text: text },
            (404, _) => Self::PartyNotFound { server_text: text },
            (409, "No live session.") => Self::NoLiveSession { server_text: text },
            (409, "Session not live.") => Self::NotLive { server_text: text },
            (503, "No dedicated media worker available.") => Self::NoWorker { server_text: text },
            (503, "Assigned worker has no endpoint yet.") => {
                Self::WorkerEndpointMissing { server_text: text }
            }
            (400, _) => Self::InvalidRequest {
                server_text: text.clone(),
                message: invalid_request_message(&text),
            },
            (500, _) | (502, _) | (504, _) => Self::Server { server_text: text },
            _ => Self::UnexpectedResponse {
                detail: format!("HTTP {status}: {text}"),
            },
        }
    }
}

/// Human copy for the platform's `400` validation sentences.
fn invalid_request_message(server_text: &str) -> String {
    match server_text {
        "Bad kind." => "That isn't a kind the party accepts.".to_string(),
        "Label: max 60 chars." => "The label is longer than 60 characters.".to_string(),
        "" => "The platform rejected that request.".to_string(),
        other => format!("The platform rejected that request: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_refusal_maps_to_its_own_variant() {
        /// How one documented refusal is expected to map.
        type Case = (u16, &'static str, fn(&PartyError) -> bool);
        let cases: [Case; 9] = [
            (403, "Members only.", |e| {
                matches!(e, PartyError::NotAMember { .. })
            }),
            (403, "You are banned from this party.", |e| {
                matches!(e, PartyError::Banned { .. })
            }),
            (403, "Only the owner or director can go live.", |e| {
                matches!(e, PartyError::NotPermitted { .. })
            }),
            (409, "No live session.", |e| {
                matches!(e, PartyError::NoLiveSession { .. })
            }),
            (409, "Session not live.", |e| {
                matches!(e, PartyError::NotLive { .. })
            }),
            (404, "Party not found.", |e| {
                matches!(e, PartyError::PartyNotFound { .. })
            }),
            (503, "No dedicated media worker available.", |e| {
                matches!(e, PartyError::NoWorker { .. })
            }),
            (503, "Assigned worker has no endpoint yet.", |e| {
                matches!(e, PartyError::WorkerEndpointMissing { .. })
            }),
            (500, "Server could not prepare media.", |e| {
                matches!(e, PartyError::Server { .. })
            }),
        ];
        for (status, body, check) in cases {
            let error = PartyError::from_response(status, body);
            assert!(check(&error), "{status} {body} mapped to {error:?}");
            assert_eq!(
                error.server_text(),
                body,
                "the platform's sentence must survive"
            );
        }
    }

    #[test]
    fn a_scope_refusal_is_its_own_failure_not_a_permission_problem() {
        let error = PartyError::from_response(403, "insufficient_scope");
        assert_eq!(error, PartyError::InsufficientScope);
        assert!(
            !error.is_retryable(),
            "a missing scope will not appear on its own"
        );
    }

    #[test]
    fn validation_failures_keep_the_servers_sentence_and_add_human_copy() {
        let error = PartyError::from_response(400, "Label: max 60 chars.");
        let PartyError::InvalidRequest {
            server_text,
            message,
        } = &error
        else {
            panic!("expected InvalidRequest, got {error:?}");
        };
        assert_eq!(server_text, "Label: max 60 chars.");
        assert_eq!(message, "The label is longer than 60 characters.");

        let error = PartyError::from_response(400, "Bad kind.");
        assert_eq!(error.to_string(), "That isn't a kind the party accepts.");
    }

    #[test]
    fn only_a_transport_fault_is_retryable() {
        assert!(PartyError::Transport("reset".into()).is_retryable());
        for status in [400u16, 403, 404, 409, 500, 503] {
            assert!(
                !PartyError::from_response(status, "whatever").is_retryable(),
                "HTTP {status} must not be retried automatically"
            );
        }
    }

    #[test]
    fn only_an_expired_token_needs_the_browser_flow_again() {
        assert!(PartyError::Unauthenticated.needs_sign_in());
        assert!(!PartyError::InsufficientScope.needs_sign_in());
        assert!(
            !PartyError::NoLiveSession {
                server_text: "No live session.".into()
            }
            .needs_sign_in()
        );
    }

    #[test]
    fn human_copy_never_repeats_the_technical_sentence_verbatim() {
        for (status, body) in [(403u16, "Members only."), (409, "No live session.")] {
            let error = PartyError::from_response(status, body);
            assert_ne!(
                error.to_string(),
                body,
                "{status} copy is just the server text"
            );
        }
    }

    #[test]
    fn an_unrecognised_response_is_kept_rather_than_swallowed() {
        let error = PartyError::from_response(418, "I'm a teapot");
        assert!(matches!(error, PartyError::UnexpectedResponse { .. }));
        assert!(error.server_text().contains("418"));
        assert!(error.server_text().contains("teapot"));
    }
}
