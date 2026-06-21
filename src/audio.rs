//! Click-sound playback. A dedicated thread owns the rodio output (which is `!Send` and stops
//! audio when dropped); everyone else holds a cloneable `AudioHandle` that sends play requests
//! over a channel. Fire-and-forget — the mixer sums voices, so rapid clicks overlap cleanly.

use rodio::{Decoder, DeviceSinkBuilder, Source};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

static DEFAULT_WAV: &[u8] = include_bytes!("../assets/sounds/default.wav");

#[derive(Clone, Copy)]
pub struct PlayParams {
    pub volume: f32, // 0.0..=1.0
    pub speed: f32,  // 1.0 = normal pitch/rate
}

enum AudioMsg {
    Play(PlayParams),
    SetBytes(Arc<[u8]>),
    SetDefault,
    Preview,
}

#[derive(Clone)]
pub struct AudioHandle {
    tx: Sender<AudioMsg>,
}

impl AudioHandle {
    /// Spawn the audio thread. Returns `None` if no output device is available.
    pub fn spawn() -> Option<AudioHandle> {
        let (tx, rx) = channel::<AudioMsg>();
        let (ready_tx, ready_rx) = channel::<bool>();
        thread::Builder::new()
            .name("citron-audio".into())
            .spawn(move || audio_thread(rx, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(true) => Some(AudioHandle { tx }),
            _ => None,
        }
    }

    pub fn play(&self, params: PlayParams) {
        let _ = self.tx.send(AudioMsg::Play(params));
    }
    pub fn set_default(&self) {
        let _ = self.tx.send(AudioMsg::SetDefault);
    }
    pub fn set_custom(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(AudioMsg::SetBytes(bytes.into()));
    }
    pub fn preview(&self) {
        let _ = self.tx.send(AudioMsg::Preview);
    }
}

fn default_bytes() -> Arc<[u8]> {
    Arc::from(DEFAULT_WAV.to_vec().into_boxed_slice())
}

fn audio_thread(rx: Receiver<AudioMsg>, ready_tx: Sender<bool>) {
    // MixerDeviceSink owns the cpal stream — it is !Send and must live here for the whole loop.
    let handle = match DeviceSinkBuilder::open_default_sink() {
        Ok(mut h) => {
            h.log_on_drop(false);
            let _ = ready_tx.send(true);
            h
        }
        Err(_) => {
            let _ = ready_tx.send(false);
            return;
        }
    };
    let mixer = handle.mixer().clone();
    let mut bytes: Arc<[u8]> = default_bytes();
    while let Ok(msg) = rx.recv() {
        match msg {
            AudioMsg::SetDefault => bytes = default_bytes(),
            AudioMsg::SetBytes(b) => bytes = b,
            AudioMsg::Preview => play_once(&mixer, &bytes, PlayParams { volume: 1.0, speed: 1.0 }),
            AudioMsg::Play(p) => play_once(&mixer, &bytes, p),
        }
    }
    // `handle` drops here when all senders are gone → stream stops cleanly.
}

fn play_once(mixer: &rodio::mixer::Mixer, bytes: &Arc<[u8]>, p: PlayParams) {
    let cursor = Cursor::new(Arc::clone(bytes));
    let src = match Decoder::try_from(cursor) {
        Ok(d) => d,
        Err(_) => return,
    };
    mixer.add(src.speed(p.speed.max(0.1)).amplify(p.volume.clamp(0.0, 1.0)));
}

/// Decodable-WAV check used before accepting a custom file (off the hot path).
pub fn validate_wav(bytes: &[u8]) -> Result<(), ()> {
    Decoder::try_from(Cursor::new(bytes.to_vec()))
        .map(|_| ())
        .map_err(|_| ())
}
