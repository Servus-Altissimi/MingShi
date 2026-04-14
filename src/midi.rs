// Standard MIDI File (SMF) parsing code for MingShi! This feature was very 
// necessary and on the backburner for far too long.
// There were a lot of open-source examples to work with. I could've used
// an existing crate, but I persee want to keep dependencies to a minimum.
//
// Supported:
//   Format 0, 1, 2; running status, velocity-0 note-off, SysEx skip and all meta events
//   General MIDI program -> WaveformType mapping
//
// Not supported / TODO:
//   SMPTE timecode division
//   Pitch bend, aftertouch, CC (filtered out)
//   Percussion channel (ch 9) is parsed as pitched notes

// MIDI byte references:
//
//   Status bytes (high nibble = type, low nibble = channel 0–15):
//     0x80  Note Off
//     0x90  Note On           (vel = 0 same as Note Off, running status)
//     0xA0  Aftertouch        (filtered)
//     0xB0  Control Change    (filtered)
//     0xC0  Program Change    (1 data byte, no channel)
//     0xD0  Channel Pressure  (filtered)
//     0xE0  Pitch Bend        (filtered)
//     0xF0  SysEx start       (skipped; resets running status)
//     0xF7  SysEx end/escape  (skipped; resets running status)
//     0xF8+ System Real-Time  (does not reset running status, hence < 0xF8 guard)
//     0xFF  Meta event        (not a channel message)
//
//   Meta event types (follow 0xFF):
//     0x03  Track name
//     0x2F  End of track
//     0x51  Set tempo (3-byte payload = microseconds per beat; default 500 000 = 120 BPM)
//     0x58  Time signature
//
//   Header division field (bytes 12-13):
//     MSB=0  Ticks-per-beat mode   (supported)
//     MSB=1  SMPTE timecode mode   (not supported)
//
//   VLQ , delta-time encoding:
//     Each byte contributes 7 value bits; bit 7 = 1 meaning another byte follows.
//     Max 4 bytes (28 bits).

use std::collections::HashMap;
use crate::error::SynthError;
use crate::instrument::{Chord, Instrument, InstrumentSource, Note, SequenceElement};
use crate::track::MelodyTrack;
use crate::waveform::WaveformType;

fn read_u16_be(data: &[u8], off: usize) -> Result<u16, SynthError> {
    data.get(off..off + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| SynthError::ParseError("Unexpected EOF reading u16".into()))
}

fn read_u32_be(data: &[u8], off: usize) -> Result<u32, SynthError> {
    data.get(off..off + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| SynthError::ParseError("Unexpected EOF reading u32".into()))
}

// Reads a MIDI variable-length quantity & returns (value, bytes_consumed)
fn read_vlq(data: &[u8]) -> Result<(u32, usize), SynthError> {
    let mut value = 0u32;
    for (i, &byte) in data.iter().enumerate().take(4) {
        value = (value << 7) | (byte & 0x7F) as u32;
        if byte & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(SynthError::ParseError("VLQ exceeds 4 bytes or is cut short".into()))
}

pub fn midi_note_to_freq(midi: u8) -> f32 {
    440.0_f32 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

#[derive(Debug, Clone)]
struct MidiEvent {
    tick: u64,
    kind: EventKind,
}

#[derive(Debug, Clone)]
enum EventKind {
    NoteOn        { channel: u8, note: u8, velocity: u8 },
    NoteOff       { channel: u8, note: u8 },
    ProgramChange { _channel: u8, program: u8 },
    SetTempo      (u32),
    TimeSignature { numerator: u8, denominator_pow2: u8 },
    TrackName     (String),
    EndOfTrack,
    Other,
}

fn parse_track_chunk(data: &[u8]) -> Result<Vec<MidiEvent>, SynthError> {
    let mut events   = Vec::new();
    let mut pos      = 0usize;
    let mut abs_tick = 0u64;
    let mut running  = 0u8;

    while pos < data.len() {
        let (delta, n) = read_vlq(&data[pos..])?;
        pos      += n;
        abs_tick += delta as u64;

        if pos >= data.len() { break; }

        let byte   = data[pos];
        let status = if byte & 0x80 != 0 {
            if byte < 0xF8 { running = byte; }
            pos += 1;
            byte
        } else {
            running
        };

        let kind = if status == 0xFF {
            let meta_type = *data.get(pos).ok_or_else(|| SynthError::ParseError("Meta type missing".into()))?;
            pos += 1;
            let (len, n) = read_vlq(&data[pos..])?;
            pos += n;
            let payload = data.get(pos..pos + len as usize)
                .ok_or_else(|| SynthError::ParseError("Meta payload truncated".into()))?;
            pos += len as usize;

            match meta_type {
                0x03 => EventKind::TrackName(String::from_utf8_lossy(payload).into_owned()),
                0x2F => {
                    events.push(MidiEvent { tick: abs_tick, kind: EventKind::EndOfTrack });
                    break;
                }
                0x51 if len == 3 => {
                    let t = u32::from_be_bytes([0, payload[0], payload[1], payload[2]]);
                    EventKind::SetTempo(t)
                }
                0x58 if len >= 2 => EventKind::TimeSignature {
                    numerator:        payload[0],
                    denominator_pow2: payload[1],
                },
                _ => EventKind::Other,
            }
        } else if status == 0xF0 || status == 0xF7 {
            let (len, n) = read_vlq(&data[pos..])?;
            pos    += n + len as usize;
            running = 0;
            EventKind::Other
        } else {
            let event_type = status & 0xF0;
            let channel    = status & 0x0F;

            macro_rules! read_byte {
                ($err:expr) => {{
                    let b = *data.get(pos).ok_or_else(|| SynthError::ParseError($err.into()))?;
                    pos += 1;
                    b
                }};
            }

            match event_type {
                0x80 => {
                    let note = read_byte!("NoteOff note missing");
                    pos += 1;
                    EventKind::NoteOff { channel, note }
                }
                0x90 => {
                    let note = read_byte!("NoteOn note missing");
                    let vel  = read_byte!("NoteOn velocity missing");
                    if vel == 0 { EventKind::NoteOff { channel, note } }
                    else        { EventKind::NoteOn  { channel, note, velocity: vel } }
                }
                0xA0 | 0xB0 | 0xE0 => { pos += 2; EventKind::Other }
                0xC0 => {
                    let prog = read_byte!("ProgramChange missing");
                    EventKind::ProgramChange { _channel: channel, program: prog }
                }
                0xD0 => { pos += 1; EventKind::Other }
                _ => EventKind::Other,
            }
        };

        events.push(MidiEvent { tick: abs_tick, kind });
    }

    Ok(events)
}

struct TempoMap {
    checkpoints:    Vec<(u64, f64, u32)>,
    ticks_per_beat: u16,
}

impl TempoMap {
    fn build(all_tracks: &[&[MidiEvent]], ticks_per_beat: u16) -> Self {
        const DEFAULT_TEMPO: u32 = 500_000;

        let mut raw: Vec<(u64, u32)> = all_tracks.iter()
            .flat_map(|evs| evs.iter())
            .filter_map(|e| if let EventKind::SetTempo(t) = &e.kind { Some((e.tick, *t)) } else { None })
            .collect();
        raw.sort_by_key(|(t, _)| *t);
        raw.dedup_by_key(|(t, _)| *t);

        let mut checkpoints: Vec<(u64, f64, u32)> = Vec::with_capacity(raw.len() + 1);
        checkpoints.push((0, 0.0, DEFAULT_TEMPO));

        for (tick, tempo) in raw {
            let &(prev_tick, prev_secs, prev_tempo) = checkpoints.last().unwrap();
            if tick == 0 {
                checkpoints[0].2 = tempo;
                continue;
            }
            let elapsed = (tick - prev_tick) as f64 * prev_tempo as f64
                / (ticks_per_beat as f64 * 1_000_000.0);
            checkpoints.push((tick, prev_secs + elapsed, tempo));
        }

        TempoMap { checkpoints, ticks_per_beat }
    }

    fn ticks_to_seconds(&self, tick: u64) -> f32 {
        let idx = self.checkpoints
            .partition_point(|(t, _, _)| *t <= tick)
            .saturating_sub(1);
        let (base_tick, base_secs, tempo) = self.checkpoints[idx];
        let elapsed = (tick - base_tick) as f64 * tempo as f64
            / (self.ticks_per_beat as f64 * 1_000_000.0);
        (base_secs + elapsed) as f32
    }
}

const CHORD_EPSILON: f32 = 0.005; // Redundant.

// Output tracks are pinned to 120 BPM so beat durations can stay stable.
const FIXED_BPM: f32    = 120.0;
const FIXED_BEAT_S: f32 = 60.0 / FIXED_BPM;

#[inline]
fn secs_to_beats(s: f32) -> f32 { s / FIXED_BEAT_S }

fn build_melody_track(
    events:    &[MidiEvent],
    tempo_map: &TempoMap,
    name:      &str,
    program:   Option<u8>,
) -> MelodyTrack {
    let mut time_sig = (4u32, 4u32);
    for ev in events {
        if let EventKind::TimeSignature { numerator, denominator_pow2 } = &ev.kind {
            time_sig = (*numerator as u32, 2u32.pow(*denominator_pow2 as u32));
            break;
        }
    }

    struct RawNote { start: f32, end: f32, freq: f32, velocity: f32 }

    // FIFO queue per (channel, note) so legato playing resolves correctly.
    let mut active: HashMap<(u8, u8), Vec<(f32, f32)>> = HashMap::new();
    let mut raw: Vec<RawNote> = Vec::new();

    for ev in events {
        match &ev.kind {
            EventKind::NoteOn { channel, note, velocity } => {
                let s = tempo_map.ticks_to_seconds(ev.tick);
                active
                    .entry((*channel, *note))
                    .or_default()
                    .push((s, *velocity as f32 / 127.0));
            }
            EventKind::NoteOff { channel, note } => {
                if let Some(queue) = active.get_mut(&(*channel, *note)) {
                    if !queue.is_empty() {
                        let (start_s, vel) = queue.remove(0);
                        let end_s = tempo_map.ticks_to_seconds(ev.tick).max(start_s + 0.001);
                        raw.push(RawNote {
                            start: start_s,
                            end: end_s,
                            freq: midi_note_to_freq(*note),
                            velocity: vel,
                        });
                        if queue.is_empty() {
                            active.remove(&(*channel, *note));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let track_end_s = events.iter().map(|e| e.tick).max()
        .map(|t| tempo_map.ticks_to_seconds(t))
        .unwrap_or(0.0);
    for ((_ch, note), queue) in active {
        for (start_s, vel) in queue {
            raw.push(RawNote {
                start: start_s,
                end: track_end_s.max(start_s + 0.001),
                freq: midi_note_to_freq(note),
                velocity: vel,
            });
        }
    }

    if raw.is_empty() {
        return MelodyTrack {
            name:           name.to_string(),
            instrument:     instrument_for_program(program),
            sequence:       Vec::new(),
            tempo:          FIXED_BPM,
            length:         0.0,
            loop_point:     None,
            time_signature: time_sig,
            swing:          0.0,
        };
    }

    raw.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));

    let mut sequence      = Vec::new();
    let mut cursor_s      = 0.0f32;
    let mut track_end_used = 0.0f32;
    let mut i             = 0;

    while i < raw.len() {
        let group_start = raw[i].start;

        let gap = group_start - cursor_s;
        if gap > CHORD_EPSILON {
            sequence.push(SequenceElement::Rest(secs_to_beats(gap)));
        }

        let count = raw[i..].partition_point(|n| (n.start - group_start).abs() < CHORD_EPSILON);
        let group = &raw[i..i + count];

        let dur_s     = group.iter().map(|n| n.end - n.start).fold(0.0f32, f32::max).max(0.001);
        let dur_beats = secs_to_beats(dur_s);
        let avg_vel   = group.iter().map(|n| n.velocity).sum::<f32>() / group.len() as f32;

        if group.len() == 1 {
            sequence.push(SequenceElement::Note(Note {
                pitch:    group[0].freq,
                duration: dur_beats,
                velocity: group[0].velocity,
                pan:      None,
                slide_to: None,
            }));
        } else {
            sequence.push(SequenceElement::Chord(Chord {
                pitches:  group.iter().map(|n| n.freq).collect(),
                duration: dur_beats,
                velocity: avg_vel,
            }));
        }

        cursor_s       = group_start + dur_s;
        track_end_used = track_end_used.max(cursor_s);
        i             += count;
    }

    MelodyTrack {
        name:           name.to_string(),
        instrument:     instrument_for_program(program),
        sequence,
        tempo:          FIXED_BPM,
        length:         secs_to_beats(track_end_used),
        loop_point:     None,
        time_signature: time_sig,
        swing:          0.0,
    }
}

fn instrument_for_program(program: Option<u8>) -> Instrument {
    use crate::effects::{DelayParams, FilterParams, FilterType, ReverbParams};
    use WaveformType::*;

    struct Spec {
        wf:      WaveformType,
        name:    &'static str,
        attack:  f32,
        decay:   f32,
        sustain: f32,
        release: f32,
        volume:  f32,
    }
    macro_rules! sp {
        ($wf:expr, $name:literal, $a:expr, $d:expr, $s:expr, $r:expr, $v:expr) => {
            Spec { wf: $wf, name: $name, attack: $a, decay: $d, sustain: $s, release: $r, volume: $v }
        };
    }
    
    // General midi instruments, these should be correct
    let s = match program {
        None | Some(0..=7)     => sp!(Sine,     "Piano",       0.004, 0.55, 0.10, 0.30, 0.55),
        Some(8..=15)           => sp!(Triangle, "Chrom. Perc", 0.003, 0.25, 0.00, 0.15, 0.55),
        Some(16..=23)          => sp!(Square,   "Organ",       0.060, 0.05, 0.90, 0.40, 0.45),
        Some(24..=31)          => sp!(Triangle, "Guitar",      0.005, 0.40, 0.05, 0.20, 0.52),
        Some(32..=39)          => sp!(Sawtooth, "Bass",        0.006, 0.18, 0.45, 0.12, 0.58),
        Some(40..=47)          => sp!(Sawtooth, "Strings",     0.120, 0.10, 0.85, 0.45, 0.48),
        Some(48..=55)          => sp!(Sawtooth, "Ensemble",    0.200, 0.12, 0.80, 0.60, 0.44),
        Some(56..=63)          => sp!(Square,   "Brass",       0.050, 0.08, 0.82, 0.15, 0.50),
        Some(64..=71)          => sp!(Square,   "Reed",        0.035, 0.08, 0.78, 0.20, 0.48),
        Some(72..=79)          => sp!(Sine,     "Pipe",        0.070, 0.06, 0.88, 0.30, 0.46),
        Some(80..=87)          => sp!(Sawtooth, "Synth Lead",  0.008, 0.06, 0.80, 0.10, 0.52),
        Some(88..=95)          => sp!(Triangle, "Synth Pad",   0.350, 0.15, 0.75, 0.80, 0.42),
        Some(96..=103)         => sp!(Sawtooth, "Synth FX",    0.180, 0.20, 0.55, 0.50, 0.40),
        Some(104..=111)        => sp!(Triangle, "Ethnic",      0.008, 0.30, 0.10, 0.18, 0.50),
        Some(112..=119)        => sp!(Sine,     "Percussive",  0.002, 0.12, 0.00, 0.08, 0.55),
        Some(120..=127) | Some(_) => sp!(Noise, "SFX",         0.010, 0.10, 0.30, 0.15, 0.35),
    };

    let mut effects = crate::effects::EffectsChain::default();

    match program {
        None | Some(0..=7) => {
            effects.reverb = Some(ReverbParams { room_size: 0.35, damping: 0.65, wet: 0.18, width: 0.9 });
        }
        Some(8..=15) => {
            effects.reverb = Some(ReverbParams { room_size: 0.45, damping: 0.55, wet: 0.22, width: 1.0 });
        }
        Some(40..=55) => {
            effects.reverb = Some(ReverbParams { room_size: 0.72, damping: 0.45, wet: 0.36, width: 1.0 });
        }
        Some(72..=79) => {
            effects.reverb = Some(ReverbParams { room_size: 0.50, damping: 0.60, wet: 0.20, width: 0.8 });
        }
        Some(88..=95) => {
            effects.reverb = Some(ReverbParams { room_size: 0.85, damping: 0.35, wet: 0.50, width: 1.0 });
        }
        Some(32..=39) => {
            effects.filter = Some(FilterParams {
                filter_type: FilterType::LowPass,
                cutoff: 380.0,
                resonance: 0.65,
            });
        }
        Some(80..=87) => {
            effects.delay = Some(DelayParams { time: 0.06, feedback: 0.20, wet: 0.18 });
        }
        _ => {}
    }

    Instrument {
        name:    s.name.to_string(),
        source:  InstrumentSource::Synthesized(s.wf),
        attack:  s.attack,
        decay:   s.decay,
        sustain: s.sustain,
        release: s.release,
        volume:  s.volume,
        effects,
        ..Instrument::default()
    }
}

pub fn parse_midi_bytes(data: &[u8]) -> Result<Vec<MelodyTrack>, SynthError> {
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return Err(SynthError::ParseError("Not a valid MIDI file (missing MThd)".into()));
    }
    let header_len = read_u32_be(data, 4)? as usize;
    if header_len < 6 {
        return Err(SynthError::ParseError("MIDI header too short".into()));
    }
    let format     = read_u16_be(data, 8)?;
    let num_tracks = read_u16_be(data, 10)?;
    let raw_div    = read_u16_be(data, 12)?;

    if raw_div & 0x8000 != 0 {
        return Err(SynthError::ParseError(
            "SMPTE timecode MIDI files are not supported; use ticks-per-beat files.".into()
        ));
    }
    let tpb = raw_div;

    let mut pos = 8 + header_len;
    let mut raw_chunks: Vec<Vec<u8>> = Vec::with_capacity(num_tracks as usize);

    for i in 0..num_tracks {
        if pos + 8 > data.len() {
            eprintln!("Warning: expected {num_tracks} MIDI tracks, found {i}; stopping early.");
            break;
        }
        if &data[pos..pos + 4] != b"MTrk" {
            return Err(SynthError::ParseError(
                format!("Expected MTrk at offset {pos:#x}, got {:?}", &data[pos..pos + 4])
            ));
        }
        let chunk_len = read_u32_be(data, pos + 4)? as usize;
        pos += 8;
        let end = pos + chunk_len;
        if end > data.len() {
            return Err(SynthError::ParseError(format!("Track {i} data is truncated")));
        }
        raw_chunks.push(data[pos..end].to_vec());
        pos = end;
    }

    let event_lists: Vec<Vec<MidiEvent>> = raw_chunks.iter()
        .map(|c| parse_track_chunk(c))
        .collect::<Result<_, _>>()?;

    let all_refs: Vec<&[MidiEvent]> = event_lists.iter().map(|v| v.as_slice()).collect();

    let build_track = |events: &[MidiEvent], idx: usize, tm: &TempoMap| -> MelodyTrack {
        let name = events.iter().find_map(|e| {
            if let EventKind::TrackName(n) = &e.kind { Some(n.clone()) } else { None }
        }).unwrap_or_else(|| format!("track_{idx}"));

        let program = events.iter().find_map(|e| {
            if let EventKind::ProgramChange { program, .. } = &e.kind { Some(*program) } else { None }
        });

        build_melody_track(events, tm, &name, program)
    };

    let tracks: Vec<MelodyTrack> = match format {
        0 => {
            let tm = TempoMap::build(&all_refs, tpb);
            vec![build_track(&event_lists[0], 0, &tm)]
        }
        1 => {
            let tm = TempoMap::build(&all_refs, tpb);
            event_lists.iter().enumerate()
                .map(|(i, evs)| build_track(evs, i, &tm))
                .filter(|t| !t.sequence.is_empty())
                .collect()
        }
        2 => {
            event_lists.iter().enumerate().map(|(i, evs)| {
                let tm = TempoMap::build(&[evs.as_slice()], tpb);
                build_track(evs, i, &tm)
            })
            .filter(|t| !t.sequence.is_empty())
            .collect()
        }
        _ => return Err(SynthError::ParseError(format!("Unknown MIDI format: {format}"))),
    };

    if tracks.is_empty() {
        return Err(SynthError::ParseError("No playable notes found!".into()));
    }

    Ok(tracks)
}
