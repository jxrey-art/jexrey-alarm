//! Audio playback engine.
//!
//! Requirement: when an alarm fires its sound must loop **forever** until
//! the user presses STOP — never stop after N repeats or a timeout.
//!
//! rodio's `Decoder` can't be cheaply cloned, which is what `repeat_infinite`
//! needs, so instead of trying to re-open/re-seek the file every loop (slow,
//! and one more way a flaky file path could interrupt the loop) we decode
//! the whole clip into an in-memory sample buffer *once*, wrap it in a
//! cheap-to-clone `SamplesBuffer`, and loop that buffer forever. A few
//! seconds of decoded PCM audio is a trivial amount of RAM, and looping a
//! buffer already sitting in memory can never "stall" waiting on disk I/O.
//!
//! Every alarm gets its own `Sink`, so several alarms can legitimately ring
//! at once (see `alarm.rs` / scheduler docs) without stepping on each
//! other's audio.

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// What actually ended up playing for a given trigger — surfaced back to
/// the caller so the event log / UI can be honest about it (robustness
/// requirement: never silently do something other than what was asked).
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackKind {
    File(String),
    FallbackBeep(String),
}

pub struct AudioEngine {
    // Kept alive for the whole app lifetime: dropping `OutputStream` tears
    // down the audio device. Never read directly after construction.
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sinks: HashMap<u64, Sink>,
}

impl AudioEngine {
    pub fn new() -> Self {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Self {
                _stream: Some(stream),
                handle: Some(handle),
                sinks: HashMap::new(),
            },
            Err(_) => Self {
                _stream: None,
                handle: None,
                sinks: HashMap::new(),
            },
        }
    }

    pub fn is_available(&self) -> bool {
        self.handle.is_some()
    }

    /// True if at least one alarm's sound is currently looping.
    pub fn any_playing(&self) -> bool {
        !self.sinks.is_empty()
    }

    #[allow(dead_code)]
    pub fn is_playing(&self, alarm_id: u64) -> bool {
        self.sinks.contains_key(&alarm_id)
    }

    pub fn playing_count(&self) -> usize {
        self.sinks.len()
    }

    /// Start looping the given alarm's sound forever. If `sound_path` is
    /// missing, unreadable, or not a decodable audio file, falls back to a
    /// synthesized beep pattern rather than leaving the alarm silent.
    pub fn play_loop(
        &mut self,
        alarm_id: u64,
        sound_path: Option<&str>,
    ) -> Result<PlaybackKind, String> {
        // Stop any previous loop for this same id first (defensive: avoids
        // ever stacking two sinks for one alarm).
        self.stop(alarm_id);

        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| "no audio output device available".to_string())?;

        let sink = Sink::try_new(handle).map_err(|e| format!("could not open sink: {e}"))?;

        if let Some(path) = sound_path {
            match load_looping_buffer(path) {
                Ok(buffer) => {
                    sink.append(buffer.repeat_infinite());
                    sink.play();
                    self.sinks.insert(alarm_id, sink);
                    return Ok(PlaybackKind::File(path.to_string()));
                }
                Err(reason) => {
                    // Fall through to the beep fallback below, but remember why.
                    let beep = beep_buffer();
                    sink.append(beep.repeat_infinite());
                    sink.play();
                    self.sinks.insert(alarm_id, sink);
                    return Ok(PlaybackKind::FallbackBeep(reason));
                }
            }
        }

        let beep = beep_buffer();
        sink.append(beep.repeat_infinite());
        sink.play();
        self.sinks.insert(alarm_id, sink);
        Ok(PlaybackKind::FallbackBeep("no sound assigned".to_string()))
    }

    /// Stop a single alarm's loop immediately.
    pub fn stop(&mut self, alarm_id: u64) {
        if let Some(sink) = self.sinks.remove(&alarm_id) {
            sink.stop();
        }
    }

    /// Stop every currently-ringing alarm immediately.
    pub fn stop_all(&mut self) {
        for (_, sink) in self.sinks.drain() {
            sink.stop();
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode an entire audio file (WAV / MP3 / OGG Vorbis / FLAC — anything
/// Symphonia recognises) into a cloneable in-memory sample buffer.
fn load_looping_buffer(path: &str) -> Result<SamplesBuffer<f32>, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(format!("sound file not found: {path}"));
    }
    let file = File::open(p).map_err(|e| format!("cannot open sound file: {e}"))?;
    let reader = BufReader::new(file);
    let decoder =
        Decoder::new(reader).map_err(|e| format!("unsupported or corrupt audio file: {e}"))?;

    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

    if samples.is_empty() {
        return Err("decoded audio file contained no samples".to_string());
    }

    Ok(SamplesBuffer::new(channels, sample_rate, samples))
}

/// A short synthesized two-tone chirp followed by silence, packaged as a
/// buffer meant to be looped with `repeat_infinite()`. Used whenever no
/// sound file is assigned, or the assigned file can't be played, so an
/// alarm can never fail to make noise.
fn beep_buffer() -> SamplesBuffer<f32> {
    const SAMPLE_RATE: u32 = 44_100;
    const TONE_HZ_A: f32 = 880.0;
    const TONE_HZ_B: f32 = 1108.0;
    const TONE_SECS: f32 = 0.18;
    const GAP_SECS: f32 = 0.12;
    const PAUSE_SECS: f32 = 0.35;

    let mut samples = Vec::new();
    push_tone(&mut samples, SAMPLE_RATE, TONE_HZ_A, TONE_SECS);
    push_silence(&mut samples, SAMPLE_RATE, GAP_SECS);
    push_tone(&mut samples, SAMPLE_RATE, TONE_HZ_B, TONE_SECS);
    push_silence(&mut samples, SAMPLE_RATE, PAUSE_SECS);

    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

fn push_tone(samples: &mut Vec<f32>, sample_rate: u32, freq_hz: f32, secs: f32) {
    let n = (sample_rate as f32 * secs) as usize;
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        // Gentle attack/decay envelope so the beep doesn't click.
        let envelope = ((std::f32::consts::PI * t / secs).sin()).abs();
        let v = (2.0 * std::f32::consts::PI * freq_hz * t).sin() * 0.5 * envelope;
        samples.push(v);
    }
}

fn push_silence(samples: &mut Vec<f32>, sample_rate: u32, secs: f32) {
    let n = (sample_rate as f32 * secs) as usize;
    samples.extend(std::iter::repeat(0.0f32).take(n));
}
