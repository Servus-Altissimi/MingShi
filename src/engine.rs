#![allow(dead_code)]

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{StreamConfig, Stream};

use crate::error::SynthError;
use crate::instrument::{Instrument, InstrumentSource, SampleData, SequenceElement};
use crate::track::MelodyTrack;
use crate::arrangement::Arrangement;
use crate::effects::EffectsProcessor;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

pub struct DynamicParameters {
    pub master_volume: f32,
    pub master_pitch: f32,
    pub track_volumes: HashMap<String, f32>,
    pub track_enabled: HashMap<String, bool>,
    pub crossfade_duration: f32,
}

impl Default for DynamicParameters {
    fn default() -> Self {
        DynamicParameters {
            master_volume: 1.0,
            master_pitch: 1.0,
            track_volumes: HashMap::new(),
            track_enabled: HashMap::new(),
            crossfade_duration: 1.0,
        }
    }
}

struct PlaybackContext {
    arrangement: Arrangement,
    current_sample: usize,
    state: PlaybackState,
    loop_enabled: bool,
    dynamic_params: DynamicParameters,
    param_interpolators: HashMap<String, f32>,
    crossfade_state: Option<CrossfadeState>,
}

struct CrossfadeState {
    target_arrangement: Arrangement,
    progress: f32,
    duration_samples: usize,
}

pub struct SynthEngine {
    mel_cache: HashMap<String, MelodyTrack>,
    sample_cache: HashMap<String, SampleData>,
    stream_config: StreamConfig,
    pub sample_rate: f32,
    playback_context: Arc<Mutex<Option<PlaybackContext>>>,
    stream: Option<Stream>,
}

impl SynthEngine {
    pub fn new() -> Result<Self, SynthError> {
         
        // cpal has issues with devices on WASM 
        #[cfg(target_arch = "wasm32")]
        {
            return Ok(SynthEngine {
                mel_cache: HashMap::new(),
                sample_cache: HashMap::new(),
                stream_config: StreamConfig {
                    channels: 2,
                    sample_rate: 44100,
                    buffer_size: cpal::BufferSize::Default,
                },
                sample_rate: 44100.0,
                playback_context: Arc::new(Mutex::new(None)),
                stream: None,
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let host = cpal::default_host();
            let device = host.default_output_device()
                .ok_or_else(|| SynthError::AudioError("No output device found".to_string()));
            let config = device?.default_output_config()
                .map_err(|e| SynthError::AudioError(e.to_string()))?;
            let stream_config = config.config();

            Ok(SynthEngine {
                mel_cache: HashMap::new(),
                sample_cache: HashMap::new(),
                stream_config: stream_config.clone(),
                sample_rate: stream_config.sample_rate as f32,
                playback_context: Arc::new(Mutex::new(None)),
                stream: None,
            })
        }
    }

    // Used by GpuSynthEngine to delegate CPU-side work without requiring audio hardware, internal only.
    pub(crate) fn new_offline(sample_rate: f32) -> Self {
        SynthEngine {
            mel_cache:    HashMap::new(),
            sample_cache: HashMap::new(),
            stream_config: StreamConfig {
                channels:    2,
                sample_rate: sample_rate as u32,     
                buffer_size: cpal::BufferSize::Default,
            },
            sample_rate,
            playback_context: Arc::new(Mutex::new(None)),
            stream: None,
        }
    }

    pub fn get_sample_cache(&self) -> &HashMap<String, SampleData> {
        &self.sample_cache
    }

    pub fn load_sample(&mut self, name: &str, path: &str) -> Result<(), Box<dyn Error>> {
        let data = std::fs::read(path)?;
        let cursor = std::io::Cursor::new(data);
        let mut reader = hound::WavReader::new(cursor)?;
        let spec = reader.spec();
        let samples: Result<Vec<f32>, _> = reader.samples::<i16>()
            .map(|r| r.map(|s| s as f32 / 32768.0))
            .collect();
        let sample_data = SampleData {
            samples: Arc::new(samples?),
            sample_rate: spec.sample_rate,
        };
        self.sample_cache.insert(name.to_string(), sample_data);
        Ok(())
    }

    pub fn load_melody(&mut self, name: &str, path: &str) -> Result<(), Box<dyn Error>> {
        let content = std::fs::read_to_string(path)?;
        let track = MelodyTrack::from_mel(&content, &self.sample_cache)?;
        self.mel_cache.insert(name.to_string(), track);
        Ok(())
    }

    pub fn load_arrangement(&self, path: &str) -> Result<Arrangement, SynthError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SynthError::FileError(e.to_string()))?;
        Arrangement::from_bmi(&content, &self.mel_cache)
    }

    pub fn play_arrangement(&mut self, arrangement: Arrangement) -> Result<(), SynthError> {
        self.stop();
        let mut context = PlaybackContext {
            arrangement,
            current_sample: 0,
            state: PlaybackState::Playing,
            loop_enabled: false,
            dynamic_params: DynamicParameters::default(),
            param_interpolators: HashMap::new(),
            crossfade_state: None,
        };
        for (track, _, _) in &context.arrangement.tracks {
            context.dynamic_params.track_enabled.insert(track.name.clone(), true);
            context.dynamic_params.track_volumes.insert(track.name.clone(), 1.0);
        }
        *self.playback_context.lock().unwrap() = Some(context);
        self.start_stream()?;
        Ok(())
    }

    pub fn crossfade_to(&mut self, new_arrangement: Arrangement, duration: f32) -> Result<(), SynthError> {
        {
            let mut ctx_lock = self.playback_context.lock().unwrap();
            if let Some(ctx) = ctx_lock.as_mut() {
                ctx.crossfade_state = Some(CrossfadeState {
                    target_arrangement: new_arrangement,
                    progress: 0.0,
                    duration_samples: (duration * self.sample_rate) as usize,
                });
                return Ok(());
            }
        }
        self.play_arrangement(new_arrangement)?;
        Ok(())
    }

    pub fn set_loop_enabled(&self, enabled: bool) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            ctx.loop_enabled = enabled;
        }
    }

    pub fn pause(&self) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            if ctx.state == PlaybackState::Playing {
                ctx.state = PlaybackState::Paused;
            }
        }
    }

    pub fn resume(&self) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            if ctx.state == PlaybackState::Paused {
                ctx.state = PlaybackState::Playing;
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() { drop(stream); }
        *self.playback_context.lock().unwrap() = None;
    }

    pub fn set_master_volume(&self, volume: f32) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            ctx.dynamic_params.master_volume = volume.max(0.0).min(2.0);
        }
    }

    pub fn set_master_pitch(&self, pitch: f32) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            ctx.dynamic_params.master_pitch = pitch.max(0.5).min(2.0);
        }
    }

    pub fn set_track_enabled(&self, track_name: &str, enabled: bool) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            ctx.dynamic_params.track_enabled.insert(track_name.to_string(), enabled);
        }
    }

    pub fn set_track_volume(&self, track_name: &str, volume: f32) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            ctx.dynamic_params.track_volumes.insert(track_name.to_string(), volume.max(0.0).min(2.0));
        }
    }

    pub fn interpolate_track_volume(&self, track_name: &str, target: f32, duration: f32) {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_mut() {
            let key = format!("vol_{}", track_name);
            ctx.param_interpolators.insert(key, duration);
            ctx.dynamic_params.track_volumes.insert(track_name.to_string(), target);
        }
    }

    pub fn get_playback_position(&self) -> f32 {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_ref() {
            ctx.current_sample as f32 / self.sample_rate
        } else {
            0.0
        }
    }

    pub fn get_playback_state(&self) -> PlaybackState {
        if let Some(ctx) = self.playback_context.lock().unwrap().as_ref() {
            ctx.state
        } else {
            PlaybackState::Stopped
        }
    }

    fn start_stream(&mut self) -> Result<(), SynthError> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| SynthError::AudioError("No output device".to_string()))?;
        let config = self.stream_config.clone();
        let sample_rate = self.sample_rate;
        let ctx = Arc::clone(&self.playback_context);

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut context_lock = ctx.lock().unwrap();
                if let Some(context) = context_lock.as_mut() {
                    if context.state != PlaybackState::Playing {
                        for s in data.iter_mut() { *s = 0.0; }
                        return;
                    }
                    for frame in data.chunks_mut(config.channels as usize) {
                        let mut output = Self::synthesize_single_sample(
                            &context.arrangement,
                            context.current_sample,
                            sample_rate,
                            &context.dynamic_params,
                        );
                        if let Some(ref mut cf) = context.crossfade_state {
                            let t = cf.progress / cf.duration_samples as f32;
                            let tgt = Self::synthesize_single_sample(
                                &cf.target_arrangement,
                                context.current_sample,
                                sample_rate,
                                &context.dynamic_params,
                            );
                            output = output * (1.0 - t) + tgt * t;
                            cf.progress += 1.0;
                            if cf.progress >= cf.duration_samples as f32 {
                                context.arrangement = cf.target_arrangement.clone();
                                context.crossfade_state = None;
                            }
                        }
                        context.current_sample += 1;
                        if context.loop_enabled {
                            if let Some(ref lp) = context.arrangement.loop_point {
                                let pos = context.current_sample as f32 / sample_rate;
                                if pos >= lp.end {
                                    context.current_sample = (lp.start * sample_rate) as usize;
                                }
                            } else {
                                let total = (context.arrangement.total_length * sample_rate) as usize;
                                if context.current_sample >= total { context.current_sample = 0; }
                            }
                        } else {
                            let total = (context.arrangement.total_length * sample_rate) as usize;
                            if context.current_sample >= total { context.state = PlaybackState::Stopped; }
                        }
                        let cur_t = context.current_sample as f32 / sample_rate;
                        let tot   = context.arrangement.total_length;
                        let mut fade = 1.0f32;
                        if let Some(fi) = context.arrangement.fade_in  { if cur_t < fi { fade *= cur_t / fi; } }
                        if let Some(fo) = context.arrangement.fade_out { let fs = tot - fo; if cur_t > fs { fade *= (tot - cur_t) / fo; } }
                        let final_out = output * context.dynamic_params.master_volume * fade;
                        for s in frame.iter_mut() { *s = final_out; }
                    }
                } else {
                    for s in data.iter_mut() { *s = 0.0; }
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        ).map_err(|e| SynthError::AudioError(e.to_string()))?;

        stream.play().map_err(|e| SynthError::AudioError(e.to_string()))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn synthesize_single_sample(
        arrangement: &Arrangement,
        sample_idx: usize,
        sample_rate: f32,
        params: &DynamicParameters,
    ) -> f32 {
        let mut output = 0.0;
        let current_time = sample_idx as f32 / sample_rate;
        for (track, start_time, overrides) in &arrangement.tracks {
            let enabled = params.track_enabled.get(&track.name).copied().unwrap_or(true);
            if !enabled { continue; }
            let track_vol = params.track_volumes.get(&track.name).copied().unwrap_or(1.0);
            if current_time < *start_time { continue; }
            let track_time = current_time - start_time;
            let mut cum = 0.0;
            let beat_dur = 60.0 / track.tempo;
            for element in &track.sequence {
                match element {
                    SequenceElement::Note(note) => {
                        let nd = note.duration * beat_dur;
                        let next = cum + nd;
                        if track_time >= cum && track_time < next {
                            let t = track_time - cum;
                            let env = Self::calculate_envelope_static(t, nd, &track.instrument);
                            let mut pitch = note.pitch;
                            if let Some(st) = note.slide_to {
                                pitch = note.pitch * (1.0 - t / nd) + st * (t / nd);
                            }
                            let sample = match &track.instrument.source {
                                InstrumentSource::Synthesized(wf) => wf.generate_sample((track_time * pitch * params.master_pitch) % 1.0),
                                InstrumentSource::Sample(sd)      => Self::interpolate_sample(sd, t, track.instrument.pitch * params.master_pitch),
                            };
                            let vol = track.instrument.volume * overrides.volume.unwrap_or(1.0) * track_vol;
                            output += sample * env * note.velocity * vol;
                            break;
                        }
                        cum = next;
                    }
                    SequenceElement::Chord(chord) => {
                        let cd = chord.duration * beat_dur;
                        let next = cum + cd;
                        if track_time >= cum && track_time < next {
                            let t = track_time - cum;
                            let env = Self::calculate_envelope_static(t, cd, &track.instrument);
                            for pitch in &chord.pitches {
                                let sample = match &track.instrument.source {
                                    InstrumentSource::Synthesized(wf) => wf.generate_sample((track_time * pitch * params.master_pitch) % 1.0),
                                    InstrumentSource::Sample(sd)      => Self::interpolate_sample(sd, t, track.instrument.pitch * params.master_pitch),
                                };
                                let vol = track.instrument.volume * overrides.volume.unwrap_or(1.0) * track_vol;
                                output += sample * env * chord.velocity * vol / chord.pitches.len() as f32;
                            }
                            break;
                        }
                        cum = next;
                    }
                    SequenceElement::Rest(d) => { cum += d * beat_dur; }
                }
            }
        }
        output
    }

    pub fn synthesize_arrangement(&self, arrangement: &Arrangement) -> Result<Vec<f32>, SynthError> {
        self.synthesize_arrangement_private(arrangement, &DynamicParameters::default())
    }

	fn synthesize_arrangement_private(
        &self,
        arrangement: &Arrangement,
        params: &DynamicParameters,
    ) -> Result<Vec<f32>, SynthError> {
        let total_samples = (arrangement.total_length * self.sample_rate) as usize;
        let mut buffer = vec![0.0f32; total_samples];

        for (track, start_time, overrides) in &arrangement.tracks {
            let enabled = params.track_enabled.get(&track.name).copied().unwrap_or(true);
            if !enabled { continue; }
            let track_vol = params.track_volumes.get(&track.name).copied().unwrap_or(1.0);
            let start_sample = (start_time * self.sample_rate) as usize;
            let mut t = track.clone();
            if let Some(v) = overrides.volume      { t.instrument.volume = v; }
            if let Some(p) = overrides.pitch        { t.instrument.pitch  = p * params.master_pitch; }
            if let Some(tm) = overrides.tempo       { t.tempo = tm; }
            if let Some(r) = &overrides.reverb      { t.instrument.effects.reverb     = Some(r.clone()); }
            if let Some(d) = &overrides.delay       { t.instrument.effects.delay      = Some(d.clone()); }
            if let Some(x) = &overrides.distortion  { t.instrument.effects.distortion = Some(x.clone()); }
            if let Some(f) = &overrides.filter      { t.instrument.effects.filter     = Some(f.clone()); }
            t.instrument.volume *= track_vol;

            // Actual duration in samples
            let beat_dur    = 60.0 / t.tempo;
            let track_secs  = t.length * beat_dur;
            let track_total = (track_secs * self.sample_rate) as usize;
            if track_total == 0 { continue; }

            // Synthesize the full track into its own buffer in one pass.
            let mut track_buf = vec![0.0f32; track_total];
            self.synthesize_track_into(&mut track_buf, &t, 0);

            // Then apply stateful effects in a single ordered pass
            if t.instrument.effects.has_any() {
                let mut fx = EffectsProcessor::new(self.sample_rate);
                for s in track_buf.iter_mut() {
                    *s = fx.process(*s, &t.instrument.effects);
                }
            }

            // To accumulate into the arrangement output buffer.
            for (i, &s) in track_buf.iter().enumerate() {
                if let Some(dst) = buffer.get_mut(start_sample + i) {
                    *dst += s * params.master_volume;
                }
            }
        }

        if let Some(fi) = arrangement.fade_in {
            let n = (fi * self.sample_rate) as usize;
            for i in 0..n.min(buffer.len()) { buffer[i] *= i as f32 / n as f32; }
        }
        if let Some(fo) = arrangement.fade_out {
            let n  = (fo * self.sample_rate) as usize;
            let fs = buffer.len().saturating_sub(n);
            for i in fs..buffer.len() { buffer[i] *= (buffer.len() - i) as f32 / n as f32; }
        }
        if let Some(max) = buffer.iter().map(|v| v.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()) {
            if max > 1.0 { buffer.iter_mut().for_each(|s| *s /= max); }
        }
        Ok(buffer)
    }
 
    pub(crate) fn synthesize_track_into(&self, buffer: &mut [f32], track: &MelodyTrack, start_sample: usize) {
        let mut cur = 0usize;
        let beat_dur = 60.0 / track.tempo;
        for element in &track.sequence {
            match element {
                SequenceElement::Note(note) => {
                    let nd = note.duration * beat_dur;
                    match &track.instrument.source {
                        InstrumentSource::Synthesized(_) => {
                            let ns = (nd * self.sample_rate) as usize;
                            let mut phase = 0.0f32;
                            for i in 0..ns {
                                let idx = start_sample + cur + i;
                                if idx >= buffer.len() { break; }
                                let t = i as f32 / self.sample_rate;
                                let env = self.calculate_envelope(t, nd, &track.instrument);
                                let mut pitch = note.pitch;
                                if let Some(st) = note.slide_to { pitch = note.pitch * (1.0 - t / nd) + st * (t / nd); }
                                if let InstrumentSource::Synthesized(wf) = &track.instrument.source {
                                    buffer[idx] += wf.generate_sample(phase) * env * note.velocity * track.instrument.volume;
                                    phase += pitch / self.sample_rate;
                                    if phase >= 1.0 { phase -= 1.0; }
                                }
                            }
                            cur += ns;
                        }
                        InstrumentSource::Sample(sd) => {
                            let pr  = track.instrument.pitch;
                            let olen = (sd.samples.len() as f32 / pr) as usize;
                            let adur = olen as f32 / self.sample_rate;
                            for i in 0..olen {
                                let idx = start_sample + cur + i;
                                if idx >= buffer.len() { break; }
                                let t = i as f32 / self.sample_rate;
                                let env = self.calculate_envelope(t, adur, &track.instrument);
                                buffer[idx] += Self::interpolate_sample(sd, t, pr) * env * note.velocity * track.instrument.volume;
                            }
                            cur += olen;
                        }
                    }
                }
                SequenceElement::Chord(chord) => {
                    let cd = chord.duration * beat_dur;
                    let cs = (cd * self.sample_rate) as usize;
                    for pitch in &chord.pitches {
                        let mut phase = 0.0f32;
                        for i in 0..cs {
                            let idx = start_sample + cur + i;
                            if idx >= buffer.len() { break; }
                            let t = i as f32 / self.sample_rate;
                            let env = self.calculate_envelope(t, cd, &track.instrument);
                            if let InstrumentSource::Synthesized(wf) = &track.instrument.source {
                                buffer[idx] += wf.generate_sample(phase) * env * chord.velocity * track.instrument.volume / chord.pitches.len() as f32;
                                phase += pitch / self.sample_rate;
                                if phase >= 1.0 { phase -= 1.0; }
                            }
                        }
                    }
                    cur += cs;
                }
                SequenceElement::Rest(d) => { cur += (d * beat_dur * self.sample_rate) as usize; }
            }
        }
    }

    #[inline]
    fn interpolate_sample(sd: &SampleData, t: f32, pitch: f32) -> f32 {
        let pos = t * sd.sample_rate as f32 * pitch;
        let idx = pos as usize;
        if idx >= sd.samples.len() { return 0.0; }
        if idx + 1 < sd.samples.len() {
            let frac = pos - idx as f32;
            sd.samples[idx] * (1.0 - frac) + sd.samples[idx + 1] * frac
        } else {
            sd.samples[idx]
        }
    }

    fn calculate_envelope(&self, time: f32, duration: f32, instr: &Instrument) -> f32 {
        Self::calculate_envelope_static(time, duration, instr)
    }

    fn calculate_envelope_static(time: f32, duration: f32, instr: &Instrument) -> f32 {
        if time >= duration { return 0.0; }

        let attack  = instr.attack.max(1e-6);
        let decay   = instr.decay.max(1e-6);
        let release = instr.release.max(1e-6);

        // When a note is shorter than attack + release, proportionally scale both so they still fit.
        let min_ar = attack + release;
        let (eff_attack, eff_release) = if duration < min_ar {
            let s = duration / min_ar;
            (attack * s, release * s)
        } else {
            (attack, release)
        };

        // rel_start is always >= 0 now because eff_release <= duration by construction.
        let rel_start = duration - eff_release;

        // Level at the moment release begins, may land mid-attack or mid-decay.
        let level_at_rel = if rel_start < eff_attack {
            rel_start / eff_attack
        } else if rel_start < eff_attack + decay {
            1.0 - ((rel_start - eff_attack) / decay) * (1.0 - instr.sustain)
        } else {
            instr.sustain
        };

        if time >= rel_start {
            let t = (time - rel_start) / eff_release;
            return level_at_rel * (1.0 - t).max(0.0);
        }

        if time < eff_attack {
            time / eff_attack
        } else if time < eff_attack + decay {
            1.0 - ((time - eff_attack) / decay) * (1.0 - instr.sustain)
        } else {
            instr.sustain
        }
    }

    pub fn load_midi(&mut self, name_prefix: &str, path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let data   = std::fs::read(path)?;
        let tracks = crate::midi::parse_midi_bytes(&data)?;
        let single = tracks.len() == 1;
        let mut keys = Vec::with_capacity(tracks.len());

        for track in tracks {
            let key = if single {
                name_prefix.to_string()
            } else {
                format!("{}_{}", name_prefix, track.name)
            };
            self.mel_cache.insert(key.clone(), track);
            keys.push(key);
        }

        Ok(keys)
    }
}

