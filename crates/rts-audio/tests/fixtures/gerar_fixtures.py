# Gera as fixtures OGG/Vorbis dos testes do rts-audio. Rodar UMA vez e versionar
# os .ogg: os testes não dependem de Python.
#   python -m pip install soundfile numpy
#   python crates/rts-audio/tests/fixtures/gerar_fixtures.py
import pathlib
import numpy as np
import soundfile as sf

AQUI = pathlib.Path(__file__).parent

# 0,25 s de seno de 440 Hz, amplitude 0,5, mono, 22 050 Hz.
t = np.arange(int(22050 * 0.25)) / 22050.0
mono = (0.5 * np.sin(2.0 * np.pi * 440.0 * t)).astype(np.float32)
sf.write(AQUI / "seno440_mono_22050.ogg", mono, 22050, format="OGG", subtype="VORBIS")

# 0,5 s estéreo a 44 100 Hz: esquerda 440 Hz em 0,5, direita 880 Hz em 0,25 —
# os picos diferentes dizem se a ordem dos canais saiu certa.
t2 = np.arange(int(44100 * 0.5)) / 44100.0
esq = 0.5 * np.sin(2.0 * np.pi * 440.0 * t2)
dir_ = 0.25 * np.sin(2.0 * np.pi * 880.0 * t2)
sf.write(AQUI / "estereo_44100.ogg", np.stack([esq, dir_], axis=1).astype(np.float32), 44100,
         format="OGG", subtype="VORBIS")
print("ok", sorted(p.name for p in AQUI.glob("*.ogg")))
