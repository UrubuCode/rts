//! Web Streams — ReadableStream / TransformStream family (9 classes). Migrado ao
//! modelo `#[rts_class]` (stage 5) via membros `external` — os externs
//! `__RTS_FN_GL_*STREAM*` ficam em `instance.rs` intactos; o macro deriva apenas
//! os 9 `*_CLASS_SPEC`. Os getters `writable`/`readable` compartilham os externs
//! `__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE/READABLE` entre 4 classes.

pub mod instance;

#[allow(unused_imports)]
use rts_engine::abi::ty::Handle;
use rts_macro::rts_class;

/// ReadableStream.
#[rts_class(ReadableStream, spec = "READABLE_STREAM_CLASS_SPEC")]
impl ReadableStreamClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_READABLE_STREAM_NEW",
        ts = "new ReadableStream(underlyingSource?: object): ReadableStream"
    )]
    pub fn new(_src: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "getReader",
        symbol = "__RTS_FN_GL_READABLE_STREAM_GET_READER",
        ts = "getReader(): ReadableStreamDefaultReader"
    )]
    pub fn get_reader(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "pipeThrough",
        symbol = "__RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH",
        ts = "pipeThrough(t: { writable: WritableStream; readable: ReadableStream }): ReadableStream"
    )]
    pub fn pipe_through(_h: Handle, _t: Handle) -> Handle {
        unreachable!()
    }
}

/// TextEncoderStream (identity passthrough).
#[rts_class(TextEncoderStream, spec = "TEXT_ENCODER_STREAM_CLASS_SPEC")]
impl TextEncoderStreamClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_TEXT_ENCODER_STREAM_NEW",
        ts = "new TextEncoderStream(): TextEncoderStream"
    )]
    pub fn new(_x: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "writable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        ts = "readonly writable: WritableStream"
    )]
    pub fn writable(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "readable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        ts = "readonly readable: ReadableStream"
    )]
    pub fn readable(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// TextDecoderStream (identity passthrough).
#[rts_class(TextDecoderStream, spec = "TEXT_DECODER_STREAM_CLASS_SPEC")]
impl TextDecoderStreamClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_TEXT_DECODER_STREAM_NEW",
        ts = "new TextDecoderStream(): TextDecoderStream"
    )]
    pub fn new(_x: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "writable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        ts = "readonly writable: WritableStream"
    )]
    pub fn writable(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "readable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        ts = "readonly readable: ReadableStream"
    )]
    pub fn readable(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// CompressionStream — gzip/deflate.
#[rts_class(CompressionStream, spec = "COMPRESSION_STREAM_CLASS_SPEC")]
impl CompressionStreamClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_COMPRESSION_STREAM_NEW",
        ts = "new CompressionStream(format: string): CompressionStream"
    )]
    pub fn new(_format: Str) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "writable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        ts = "readonly writable: WritableStream"
    )]
    pub fn writable(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "readable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        ts = "readonly readable: ReadableStream"
    )]
    pub fn readable(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// ReadableStreamDefaultReader.
#[rts_class(ReadableStreamDefaultReader, spec = "READER_CLASS_SPEC")]
impl ReadableStreamDefaultReaderClass {
    #[rts_method(
        external,
        name = "read",
        symbol = "__RTS_FN_GL_READABLE_STREAM_READER_READ",
        ts = "read(): Promise<{value: any; done: boolean}>"
    )]
    pub fn read(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// ReadableStreamDefaultController.
#[rts_class(ReadableStreamDefaultController, spec = "CONTROLLER_CLASS_SPEC")]
impl ReadableStreamDefaultControllerClass {
    #[rts_method(
        external,
        name = "enqueue",
        symbol = "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE",
        ts = "enqueue(chunk: any): void"
    )]
    pub fn enqueue(_h: Handle, _chunk: Handle) {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "close",
        symbol = "__RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE",
        ts = "close(): void"
    )]
    pub fn close(_h: Handle) {
        unreachable!()
    }
}

/// TransformStream.
#[rts_class(TransformStream, spec = "TRANSFORM_STREAM_CLASS_SPEC")]
impl TransformStreamClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_NEW",
        ts = "new TransformStream(transformer?: object): TransformStream"
    )]
    pub fn new(_t: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "writable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE",
        ts = "readonly writable: WritableStream"
    )]
    pub fn writable(_h: Handle) -> Handle {
        unreachable!()
    }
    #[rts_getter(
        external,
        name = "readable",
        symbol = "__RTS_FN_GL_TRANSFORM_STREAM_READABLE",
        ts = "readonly readable: ReadableStream"
    )]
    pub fn readable(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// WritableStream.
#[rts_class(WritableStream, spec = "WRITABLE_STREAM_CLASS_SPEC")]
impl WritableStreamClass {
    #[rts_method(
        external,
        name = "getWriter",
        symbol = "__RTS_FN_GL_WRITABLE_STREAM_GET_WRITER",
        ts = "getWriter(): WritableStreamDefaultWriter"
    )]
    pub fn get_writer(_h: Handle) -> Handle {
        unreachable!()
    }
}

/// WritableStreamDefaultWriter.
#[rts_class(WritableStreamDefaultWriter, spec = "WRITER_CLASS_SPEC")]
impl WritableStreamDefaultWriterClass {
    #[rts_method(
        external,
        name = "write",
        symbol = "__RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE",
        ts = "write(chunk: any): Promise<void>"
    )]
    pub fn write(_h: Handle, _chunk: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "close",
        symbol = "__RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE",
        ts = "close(): Promise<void>"
    )]
    pub fn close(_h: Handle) -> Handle {
        unreachable!()
    }
}
