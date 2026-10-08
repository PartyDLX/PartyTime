//! The publish-kind vocabulary shared with the OpenParty platform.
//!
//! These are the platform's strings, defined once here so the wire contract and the
//! local profile cannot drift apart.

use serde::{Deserialize, Serialize};

/// A kind of input a publisher may declare to a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublishKind {
    /// Screen, window, or game capture.
    Gameplay,
    /// Camera device.
    Camera,
    /// Microphone.
    Mic,
    /// Audio captured from the game.
    GameAudio,
    /// Audio from the party's voice chat.
    PartyAudio,
}

impl PublishKind {
    /// Every kind, in the order the consent rail lists them.
    pub const ALL: [Self; 5] = [
        Self::Gameplay,
        Self::Camera,
        Self::Mic,
        Self::GameAudio,
        Self::PartyAudio,
    ];

    /// The platform's wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gameplay => "gameplay",
            Self::Camera => "camera",
            Self::Mic => "mic",
            Self::GameAudio => "game-audio",
            Self::PartyAudio => "party-audio",
        }
    }

    /// Parses a platform wire string.
    ///
    /// Named `parse` rather than `from_str` so it is not mistaken for the
    /// `FromStr` trait, which this does not implement.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// Whether this kind contributes video to the published stream.
    #[must_use]
    pub const fn is_video(self) -> bool {
        matches!(self, Self::Gameplay | Self::Camera)
    }

    /// Whether this kind contributes audio to the published stream.
    #[must_use]
    pub const fn is_audio(self) -> bool {
        matches!(self, Self::Mic | Self::GameAudio | Self::PartyAudio)
    }

    /// The label shown in the console.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gameplay => "Gameplay",
            Self::Camera => "Camera",
            Self::Mic => "Mic",
            Self::GameAudio => "Game audio",
            Self::PartyAudio => "Party audio",
        }
    }
}

impl std::fmt::Display for PublishKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_wire_string() {
        for kind in PublishKind::ALL {
            assert_eq!(PublishKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn unknown_wire_strings_are_rejected_rather_than_defaulted() {
        assert_eq!(PublishKind::parse("screen"), None);
        assert_eq!(PublishKind::parse(""), None);
        assert_eq!(PublishKind::parse("Gameplay"), None);
    }

    #[test]
    fn wire_strings_match_the_platform_documentation() {
        assert_eq!(PublishKind::Gameplay.as_str(), "gameplay");
        assert_eq!(PublishKind::GameAudio.as_str(), "game-audio");
        assert_eq!(PublishKind::PartyAudio.as_str(), "party-audio");
    }

    #[test]
    fn serde_uses_the_wire_strings() {
        let json = serde_json::to_string(&PublishKind::GameAudio).expect("serialise");
        assert_eq!(json, "\"game-audio\"");
        let back: PublishKind = serde_json::from_str("\"party-audio\"").expect("deserialise");
        assert_eq!(back, PublishKind::PartyAudio);
        assert!(serde_json::from_str::<PublishKind>("\"nope\"").is_err());
    }

    #[test]
    fn video_and_audio_kinds_are_disjoint() {
        for kind in PublishKind::ALL {
            assert!(!(kind.is_video() && kind.is_audio()), "{kind} is both");
        }
    }
}
