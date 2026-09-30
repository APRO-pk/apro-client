//! Pins the API shapes that `README.md` shows.
//!
//! The README is the connection guide every app is pointed at, so its examples must
//! actually compile. These tests build the same structs and call the same constructors;
//! if the API moves, this fails instead of the documentation going quietly stale.
//!
//! Nothing here talks to a store. It is a shape check, not an integration test.

use apro_client::{
    AppInterface, ClientConfig, ConsumeDecl, EdgeRequest, Encoding, HttpStoreClient, Mode,
    Selector, TypeId,
};

#[test]
fn declaring_an_interface_matches_the_readme() {
    let interface = AppInterface {
        app: "apro-cad".into(),
        publishes: vec![TypeId::parse("apro-cad/mass-properties").unwrap()],
        consumes: vec![ConsumeDecl {
            type_id: TypeId::parse("hexadof/model-file").unwrap(),
            default_mode: Mode::Pinned,
        }],
    };

    assert_eq!(interface.app, "apro-cad");
    assert_eq!(
        interface.publishes[0].to_string(),
        "apro-cad/mass-properties"
    );
    assert_eq!(interface.consumes[0].default_mode, Mode::Pinned);
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
