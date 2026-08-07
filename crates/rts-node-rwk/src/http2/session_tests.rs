//! The session state machine, driven with no socket.
//!
//! Its own file rather than a `mod tests` at the bottom of `session.rs`: that
//! file is the protocol and this is the harness, and the file-size ceiling in
//! `CLAUDE.md` counts them together.

use super::frame::{self, CONNECTION_PREFACE, FrameType};
use super::session::*;

/// Pumps whatever each side has to say into the other until both are quiet.
///
/// This is the whole reason the state machine takes bytes rather than a socket:
/// a real client and a real server run against each other here with no network,
/// no thread and no heap, so an edge case is reachable by a test instead of by a
/// packet capture.
fn exchange(a: &mut Connection, b: &mut Connection) -> (Vec<Event>, Vec<Event>) {
    let mut from_a = Vec::new();
    let mut from_b = Vec::new();
    for _ in 0..8 {
        let a_out = a.take_outbound();
        let b_out = b.take_outbound();
        if a_out.is_empty() && b_out.is_empty() {
            break;
        }
        from_b.extend(b.receive(&a_out));
        from_a.extend(a.receive(&b_out));
    }
    (from_a, from_b)
}

fn request() -> Vec<(String, String)> {
    [
        (":method", "GET"),
        (":scheme", "http"),
        (":authority", "localhost"),
        (":path", "/"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect()
}

#[test]
fn a_request_and_its_response_cross_a_whole_session() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    let id = client.send_headers(&request(), true);
    // A client's first stream is 1 — odd, RFC 9113 §5.1.1 — and a server that
    // numbered its own the same way would collide on the first response.
    assert_eq!(id, 1);
    let (_, at_server) = exchange(&mut client, &mut server);

    let seen = at_server.iter().find_map(|event| match event {
        Event::Headers { stream_id, fields, end_stream } => {
            Some((*stream_id, fields.clone(), *end_stream))
        }
        _ => None,
    });
    let (stream_id, fields, end_stream) = seen.expect("the server saw the request");
    assert_eq!(stream_id, 1);
    assert!(end_stream, "a GET with no body ends its stream");
    assert!(fields.contains(&(":path".to_owned(), "/".to_owned())));

    server.respond(stream_id, &[(":status".to_owned(), "200".to_owned())], false);
    server.send_data(stream_id, b"hello", true);
    let (at_client, _) = exchange(&mut client, &mut server);

    let status = at_client.iter().find_map(|event| match event {
        Event::Headers { fields, .. } => fields
            .iter()
            .find(|(name, _)| name == ":status")
            .map(|(_, value)| value.clone()),
        _ => None,
    });
    assert_eq!(status.as_deref(), Some("200"));
    let body = at_client.iter().find_map(|event| match event {
        Event::Data { bytes, end_stream, .. } => Some((bytes.clone(), *end_stream)),
        _ => None,
    });
    assert_eq!(body, Some((b"hello".to_vec(), true)));
}

#[test]
fn a_settings_exchange_is_acknowledged_by_both_sides() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    let (at_client, at_server) = exchange(&mut client, &mut server);
    assert!(at_client.iter().any(|e| matches!(e, Event::Settings(_))));
    assert!(at_server.iter().any(|e| matches!(e, Event::Settings(_))));
    // Quiet afterwards. An acknowledgement that is itself acknowledged is an
    // infinite exchange, and this is what catches it.
    assert!(client.take_outbound().is_empty());
    assert!(server.take_outbound().is_empty());
}

#[test]
fn a_ping_is_echoed_with_its_own_payload() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    exchange(&mut client, &mut server);
    client.send_ping([1, 2, 3, 4, 5, 6, 7, 8]);
    let (at_client, _) = exchange(&mut client, &mut server);
    assert!(at_client.contains(&Event::PingAck([1, 2, 3, 4, 5, 6, 7, 8])));
}

/// CVE-2023-44487: a stream reset right after it is opened costs the peer one
/// frame and costs this side a whole request, and does NOT count against the
/// concurrency limit — so the limit is not a defence and the budget is.
#[test]
fn a_flood_of_resets_ends_the_connection() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    exchange(&mut client, &mut server);
    for _ in 0..(RESET_BUDGET + 1) {
        let id = client.send_headers(&request(), false);
        client.send_reset(id, CANCEL);
    }
    let (at_client, at_server) = exchange(&mut client, &mut server);
    assert!(
        at_server.contains(&Event::Failed("the peer reset too many streams")),
        "the server did not notice: {at_server:?}"
    );
    let told = at_client.iter().any(|event| {
        matches!(event, Event::Goaway { error_code, .. } if *error_code == ENHANCE_YOUR_CALM)
    });
    assert!(told, "the client was not told why: {at_client:?}");
}

/// One under the budget is a client cancelling everything it asked for, which is
/// legitimate. A mitigation that fires on it is a bug.
#[test]
fn resets_within_the_budget_are_left_alone() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    exchange(&mut client, &mut server);
    for _ in 0..RESET_BUDGET {
        let id = client.send_headers(&request(), false);
        client.send_reset(id, CANCEL);
    }
    let (_, at_server) = exchange(&mut client, &mut server);
    assert!(!at_server.iter().any(|e| matches!(e, Event::Failed(_))));
    assert!(!server.finished());
}

#[test]
fn a_wrong_preface_is_refused_rather_than_parsed_as_frames() {
    let mut server = Connection::new(Side::Server);
    let events = server.receive(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n\r\n\r\n\r\n");
    assert!(events.contains(&Event::Failed("the client's connection preface was wrong")));
    assert!(server.finished());
}

/// Flow control is real: a peer that advertises a small window gets no more than
/// that, and `send_data` says how much it actually sent.
#[test]
fn sending_is_bounded_by_the_window_the_peer_advertised() {
    let mut client = Connection::new(Side::Client);
    let mut server = Connection::new(Side::Server);
    exchange(&mut client, &mut server);
    // The server tells the client its streams start with ten bytes.
    let settings = frame::write_settings(&[(0x4, 10)]);
    client.receive(&frame::write_frame(FrameType::Settings, 0, 0, &settings));
    let id = client.send_headers(&request(), false);
    let sent = client.send_data(id, b"0123456789abcdef", false);
    assert_eq!(sent, 10, "the window was ten bytes and sixteen were offered");
    // And a WINDOW_UPDATE reopens it.
    let update = frame::write_window_update(6);
    client.receive(&frame::write_frame(FrameType::WindowUpdate, 0, id, &update));
    assert_eq!(client.send_data(id, b"abcdef", false), 6);
}

/// A window raised by SETTINGS applies to streams already open, as a DELTA —
/// RFC 9113 §6.9.2. Assigning instead of adding is the mistake that stalls a
/// connection whose peer raises the window mid-flight.
#[test]
fn a_later_settings_adjusts_open_streams_by_the_difference() {
    let mut client = Connection::new(Side::Client);
    let id = client.send_headers(&request(), false);
    let smaller = frame::write_settings(&[(0x4, 10)]);
    client.receive(&frame::write_frame(FrameType::Settings, 0, 0, &smaller));
    assert_eq!(client.send_data(id, &[0; 32], false), 10);
    let larger = frame::write_settings(&[(0x4, 20)]);
    client.receive(&frame::write_frame(FrameType::Settings, 0, 0, &larger));
    // Ten more, not twenty: the stream had already spent its first ten.
    assert_eq!(client.send_data(id, &[0; 32], false), 10);
}

/// A header block that does not fit one frame is refused by name. Waiting for a
/// CONTINUATION this implementation cannot parse would hang.
#[test]
fn a_header_block_spanning_frames_is_refused_and_not_awaited() {
    let mut server = Connection::new(Side::Server);
    server.receive(CONNECTION_PREFACE);
    let events = server.receive(&frame::write_frame(FrameType::Headers, 0, 1, &[0x82]));
    assert!(events.contains(&Event::Failed("a header block spanning frames is not supported")));
}
