//! What the media engine is doing, as far as the console can honestly report it.
//!
//! `studio-obs` links libobs and has a verified startup smoke, but the console does not
//! yet own a long-lived engine runtime. The status reports that integration boundary
//! instead of implying that libobs is absent or that publishing is ready.

/// The media engine's observable state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EngineStatus {
    /// The libobs runtime is not yet integrated into the console.
    #[default]
    NotIntegrated,
    /// libobs is integrated but no profile is loaded.
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
            Self::NotIntegrated => "Media engine not integrated".to_string(),
            Self::Idle => "No profile loaded".to_string(),
            Self::Ready { profile } => format!("Profile: {profile}"),
        }
    }

    /// The reason publish controls are unavailable, or `None` when they are available.
    #[must_use]
    pub fn blocked_reason(&self) -> Option<&'static str> {
        match self {
            Self::NotIntegrated => Some(
                "The libobs smoke works, but the console has no long-lived engine runtime or publish output yet.",
            ),
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
        assert!(!EngineStatus::NotIntegrated.can_publish());
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
            EngineStatus::NotIntegrated.can_publish(),
            EngineStatus::NotIntegrated.blocked_reason().is_none()
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
