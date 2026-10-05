//! What the media engine is doing, as far as the console can honestly report it.
//!
//! libobs is not linked into this build yet (spike S2 in `docs/PLAN.md`). The console
//! must not imply a live engine it does not have, so the status is an explicit variant
//! with a stated reason rather than a defaulted "ready".

/// The media engine's observable state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EngineStatus {
    /// libobs is not present in this build.
    #[default]
    NotBuilt,
    /// libobs is present but no profile is loaded.
    Idle,
    /// A profile is loaded and the compositor is running.
    Ready {
        /// Name of the loaded profile.
        profile: String,
    },
}

impl EngineStatus {
    /// Whether the console may offer publish controls.
    #[must_use]
    pub const fn can_publish(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// One line for the status bar. Never claims more than is true.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::NotBuilt => "Media engine not built".to_string(),
            Self::Idle => "No profile loaded".to_string(),
            Self::Ready { profile } => format!("Profile: {profile}"),
        }
    }

    /// The reason publish controls are unavailable, or `None` when they are available.
    #[must_use]
    pub fn blocked_reason(&self) -> Option<&'static str> {
        match self {
            Self::NotBuilt => {
                Some("Publishing needs the libobs engine, which is not part of this build yet.")
            }
            Self::Idle => Some("Load a profile before publishing."),
            Self::Ready { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_ready_engine_may_publish() {
        assert!(!EngineStatus::NotBuilt.can_publish());
        assert!(!EngineStatus::Idle.can_publish());
        assert!(
            EngineStatus::Ready {
                profile: "Default".into()
            }
            .can_publish()
        );
    }

    #[test]
    fn blocked_reason_is_present_exactly_when_publish_is_unavailable() {
        assert_eq!(
            EngineStatus::NotBuilt.can_publish(),
            EngineStatus::NotBuilt.blocked_reason().is_none()
        );
        assert_eq!(
            EngineStatus::Idle.can_publish(),
            EngineStatus::Idle.blocked_reason().is_none()
        );
        assert_eq!(
            EngineStatus::Ready {
                profile: "D".into()
            }
            .can_publish(),
            EngineStatus::Ready {
                profile: "D".into()
            }
            .blocked_reason()
            .is_none()
        );
    }

    #[test]
    fn summary_names_the_profile_when_ready() {
        assert_eq!(
            EngineStatus::Ready {
                profile: "Friday Night".into()
            }
            .summary(),
            "Profile: Friday Night"
        );
    }
}
