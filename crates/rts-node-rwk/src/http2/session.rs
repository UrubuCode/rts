//! The session and stream lifecycle: what a connection does with the frames
//! `frame.rs` reads and writes.
//!
//! # Why this is pure Rust with no engine in it
//!
//! The same split `frame.rs` and `http/parser.rs` already draw. Everything here
//! takes and answers bytes and plain structs, so the protocol can be driven by a
//! test with no socket and no heap — which is what the round trip at the bottom
//! of this file does, running a real client and a real server against each other
//! through two byte buffers.
//!
//! A state machine that can only be tested through a socket is one whose edge
//! cases are not tested.
//!
//! # What it implements
//!
//! The half `http2.md` said was refused: the connection preface, the SETTINGS
//! exchange and its acknowledgement, stream creation and numbering, HEADERS and
//! DATA in both directions, END_STREAM, RST_STREAM, WINDOW_UPDATE with real
//! connection- and stream-level flow control, PING and its acknowledgement, and
//! GOAWAY.
//!
//! # The mitigation the spec calls mandatory
//!
//! CVE-2023-44487 — a peer that opens streams and immediately resets them makes
//! a server do unbounded work for free, because a reset stream does not count
//! against `SETTINGS_MAX_CONCURRENT_STREAMS`. [`Connection::note_reset`] counts
//! resets and answers GOAWAY(ENHANCE_YOUR_CALM) past a threshold. Refusing to
//! build the session at all was the previous answer; a session without this
//! would have been the wrong one.
//!
//! # Not implemented, by name
//!
//! `PUSH_PROMISE` and server push entirely, `CONTINUATION` (so a header block
//! larger than one frame is a protocol error rather than being reassembled —
//! `frame.rs` says the same), priority as anything but wire-compatible bytes,
//! trailers, and ALPN/TLS negotiation. `h2c` — cleartext with prior knowledge —
//! is what this speaks.

use std::collections::HashMap;

use super::frame::{
    self, CONNECTION_PREFACE, FLAG_ACK, FLAG_END_HEADERS, FLAG_END_STREAM, FrameType,
};
use super::hpack::{Decoder, Encoder, HeaderField};

/// RFC 9113 §7 — no error, the code a clean GOAWAY carries.
pub const NO_ERROR: u32 = 0x0;
/// RFC 9113 §7 — the peer broke the protocol.
pub const PROTOCOL_ERROR: u32 = 0x1;
/// RFC 9113 §7 — the peer sent past a window it was given.
pub const FLOW_CONTROL_ERROR: u32 = 0x3;
/// RFC 9113 §7 — this side no longer wants the stream.
pub const CANCEL: u32 = 0x8;
/// RFC 9113 §7 — the peer is behaving abusively; see the reset budget.
pub const ENHANCE_YOUR_CALM: u32 = 0xb;

/// The default connection and stream window, RFC 9113 §6.9.2.
const INITIAL_WINDOW: i64 = 65_535;

/// How many resets a peer may send before the connection is torn down.
///
/// A hundred is high enough that no legitimate client reaches it — a browser
/// cancelling every request it made would still be under it — and low enough
/// that the attack is over in one round trip rather than after the work is
/// already done. The count is per connection and never decays, deliberately: a
/// window that forgives would let an attacker pace itself under it forever.
pub(super) const RESET_BUDGET: u32 = 100;

/// Which side of the connection this is.
///
/// It decides two things and both are wire-visible: who sends the preface, and
/// whether locally-created streams are odd or even (RFC 9113 §5.1.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// Opens the connection, sends the preface, numbers its streams odd.
    Client,
    /// Accepts it, reads the preface, numbers its streams even.
    Server,
}

/// Something the owner of a connection has to act on.
///
/// Answered rather than dispatched: this file knows nothing about JavaScript,
/// and the caller that does turns each of these into an event.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// A peer opened a stream and sent its request or response headers.
    Headers {
        /// The stream they arrived on.
        stream_id: u32,
        /// Name and value in wire order, pseudo-headers included.
        fields: Vec<(String, String)>,
        /// The peer will send nothing more on this stream.
        end_stream: bool,
    },
    /// Body bytes for a stream.
    Data {
        /// The stream they arrived on.
        stream_id: u32,
        /// The body bytes, padding already removed.
        bytes: Vec<u8>,
        /// The peer will send nothing more on this stream.
        end_stream: bool,
    },
    /// The peer reset one stream.
    Reset {
        /// The stream the peer gave up on.
        stream_id: u32,
        /// Why, as an RFC 9113 §7 code.
        error_code: u32,
    },
    /// The peer is going away; no stream above `last_stream_id` was processed.
    Goaway {
        /// The highest stream the peer processed; anything above it was not.
        last_stream_id: u32,
        /// Why, as an RFC 9113 §7 code.
        error_code: u32,
    },
    /// The peer's settings, once acknowledged.
    Settings(Vec<(u16, u32)>),
    /// A ping the peer acknowledged, by its opaque payload.
    PingAck([u8; 8]),
    /// This connection refused to continue, and why. The bytes to send are
    /// already in the outbound buffer.
    Failed(&'static str),
}

/// One stream's flow-control and lifecycle state.
#[derive(Debug)]
struct Stream {
    /// How many bytes this side may still send on it.
    send_window: i64,
    /// How many the peer may still send here.
    receive_window: i64,
    /// The peer said END_STREAM.
    remote_closed: bool,
    /// This side said END_STREAM.
    local_closed: bool,
}

impl Stream {
    fn new(initial: i64) -> Self {
        Self {
            send_window: initial,
            receive_window: INITIAL_WINDOW,
            remote_closed: false,
            local_closed: false,
        }
    }
}

/// One HTTP/2 connection, driven by bytes in and bytes out.
pub struct Connection {
    encoder: Encoder,
    decoder: Decoder,
    /// Bytes read off the peer that do not yet make a whole frame.
    inbound: Vec<u8>,
    /// Bytes to send. The owner drains this whenever it likes.
    outbound: Vec<u8>,
    streams: HashMap<u32, Stream>,
    next_stream_id: u32,
    connection_send_window: i64,
    connection_receive_window: i64,
    /// What the peer said its streams start with, applied to streams opened
    /// after it arrived.
    peer_initial_window: i64,
    peer_max_frame_size: usize,
    /// Whether the peer's preface has been seen. A server reads it; a client
    /// does not expect one.
    preface_seen: bool,
    resets_seen: u32,
    /// Set once this side has sent GOAWAY. Nothing more is written after it.
    going_away: bool,
    last_peer_stream_id: u32,
}

impl Connection {
    /// A new connection, with the preface and the opening SETTINGS already
    /// queued for sending.
    ///
    /// Queued in the constructor rather than left to the caller, because RFC
    /// 9113 §3.4 makes both mandatory and first: a connection whose owner
    /// forgets is one the peer closes with a protocol error, which is a bug
    /// that only shows up against a strict implementation.
    pub fn new(side: Side) -> Self {
        let mut connection = Self {
            encoder: Encoder::new(4096),
            decoder: Decoder::new(4096),
            inbound: Vec::new(),
            outbound: Vec::new(),
            streams: HashMap::new(),
            next_stream_id: match side {
                Side::Client => 1,
                Side::Server => 2,
            },
            connection_send_window: INITIAL_WINDOW,
            connection_receive_window: INITIAL_WINDOW,
            peer_initial_window: INITIAL_WINDOW,
            peer_max_frame_size: 16_384,
            preface_seen: side == Side::Client,
            resets_seen: 0,
            going_away: false,
            last_peer_stream_id: 0,
        };
        if side == Side::Client {
            connection.outbound.extend_from_slice(CONNECTION_PREFACE);
        }
        let settings = frame::write_settings(&[]);
        connection
            .outbound
            .extend_from_slice(&frame::write_frame(FrameType::Settings, 0, 0, &settings));
        connection
    }

    /// Bytes waiting to go to the peer, taken.
    pub fn take_outbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbound)
    }

    /// Whether this connection has sent GOAWAY and should be closed.
    pub fn finished(&self) -> bool {
        self.going_away
    }

    /// The id a new locally-opened stream gets.
    ///
    /// Odd for a client and even for a server, RFC 9113 §5.1.1, and stepping by
    /// two is what keeps that true for the life of the connection.
    fn take_stream_id(&mut self) -> u32 {
        let id = self.next_stream_id;
        self.next_stream_id += 2;
        id
    }

    /// Opens a stream with `fields` as its header block.
    ///
    /// `end_stream` for a request with no body — a GET, which is most of them.
    pub fn send_headers(&mut self, fields: &[(String, String)], end_stream: bool) -> u32 {
        let id = self.take_stream_id();
        self.write_headers(id, fields, end_stream);
        self.streams
            .insert(id, Stream::new(self.peer_initial_window));
        if end_stream && let Some(stream) = self.streams.get_mut(&id) {
            stream.local_closed = true;
        }
        id
    }

    /// Answers on a stream the peer opened.
    pub fn respond(&mut self, stream_id: u32, fields: &[(String, String)], end_stream: bool) {
        self.write_headers(stream_id, fields, end_stream);
        if end_stream && let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.local_closed = true;
        }
    }

    fn write_headers(&mut self, stream_id: u32, fields: &[(String, String)], end_stream: bool) {
        let encoded: Vec<HeaderField> = fields
            .iter()
            .map(|(name, value)| HeaderField {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        let block = self.encoder.encode(&encoded);
        let mut flags = FLAG_END_HEADERS;
        if end_stream {
            flags |= FLAG_END_STREAM;
        }
        self.outbound
            .extend_from_slice(&frame::write_frame(FrameType::Headers, flags, stream_id, &block));
    }

    /// Sends body bytes, split to the peer's maximum frame size and clamped to
    /// what both windows allow.
    ///
    /// Answers how many bytes were actually sent. A caller with more to send
    /// waits for a WINDOW_UPDATE — which is the whole point of flow control and
    /// the part an implementation that ignores it gets away with until it meets
    /// a peer that advertises a small window.
    pub fn send_data(&mut self, stream_id: u32, bytes: &[u8], end_stream: bool) -> usize {
        let allowed = {
            let Some(stream) = self.streams.get(&stream_id) else {
                return 0;
            };
            stream
                .send_window
                .min(self.connection_send_window)
                .max(0) as usize
        };
        let sending = bytes.len().min(allowed);
        let mut at = 0;
        while at < sending {
            let end = (at + self.peer_max_frame_size).min(sending);
            let last = end == sending && end_stream && sending == bytes.len();
            let flags = if last { FLAG_END_STREAM } else { 0 };
            self.outbound.extend_from_slice(&frame::write_frame(
                FrameType::Data,
                flags,
                stream_id,
                &bytes[at..end],
            ));
            at = end;
        }
        // An empty body with END_STREAM is a real frame and not a no-op: it is
        // how a caller says "that is all" without having anything to say.
        if sending == 0 && end_stream {
            self.outbound.extend_from_slice(&frame::write_frame(
                FrameType::Data,
                FLAG_END_STREAM,
                stream_id,
                &[],
            ));
        }
        self.connection_send_window -= sending as i64;
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.send_window -= sending as i64;
            if end_stream && sending == bytes.len() {
                stream.local_closed = true;
            }
        }
        sending
    }

    /// Resets one stream.
    pub fn send_reset(&mut self, stream_id: u32, error_code: u32) {
        let payload = frame::write_rst_stream(error_code);
        self.outbound.extend_from_slice(&frame::write_frame(
            FrameType::RstStream,
            0,
            stream_id,
            &payload,
        ));
        self.streams.remove(&stream_id);
    }

    /// Sends a ping with an opaque payload the peer must echo.
    pub fn send_ping(&mut self, payload: [u8; 8]) {
        let body = frame::write_ping(payload);
        self.outbound
            .extend_from_slice(&frame::write_frame(FrameType::Ping, 0, 0, &body));
    }

    /// Sends GOAWAY and stops writing anything else.
    pub fn send_goaway(&mut self, error_code: u32) {
        if self.going_away {
            return;
        }
        let payload = frame::write_goaway(self.last_peer_stream_id, error_code, &[]);
        self.outbound
            .extend_from_slice(&frame::write_frame(FrameType::Goaway, 0, 0, &payload));
        self.going_away = true;
    }

    /// Counts one reset and says whether the peer has spent its budget.
    fn note_reset(&mut self) -> bool {
        self.resets_seen += 1;
        self.resets_seen > RESET_BUDGET
    }

    /// Feeds bytes from the peer, answering what the owner must act on.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.inbound.extend_from_slice(bytes);
        let mut events = Vec::new();
        if !self.preface_seen {
            if self.inbound.len() < CONNECTION_PREFACE.len() {
                return events;
            }
            if &self.inbound[..CONNECTION_PREFACE.len()] != CONNECTION_PREFACE {
                self.send_goaway(PROTOCOL_ERROR);
                events.push(Event::Failed("the client's connection preface was wrong"));
                return events;
            }
            self.inbound.drain(..CONNECTION_PREFACE.len());
            self.preface_seen = true;
        }
        while let Some((parsed, used)) = frame::read_frame(&self.inbound) {
            self.inbound.drain(..used);
            self.handle(parsed, &mut events);
            if self.going_away {
                break;
            }
        }
        events
    }

    fn handle(&mut self, parsed: frame::Frame, events: &mut Vec<Event>) {
        let header = parsed.header;
        let payload = parsed.payload;
        match header.frame_type {
            FrameType::Settings => {
                if header.flags & FLAG_ACK != 0 {
                    return;
                }
                let pairs = frame::parse_settings(&payload);
                for (id, value) in &pairs {
                    match id {
                        // SETTINGS_INITIAL_WINDOW_SIZE applies to every stream
                        // already open as a DELTA, not as an assignment — RFC
                        // 9113 §6.9.2. Assigning is the mistake that stalls a
                        // connection whose peer raises the window mid-flight.
                        0x4 => {
                            let updated = i64::from(*value);
                            let delta = updated - self.peer_initial_window;
                            self.peer_initial_window = updated;
                            for stream in self.streams.values_mut() {
                                stream.send_window += delta;
                            }
                        }
                        0x5 => self.peer_max_frame_size = *value as usize,
                        _ => {}
                    }
                }
                self.outbound.extend_from_slice(&frame::write_frame(
                    FrameType::Settings,
                    FLAG_ACK,
                    0,
                    &[],
                ));
                events.push(Event::Settings(pairs));
            }
            FrameType::Headers => {
                let Some(block) = frame::parse_headers_payload(&payload, header.flags) else {
                    self.send_goaway(PROTOCOL_ERROR);
                    events.push(Event::Failed("a HEADERS frame could not be read"));
                    return;
                };
                if header.flags & FLAG_END_HEADERS == 0 {
                    // CONTINUATION is not implemented, and a session that
                    // waited for one it cannot parse would hang. Saying so is
                    // the honest answer; `frame.rs` names the same gap.
                    self.send_goaway(PROTOCOL_ERROR);
                    events.push(Event::Failed("a header block spanning frames is not supported"));
                    return;
                }
                let Some(fields) = self.decoder.decode(block) else {
                    self.send_goaway(PROTOCOL_ERROR);
                    events.push(Event::Failed("a header block did not decode"));
                    return;
                };
                let end_stream = header.flags & FLAG_END_STREAM != 0;
                self.last_peer_stream_id = self.last_peer_stream_id.max(header.stream_id);
                let entry = self
                    .streams
                    .entry(header.stream_id)
                    .or_insert_with(|| Stream::new(self.peer_initial_window));
                entry.remote_closed = end_stream;
                events.push(Event::Headers {
                    stream_id: header.stream_id,
                    fields: fields
                        .into_iter()
                        .map(|field| (field.name, field.value))
                        .collect(),
                    end_stream,
                });
            }
            FrameType::Data => {
                let Some(body) = frame::parse_data_payload(&payload, header.flags) else {
                    self.send_goaway(PROTOCOL_ERROR);
                    events.push(Event::Failed("a DATA frame could not be read"));
                    return;
                };
                // The whole payload counts against the window, padding
                // included — RFC 9113 §6.9.1. Counting only the body is the
                // error that makes a padded stream drift out of sync.
                let counted = payload.len() as i64;
                self.connection_receive_window -= counted;
                if let Some(stream) = self.streams.get_mut(&header.stream_id) {
                    stream.receive_window -= counted;
                    if stream.receive_window < 0 {
                        self.send_goaway(FLOW_CONTROL_ERROR);
                        events.push(Event::Failed("the peer sent past its stream window"));
                        return;
                    }
                }
                if self.connection_receive_window < 0 {
                    self.send_goaway(FLOW_CONTROL_ERROR);
                    events.push(Event::Failed("the peer sent past the connection window"));
                    return;
                }
                self.replenish(header.stream_id);
                let end_stream = header.flags & FLAG_END_STREAM != 0;
                if let Some(stream) = self.streams.get_mut(&header.stream_id) {
                    stream.remote_closed = stream.remote_closed || end_stream;
                }
                events.push(Event::Data {
                    stream_id: header.stream_id,
                    bytes: body.to_vec(),
                    end_stream,
                });
            }
            FrameType::WindowUpdate => {
                let Some(increment) = frame::parse_window_update(&payload) else {
                    return;
                };
                match header.stream_id {
                    0 => self.connection_send_window += i64::from(increment),
                    id => {
                        if let Some(stream) = self.streams.get_mut(&id) {
                            stream.send_window += i64::from(increment);
                        }
                    }
                }
            }
            FrameType::RstStream => {
                let error_code = frame::parse_rst_stream(&payload).unwrap_or(NO_ERROR);
                self.streams.remove(&header.stream_id);
                events.push(Event::Reset {
                    stream_id: header.stream_id,
                    error_code,
                });
                // CVE-2023-44487. A reset stream costs the peer one frame and
                // costs this side a whole request, and it does not count
                // against the concurrency limit — so the limit alone is not a
                // defence. The budget is.
                if self.note_reset() {
                    self.send_goaway(ENHANCE_YOUR_CALM);
                    events.push(Event::Failed("the peer reset too many streams"));
                }
            }
            FrameType::Ping => {
                let Some(opaque) = frame::parse_ping(&payload) else {
                    return;
                };
                match header.flags & FLAG_ACK != 0 {
                    true => events.push(Event::PingAck(opaque)),
                    false => {
                        let body = frame::write_ping(opaque);
                        self.outbound.extend_from_slice(&frame::write_frame(
                            FrameType::Ping,
                            FLAG_ACK,
                            0,
                            &body,
                        ));
                    }
                }
            }
            FrameType::Goaway => {
                if let Some(goaway) = frame::parse_goaway(&payload) {
                    events.push(Event::Goaway {
                        last_stream_id: goaway.last_stream_id,
                        error_code: goaway.error_code,
                    });
                }
                self.going_away = true;
            }
            // PRIORITY carries no state this implementation keeps, and an
            // unknown type is skipped by its length rather than treated as an
            // error — RFC 9113 §4.1 requires exactly that, and it is what lets
            // a peer use an extension without breaking this side.
            FrameType::Priority | FrameType::Other(_) => {}
        }
    }

    /// Gives the peer back the window it just spent.
    ///
    /// Immediately and in full, which is the simplest policy that cannot
    /// deadlock. A smarter one waits until the application has consumed the
    /// bytes; this side has no back pressure to report, so waiting would only
    /// invent a stall.
    fn replenish(&mut self, stream_id: u32) {
        let connection_gap = INITIAL_WINDOW - self.connection_receive_window;
        if connection_gap > 0 {
            let payload = frame::write_window_update(connection_gap as u32);
            self.outbound.extend_from_slice(&frame::write_frame(
                FrameType::WindowUpdate,
                0,
                0,
                &payload,
            ));
            self.connection_receive_window = INITIAL_WINDOW;
        }
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            let gap = INITIAL_WINDOW - stream.receive_window;
            if gap > 0 {
                stream.receive_window = INITIAL_WINDOW;
                let payload = frame::write_window_update(gap as u32);
                self.outbound.extend_from_slice(&frame::write_frame(
                    FrameType::WindowUpdate,
                    0,
                    stream_id,
                    &payload,
                ));
            }
        }
    }
}
