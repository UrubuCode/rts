// Sonda o backend ASIO: disponível? qual device/config?
import { asio_audio, io } from "rts";

io.print("ASIO disponível? " + asio_audio.is_available());
io.print("ASIO default sample rate: " + asio_audio.default_sample_rate());
io.print("ASIO default channels: " + asio_audio.default_channels());
