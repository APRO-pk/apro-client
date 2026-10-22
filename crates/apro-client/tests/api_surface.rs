//! Pins the API shapes that `README.md` shows.
//!
//! The README is the connection guide every app is pointed at, so its examples must
//! actually compile. These tests build the same structs and call the same constructors;
//! if the API moves, this fails instead of the documentation going quietly stale.
//!
//! Nothing here talks to a store. It is a shape check, not an integration test.

use apro_client::{
    AppInterface, ClientConfig, CommandCost, CommandDecl, CommandKind, CommandOutcome, ConsumeDecl,
    EdgeRequest, Encoding, HttpStoreClient, Mode, PublishDecl, Selector, Status, TypeId,
};

#[test]
fn declaring_an_interface_matches_the_readme() {
    let interface = AppInterface {
        app: "apro-cad".into(),
        publishes: vec![
            PublishDecl::from(TypeId::parse("apro-cad/mass-properties-si-v1").unwrap())
                .with_label("Mass properties")
                .with_description("Volume, mass, centre of mass and inertia.")
                .with_order(10),
            PublishDecl::from(TypeId::parse("apro-cad/mesh-stl-v1").unwrap())
                .with_label("STL mesh")
                .with_status(Status::Planned)
                .disabled_by_default()
                .with_order(30),
        ],
        consumes: vec![ConsumeDecl::from(
            TypeId::parse("burn-geometry-modeler/grain-geometry-v1").unwrap(),
        )
        .with_mode(Mode::Pinned)
        .with_label("Grain geometry")],
        commands: vec![
            CommandDecl::new("get-state", "Read the vehicle document as it stands"),
            CommandDecl::new(
                "set-parameter",
                "Set one vehicle parameter by name, for example dry-mass",
            )
            .writing()
            .with_params(serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "value": { "type": "number" }
                },
                "required": ["name", "value"],
                "additionalProperties": false
            })),
        ],
    };

    assert_eq!(interface.app, "apro-cad");
    assert_eq!(
        interface.publishes[0].type_id.as_str(),
        "apro-cad/mass-properties-si-v1"
    );
    assert_eq!(
        interface.publishes[0].label.as_deref(),
        Some("Mass properties")
    );
    assert_eq!(interface.consumes[0].default_mode, Mode::Pinned);

    // The defaults the README promises: live, enabled, JSON, order 0 unless said otherwise.
    assert_eq!(interface.publishes[0].status, Status::Live);
    assert!(interface.publishes[0].default_enabled);
    assert_eq!(interface.publishes[0].encoding, Encoding::Json);
    assert_eq!(interface.publishes[0].item_order, 10);

    // A planned element is catalogued but not claimed.
    assert_eq!(interface.publishes[1].status, Status::Planned);
    assert!(!interface.publishes[1].default_enabled);

    assert_eq!(interface.live_publishes().len(), 1);
    assert_eq!(interface.publishes.len(), 2);

    // Commands are a separate namespace with their own defaults.
    assert_eq!(interface.commands.len(), 2);
    assert_eq!(interface.commands[0].kind, CommandKind::Read);
    assert_eq!(interface.commands[0].cost, CommandCost::Instant);
    assert_eq!(interface.commands[0].params, None);
    assert_eq!(interface.commands[1].kind, CommandKind::Write);
    assert!(interface.commands[1].kind.changes_something());
    assert!(interface.commands[1].params.is_some());
}

#[test]
fn a_command_outcome_says_what_it_did() {
    let done = CommandOutcome::ok(serde_json::json!({
        "summary": "dry-mass is now 12.5 kg",
        "data": { "revision": 4 }
    }));
    assert!(done.ok);
    assert!(!done.stale);
    assert_eq!(done.summary(), Some("dry-mass is now 12.5 kg"));

    // A mutating command that could not republish admits it rather than lying.
    let stale = CommandOutcome::ok_stale(serde_json::json!({ "summary": "moved" }));
    assert!(stale.ok && stale.stale);

    // A refusal that names the alternatives is one the agent can recover from.
    let refused = CommandOutcome::failed_with(
        "no parameter called \"dry-mass\"",
        vec!["dry_mass".into(), "propellant_mass".into()],
    );
    assert!(!refused.ok);
    assert_eq!(refused.summary(), Some("no parameter called \"dry-mass\""));
    assert_eq!(refused.valid.as_deref().map(|v| v.len()), Some(2));
}

#[test]
fn registering_an_edge_matches_the_readme() {
    let request = EdgeRequest {
        consumer_app: String::new(),
        consumer_ref: None,
        type_id: TypeId::parse("apro-cad/mass-properties").unwrap(),
        instance: "satellite-a".into(),
        mode: Mode::Pinned,
        pinned_revision_number: Some(3),
        min_revision_number: None,
    };

    assert_eq!(request.instance, "satellite-a");
    assert_eq!(request.mode, Mode::Pinned);
}

#[test]
fn connecting_by_hand_matches_the_readme() {
    // `new` must not require a credential: a CLI tool or a test connects to a local
    // store with nothing but an endpoint and a slug.
    let config = ClientConfig::new("http://127.0.0.1:5555", "my-app");
    assert_eq!(config.endpoint, "http://127.0.0.1:5555");
    assert_eq!(config.app_slug, "my-app");
    assert!(config.launch_ticket.is_none());
    assert!(config.session_token.is_none());

    // A builder chain that still exists, even though this test does not connect.
    let _ = HttpStoreClient::connect as fn(ClientConfig) -> apro_client::Result<HttpStoreClient>;
}

#[test]
fn selectors_and_encodings_match_the_readme() {
    assert_eq!(Selector::Latest, Selector::default());
    assert_eq!(Selector::Number(3), Selector::Number(3));

    for encoding in [
        Encoding::Json,
        Encoding::Protobuf,
        Encoding::Blob,
        Encoding::Text,
    ] {
        assert_eq!(Encoding::parse(encoding.as_str()).unwrap(), encoding);
    }
}

#[test]
fn type_ids_are_kebab_case_and_dot_free() {
    let id = TypeId::parse("apro-cad/mass-properties").unwrap();
    assert_eq!(id.owner_app(), "apro-cad");
    assert_eq!(id.name(), "mass-properties");

    // The README states dots are rejected. Hold it to that.
    assert!(TypeId::parse("apro.cad/mass-properties").is_err());
    assert!(TypeId::parse("apro-cad.mass-properties").is_err());
    assert!(TypeId::parse("NotKebab/x").is_err());
}

#[test]
fn modes_round_trip_through_their_wire_names() {
    for mode in [Mode::Pinned, Mode::Tracking, Mode::Compatible] {
        assert_eq!(Mode::parse(mode.as_str()).unwrap(), mode);
    }
    assert_eq!(Mode::Pinned.as_str(), "pinned");
}
