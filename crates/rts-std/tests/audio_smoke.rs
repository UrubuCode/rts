//! Smoke do backend de áudio em Rust puro (sem o motor): isola se o SIGILL
//! observado no `rts run` está no runtime cpal ou no marshalling do engine.
#[test]
fn open_write_close() {
    use rts_std::audio;
    let h = audio::open_output(0, 0, 0);
    eprintln!("handle = {h}");
    if h == 0 {
        eprintln!("sem device de saída — pulando (não é falha do código)");
        return;
    }
    let sr = audio::__RTS_FN_NS_AUDIO_SAMPLE_RATE(h);
    let ch = audio::__RTS_FN_NS_AUDIO_CHANNELS(h);
    eprintln!("sr={sr} ch={ch}");
    assert!(sr > 0 && ch > 0);
    audio::__RTS_FN_NS_AUDIO_CLOSE(h);
    eprintln!("closed ok");
}
