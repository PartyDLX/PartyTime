//! The `/api/partytime/v1` wire types.
//!
//! Two rules that are easy to get wrong and expensive to get wrong:
//!
//! * **Ids are used exactly as the platform sends them.** `GET /parties` returns a bare id
//!   that is appended to the path as-is; `POST /parties/{id}/publish` echoes it back with a
//!   `party:` prefix. Stripping or adding one produces a 404, so nothing here parses,
//!   trims or re-prefixes an id.
//! * **Nothing is optional that the platform always sends.** Optionality here means the
//!   field can genuinely be absent, not that the console has not seen it yet.

use serde::{Deserialize, Serialize};

use crate::kind::PublishKind;

/// A party-scoped role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PartyRole {
    /// Owns the party.
    Owner,
    /// Cuts the program output.
    Director,
    /// Reviews inputs and ends sessions.
    Moderator,
    /// Publishes.
    Member,
}

impl PartyRole {
    /// The label the console shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Owner => "Owner",
            Self::Director => "Director",
            Self::Moderator => "Moderator",
            Self::Member => "Member",
        }
    }

    /// Whether this role may declare and review publish inputs.
    #[must_use]
    pub const fn can_review_inputs(self) -> bool {
        matches!(self, Self::Owner | Self::Moderator)
    }

    /// Whether this role may take the party live and mint publish credentials.
    #[must_use]
    pub const fn can_go_live(self) -> bool {
        matches!(self, Self::Owner | Self::Director)
    }
}

impl std::fmt::Display for PartyRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Where a declared input stands in the consent flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsentState {
    /// Declared, not yet reviewed.
    Pending,
    /// Reviewed and allowed.
    Approved,
    /// Reviewed and refused, or revoked.
    Revoked,
}

impl ConsentState {
    /// The label the console shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Awaiting owner",
            Self::Approved => "Approved",
            Self::Revoked => "Revoked",
        }
    }
}

/// The signed-in user, as the platform describes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// User id. Bare, and used as the platform sends it.
    pub id: String,
    /// Public handle.
    pub handle: String,
    /// Chosen display name.
    pub display_name: String,
    /// Avatar, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Banner, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    /// Bio, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    /// Links, when set.
    #[serde(default)]
    pub links: Vec<String>,
}

/// `GET /me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    /// Always true on success.
    pub ok: bool,
    /// The client id this token was issued to.
    pub client_id: String,
    /// The scopes the token actually carries.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Who is signed in.
    pub profile: Profile,
}

impl Me {
    /// Whether a scope is present on the current token.
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|granted| granted == scope)
    }
}

/// The live session on a party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartySession {
    /// Session id, bare.
    pub id: String,
    /// `live` or `offline`.
    pub status: String,
    /// When it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

/// One input the signed-in user has declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyInput {
    /// The party kind.
    pub kind: PublishKind,
    /// Optional label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Where it stands, including still-pending declarations.
    pub consent: ConsentState,
}

/// A party, as the list endpoint describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartySummary {
    /// Party id, bare — append to the path exactly as written.
    pub id: String,
    /// Display title.
    pub title: String,
    /// `public` or `private`.
    pub visibility: String,
    /// `draft`, `live` or `offline`.
    pub status: String,
    /// Game name, when the party set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
    /// Linked channel, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    /// Whether viewers may leave the program output.
    #[serde(default)]
    pub allow_rogue: bool,
    /// The caller's role.
    pub role: PartyRole,
    /// Whether the caller holds the director seat.
    #[serde(default)]
    pub is_director: bool,
    /// Whether the caller may take the party live.
    #[serde(default)]
    pub can_go_live: bool,
    /// The live session, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<PartySession>,
    /// Everything the caller has declared, pending included.
    #[serde(default)]
    pub my_inputs: Vec<MyInput>,
    /// The kinds the owner has approved for the caller.
    #[serde(default)]
    pub approved_kinds: Vec<PublishKind>,
    /// The director's handle, when the seat is filled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub director_handle: Option<String>,
}

impl PartySummary {
    /// Whether the party has a live session right now.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.status == "live")
    }

    /// The caller's declarations of one kind, if any.
    #[must_use]
    pub fn input(&self, kind: PublishKind) -> Option<&MyInput> {
        self.my_inputs.iter().find(|input| input.kind == kind)
    }
}

/// The live party linked to a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelLiveParty {
    /// Party id, bare.
    pub id: String,
    /// Session id, bare.
    pub session_id: String,
    /// Party title.
    pub title: String,
    /// `public` or `private`.
    pub visibility: String,
    /// Game name, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// How many are watching.
    #[serde(default)]
    pub viewer_count: u64,
}

/// What a channel is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelActivity {
    /// Subscriber count.
    #[serde(default)]
    pub subscriber_count: u64,
    /// Parties linked to the channel.
    #[serde(default)]
    pub party_count: u64,
    /// The channel's live party, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_party: Option<ChannelLiveParty>,
}

/// One channel the signed-in user owns or edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// Channel id, bare.
    pub id: String,
    /// Public handle.
    pub handle: String,
    /// Display name.
    pub name: String,
    /// Description, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Logo, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    /// `public` or `private`.
    #[serde(default)]
    pub visibility: String,
    /// The caller's relationship to it.
    pub role: String,
    /// What the channel is doing.
    #[serde(default)]
    pub activity: ChannelActivity,
}

/// `GET /channels`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelList {
    /// Always true on success.
    pub ok: bool,
    /// The channels the caller can see.
    #[serde(default)]
    pub channels: Vec<Channel>,
}

impl MyInput {
    /// Whether the owner has approved this declaration.
    #[must_use]
    pub const fn is_approved(&self) -> bool {
        matches!(self.consent, ConsentState::Approved)
    }
}

/// `GET /parties`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyList {
    /// Always true on success.
    pub ok: bool,
    /// Whether the list was filtered to live parties.
    #[serde(default)]
    pub live_only: bool,
    /// Live parties first, then by title.
    #[serde(default)]
    pub parties: Vec<PartySummary>,
}

/// One member of a party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterMember {
    /// User id, bare.
    pub user_id: String,
    /// Public handle. What the console shows.
    pub handle: String,
    /// Display name.
    pub display_name: String,
    /// Avatar, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Party role.
    pub role: PartyRole,
}

/// Someone connected to the party right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    /// User id, bare.
    pub user_id: String,
    /// Whether they are publishing. Consent-derived, not RTP-derived.
    #[serde(default)]
    pub publishing: bool,
}

/// `GET /parties/{id}` — the list item plus the roster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyDetail {
    /// The list item, verbatim.
    #[serde(flatten)]
    pub party: PartySummary,
    /// Everyone on the roster.
    #[serde(default)]
    pub roster: Vec<RosterMember>,
    /// Who is connected.
    #[serde(default)]
    pub present: Vec<Presence>,
    /// The caller's own role, with no admin bypass.
    #[serde(default)]
    pub power: Option<PartyRole>,
}

impl PartyDetail {
    /// One member by user id.
    #[must_use]
    pub fn member(&self, user_id: &str) -> Option<&RosterMember> {
        self.roster.iter().find(|member| member.user_id == user_id)
    }
}

/// A STUN or TURN server from a publish grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IceServer {
    /// `stun:` or `turn:` URLs.
    #[serde(default)]
    pub urls: Vec<String>,
    /// TURN username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// TURN credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// `POST /parties/{id}/publish` — everything one publish attempt needs.
///
/// The media token here lasts `token_expires_in_seconds` — sixty — and is single use. It
/// is not the OAuth access token and never crosses into the console's stored session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishGrant {
    /// Always true on success.
    pub ok: bool,
    /// Party id — note this arrives with the `party:` prefix the list endpoint omits.
    pub party_id: String,
    /// Session id, bare.
    pub session_id: String,
    /// The caller's role at mint time.
    pub role: PartyRole,
    /// The worker's WebSocket endpoint.
    pub worker_endpoint: String,
    /// The WHIP ingest URL. Authoritative: the path carries the worker slot.
    pub whip_ingest_url: String,
    /// The 60-second media token. Never logged, never stored.
    pub token: String,
    /// The only kinds that may be published.
    #[serde(default)]
    pub allowed_kinds: Vec<PublishKind>,
    /// ICE servers; empty when the worker advertises none.
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
    /// How long the media token is valid for.
    #[serde(default)]
    pub token_expires_in_seconds: u64,
}

/// A STUN or TURN server, as the browser-facing endpoints send it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyIceServer {
    /// `stun:` or `turn:` URLs.
    #[serde(default)]
    pub urls: Vec<String>,
    /// TURN username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// TURN credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// The body for `POST /parties/{id}/inputs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclareInput {
    /// The party kind.
    pub kind: PublishKind,
    /// Optional label, at most 60 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The platform's label limit, from `400 Label: max 60 chars.`
pub const MAX_LABEL_CHARS: usize = 60;

/// Whether a label fits the platform's limit.
#[must_use]
pub fn label_is_valid(label: &str) -> bool {
    label.chars().count() <= MAX_LABEL_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARTIES: &str = r#"{
      "ok": true, "liveOnly": true, "parties": [{
        "id": "wlyayz1ytl2u822bifb4",
        "title": "Friday Night", "visibility": "public", "status": "live",
        "gameName": "Helldivers 2", "channelId": null, "allowRogue": false,
        "role": "owner", "isDirector": false, "canGoLive": true,
        "session": { "id": "rldnbccaydlf15hgu16k", "status": "live",
                     "startedAt": "2026-10-04T11:41:19.272Z" },
        "myInputs": [
          { "kind": "camera", "label": "Face cam", "consent": "approved" },
          { "kind": "mic", "consent": "pending" }
        ],
        "approvedKinds": ["camera"],
        "directorHandle": "host"
      }]
    }"#;

    fn parties() -> PartyList {
        serde_json::from_str(PARTIES).expect("parses")
    }

    #[test]
    fn the_party_list_parses_with_pending_declarations_included() {
        let list = parties();
        assert!(list.ok);
        assert!(list.live_only);
        let party = &list.parties[0];
        assert_eq!(party.id, "wlyayz1ytl2u822bifb4");
        assert!(party.is_live());
        assert_eq!(party.role, PartyRole::Owner);
        assert!(party.can_go_live);
        // A pending declaration is how the console shows "waiting for owner approval".
        assert_eq!(party.my_inputs.len(), 2);
        assert_eq!(
            party.input(PublishKind::Mic).expect("declared").consent,
            ConsentState::Pending
        );
        assert_eq!(party.approved_kinds, vec![PublishKind::Camera]);
        assert_eq!(party.director_handle.as_deref(), Some("host"));
    }

    #[test]
    fn the_publish_grant_arrives_with_a_prefixed_party_id_and_the_list_does_not() {
        // These are two different shapes from two different endpoints. Normalising either
        // one produces a 404 against the worker slot path, so neither is touched.
        let list_id = parties().parties[0].id.clone();
        assert!(!list_id.starts_with("party:"), "the list id is bare");

        let grant: PublishGrant = serde_json::from_str(
            r#"{"ok":true,"partyId":"party:wlyayz1ytl2u822bifb4",
                "sessionId":"rldnbccaydlf15hgu16k","role":"owner",
                "workerEndpoint":"wss://openparty.example/media/w1",
                "whipIngestUrl":"https://openparty.example/whip/w1",
                "token":"eyJhbGciOiJIUzI1NiIs","allowedKinds":["camera","mic"],
                "iceServers":[],"tokenExpiresInSeconds":60}"#,
        )
        .expect("parses");

        assert!(
            grant.party_id.starts_with("party:"),
            "the grant id is prefixed"
        );
        assert_eq!(grant.whip_ingest_url, "https://openparty.example/whip/w1");
        assert_eq!(grant.token_expires_in_seconds, 60);
        assert_eq!(
            grant.allowed_kinds,
            vec![PublishKind::Camera, PublishKind::Mic]
        );
        assert!(grant.ice_servers.is_empty());
    }

    #[test]
    fn a_party_without_a_session_is_not_live() {
        let mut party = parties().parties[0].clone();
        party.session = None;
        assert!(!party.is_live());
        party.session = Some(PartySession {
            id: "s".into(),
            status: "offline".into(),
            started_at: None,
        });
        assert!(!party.is_live(), "a stopped session is not a live one");
    }

    #[test]
    fn the_detail_flattens_the_summary_without_duplicating_it() {
        let detail: PartyDetail = serde_json::from_str(
            r#"{"id":"p1","title":"Friday Night","visibility":"public","status":"live",
                "role":"member","isDirector":false,"canGoLive":false,
                "myInputs":[],"approvedKinds":["gameplay"],
                "roster":[{"userId":"u1","handle":"ada","displayName":"Ada","role":"owner"},
                          {"userId":"u2","handle":"lin","displayName":"Lin","role":"member"}],
                "present":[{"userId":"u2","publishing":true}],
                "power":"member"}"#,
        )
        .expect("parses");

        assert_eq!(detail.party.title, "Friday Night");
        assert_eq!(detail.roster.len(), 2);
        assert_eq!(detail.member("u1").expect("on the roster").handle, "ada");
        assert!(detail.member("nobody").is_none());
        assert!(detail.present[0].publishing);
        assert_eq!(detail.power, Some(PartyRole::Member));
    }

    #[test]
    fn the_me_document_reports_the_scopes_the_token_actually_carries() {
        let me: Me = serde_json::from_str(
            r#"{"ok":true,"clientId":"partytime-dev",
                "scopes":["profile:read","channels:read","parties:read","publish"],
                "profile":{"id":"i5g9espl8obrv10kdm9o","handle":"partytimesmoke",
                           "displayName":"PartyTime Smoke","avatarUrl":null,
                           "bannerUrl":null,"bio":null,"links":[],"updatedAt":null}}"#,
        )
        .expect("parses");

        assert!(me.has_scope("publish"));
        assert!(!me.has_scope("admin"));
        assert_eq!(me.profile.handle, "partytimesmoke");
    }

    #[test]
    fn a_declaration_serialises_without_an_absent_label() {
        let bare = serde_json::to_string(&DeclareInput {
            kind: PublishKind::Camera,
            label: None,
        })
        .expect("serialises");
        assert_eq!(bare, r#"{"kind":"camera"}"#);

        let labelled = serde_json::to_string(&DeclareInput {
            kind: PublishKind::Camera,
            label: Some("Face cam".into()),
        })
        .expect("serialises");
        assert_eq!(labelled, r#"{"kind":"camera","label":"Face cam"}"#);
    }

    #[test]
    fn the_label_limit_is_sixty_characters_counted_as_characters() {
        assert!(label_is_valid(&"a".repeat(60)));
        assert!(!label_is_valid(&"a".repeat(61)));
        assert!(label_is_valid(&"é".repeat(60)), "characters, not bytes");
        assert!(label_is_valid(""));
    }

    #[test]
    fn only_owner_and_director_may_go_live() {
        assert!(PartyRole::Owner.can_go_live());
        assert!(PartyRole::Director.can_go_live());
        assert!(!PartyRole::Moderator.can_go_live());
        assert!(!PartyRole::Member.can_go_live());
    }
}
