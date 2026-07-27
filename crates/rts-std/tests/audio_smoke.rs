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
    // Os símbolos ABI seguem a convenção derivada pelo `#[rtse::function]`
    // (`__rtsm_<module>_<value>`), não mais o `__RTS_FN_NS_*` hand-written.
    let sr = audio::__rtsm_audio_sample_rate(h);
    let ch = audio::__rtsm_audio_channels(h);
    eprintln!("sr={sr} ch={ch}");
    assert!(sr > 0 && ch > 0);
    audio::__rtsm_audio_close(h);
    eprintln!("closed ok");
}
