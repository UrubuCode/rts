//! `audio` namespace — ABI registration.
//!
//! Primitivo cru de I/O de áudio (placa de som virtual sobre cpal). Toda a
//! lógica de alto nível (oscilador, mixer, efeitos, decode) é TS sobre estas
//! funções. Ver `docs/specs/audio.md`.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "default_sample_rate",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE",
        args: &[],
        returns: AbiType::I64,
        doc: "Sample rate (Hz) of the default output device, or 0 if unavailable.",
        ts_signature: "default_sample_rate(): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "default_channels",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_DEFAULT_CHANNELS",
        args: &[],
        returns: AbiType::I64,
        doc: "Channel count of the default output device, or 0 if unavailable.",
        ts_signature: "default_channels(): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "open_output",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_OPEN_OUTPUT",
        args: &[AbiType::I64, AbiType::I64, AbiType::I64],
        returns: AbiType::Handle,
        doc: "Opens an output stream and starts playing (silence until first write). \
               sample_rate/channels = 0 use the device default; capacity_frames = 0 \
               uses ~500ms. Returns the stream handle (>0) or 0 on failure.",
        ts_signature:
            "open_output(sample_rate: number, channels: number, capacity_frames: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "sample_rate",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_SAMPLE_RATE",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Effective sample rate (Hz) of the stream, or 0 if the handle is invalid.",
        ts_signature: "sample_rate(handle: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "channels",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_CHANNELS",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Effective channel count of the stream, or 0 if the handle is invalid.",
        ts_signature: "channels(handle: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "is_open",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_IS_OPEN",
        args: &[AbiType::U64],
        returns: AbiType::Bool,
        doc: "Whether the stream is still open (1) or not (0).",
        ts_signature: "is_open(handle: number): boolean",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "available_frames",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_AVAILABLE_FRAMES",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Free frames in the ring buffer right now (room to write without \
               dropping). Use for backpressure: generate at most this many frames. \
               -1 if the handle is invalid.",
        ts_signature: "available_frames(handle: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "queued_frames",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_QUEUED_FRAMES",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Frames currently queued in the ring (awaiting playback). -1 if invalid.",
        ts_signature: "queued_frames(handle: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "write",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_WRITE",
        args: &[AbiType::U64, AbiType::U64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Pushes interleaved f32 samples from a buffer into the stream's ring. \
               buffer_handle must hold n_samples*4 bytes of little-endian f32. \
               Respects backpressure (never exceeds capacity_frames). Returns the \
               number of samples actually accepted (may be < n_samples); resend the \
               rest. 0 on invalid handle.",
        ts_signature: "write(handle: number, buffer_handle: number, n_samples: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "master_volume",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_MASTER_VOLUME",
        args: &[AbiType::U64, AbiType::F64],
        returns: AbiType::Void,
        doc: "Sets the stream master gain, multiplied into every sample in the \
               callback. Clamped to [0, 4].",
        ts_signature: "master_volume(handle: number, gain: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "underruns",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_UNDERRUNS",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Total underruns (device asked for samples the ring could not supply) \
               since open. Diagnostic for starvation. -1 if invalid.",
        ts_signature: "underruns(handle: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "close",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_AUDIO_CLOSE",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Closes the stream and releases the device. Repeated calls are a no-op.",
        ts_signature: "close(handle: number): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "audio",
    doc: "Low-level audio device I/O (raw output streams + ring buffer). \
          High-level synthesis/mixing/effects live in TS on top of this.",
    members: MEMBERS,
};
