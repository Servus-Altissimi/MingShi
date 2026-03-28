use std::collections::HashMap;
use crate::error::SynthError;
use crate::instrument::{Instrument, InstrumentSource, SampleData, Note, Chord, SequenceElement};
use crate::waveform::WaveformType;
use crate::effects::{ReverbParams, DelayParams, DistortionParams, FilterParams, FilterType};
use crate::utils::parse_note;

#[derive(Debug, Clone)]
pub struct LoopPoint {
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone)]
pub struct MelodyTrack {
    pub name: String,
    pub instrument: Instrument,
    pub sequence: Vec<SequenceElement>,
    pub tempo: f32,
    pub length: f32,
    pub loop_point: Option<LoopPoint>,
    pub time_signature: (u32, u32), 
    pub swing: f32, // Swing feel: 0.0 = straight, 0.5 = triplet, 1.0 = max
}

impl MelodyTrack {
    pub fn from_mel(content: &str, sample_cache: &HashMap<String, SampleData>) -> Result<Self, SynthError> {
        let mut track = MelodyTrack {
            name: "melody".to_string(),
            instrument: Instrument::default(),
            sequence: Vec::new(),
            tempo: 120.0,
            length: 0.0,
            loop_point: None,
            time_signature: (4, 4),
            swing: 0.0,
        };

        macro_rules! parse_field {
            ($line:expr, $prefix:expr, $field:expr) => {
                if let Some(v) = $line.strip_prefix($prefix) {
                    $field = v.trim().parse()
                        .map_err(|_| SynthError::ParseError(format!("Invalid {}", $prefix)))?;
                    continue;
                }
            };
        }

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") { continue; } // Comments (//) & empty lines 

            if let Some(v) = line.strip_prefix("name:") {
                track.name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("loop:") {
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    track.loop_point = Some(LoopPoint {
                        start: parts[0].parse().unwrap_or(0.0),
                        end: parts[1].parse().unwrap_or(track.length),
                    });
                }

            } else if let Some(v) = line.strip_prefix("time_sig:") { 
                let parts: Vec<&str> = v.split('/').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    track.time_signature = (
                        parts[0].parse().unwrap_or(4),
                        parts[1].parse().unwrap_or(4),
                    );
                }

            } else if let Some(v) = line.strip_prefix("sample:") {
                track.instrument.source = InstrumentSource::Sample(
                    sample_cache.get(v.trim())
                        .ok_or_else(|| SynthError::InvalidInstrument(format!("Sample not found: {}", v.trim())))?
                        .clone()
                );
                
            } else if let Some(v) = line.strip_prefix("waveform:") {
                track.instrument.source = InstrumentSource::Synthesized(match v.trim().to_lowercase().as_str() {
                    "sine" => WaveformType::Sine,
                    "square" => WaveformType::Square,
                    "triangle" => WaveformType::Triangle,
                    "sawtooth" => WaveformType::Sawtooth,
                    "noise" => WaveformType::Noise,
                    _ => return Err(SynthError::ParseError("Unknown Waveform".to_string())),
                });

            } else if let Some(v) = line.strip_prefix("note:") {
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    let pitch = parse_note(parts[0])?;
                    let duration: f32 = parts[1].parse()
                        .map_err(|_| SynthError::ParseError("Invalid Duration".to_string()))?;
                    let velocity: f32 = parts[2].split("//").next().unwrap_or("0").trim().parse()
                        .map_err(|_| SynthError::ParseError("Invalid Velocity".to_string()))?;
                    
                    let mut note = Note { pitch, duration, velocity, pan: None, slide_to: None };
                    
                    // Prse optional per-note parameters
                    for param in parts.iter().skip(3) {
                        if let Some((key, val)) = param.split_once('=') {
                            match key.trim() {
                                "pan" => note.pan = val.trim().parse().ok(),
                                "slide" => note.slide_to = Some(parse_note(val.trim())?),
                                _ => {}
                            }
                        }
                    }
                    
                    track.sequence.push(SequenceElement::Note(note));
                    track.length += duration;
                }

            } else if let Some(v) = line.strip_prefix("chord:") { // Parse chords
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    let notes_str = parts[0];
                    let duration: f32 = parts[1].parse()
                        .map_err(|_| SynthError::ParseError("Invalid Duration".to_string()))?;
                    let velocity: f32 = parts[2].split("//").next().unwrap_or("0").trim().parse()
                        .map_err(|_| SynthError::ParseError("Invalid Velocity".to_string()))?;
                    
                    let pitches: Result<Vec<f32>, _> = notes_str
                        .split('+')
                        .map(|n| parse_note(n.trim()))
                        .collect();
                    
                    track.sequence.push(SequenceElement::Chord(Chord {
                        pitches: pitches?,
                        duration,
                        velocity,
                    }));
                    track.length += duration;
                }

            } else if let Some(v) = line.strip_prefix("rest:") { 
                let duration: f32 = v.trim().parse()
                    .map_err(|_| SynthError::ParseError("Invalid rest duration".to_string()))?;
                track.sequence.push(SequenceElement::Rest(duration));
                track.length += duration;

            } else if let Some(v) = line.strip_prefix("filter:") { 
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    let filter_type = match parts[0].to_lowercase().as_str() {
                        "lowpass" | "lp" => FilterType::LowPass,
                        "highpass" | "hp" => FilterType::HighPass,
                        "bandpass" | "bp" => FilterType::BandPass,
                        _ => FilterType::LowPass,
                    };
                    track.instrument.effects.filter = Some(FilterParams {
                        filter_type,
                        cutoff: parts[1].parse().unwrap_or(1000.0),
                        resonance: parts[2].parse().unwrap_or(0.7),
                    });
                }

            } else if let Some(v) = line.strip_prefix("reverb:") {
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 4 {
                    track.instrument.effects.reverb = Some(ReverbParams {
                        room_size: parts[0].parse().unwrap_or(0.5),
                        damping: parts[1].parse().unwrap_or(0.5),
                        wet: parts[2].parse().unwrap_or(0.3),
                        width: parts[3].parse().unwrap_or(1.0),
                    });
                }

            } else if let Some(v) = line.strip_prefix("delay:") {
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    track.instrument.effects.delay = Some(DelayParams {
                        time: parts[0].parse().unwrap_or(0.25),
                        feedback: parts[1].parse().unwrap_or(0.4),
                        wet: parts[2].parse().unwrap_or(0.3),
                    });
                }

            } else if let Some(v) = line.strip_prefix("distortion:") {
                let parts: Vec<&str> = v.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 3 {
                    track.instrument.effects.distortion = Some(DistortionParams {
                        drive: parts[0].parse().unwrap_or(2.0),
                        tone: parts[1].parse().unwrap_or(0.7),
                        wet: parts[2].parse().unwrap_or(0.5),
                    });
                }

            } else {
                parse_field!(line, "tempo:", track.tempo);
                parse_field!(line, "volume:", track.instrument.volume);
                parse_field!(line, "attack:", track.instrument.attack);
                parse_field!(line, "decay:", track.instrument.decay);
                parse_field!(line, "sustain:", track.instrument.sustain);
                parse_field!(line, "release:", track.instrument.release);
                parse_field!(line, "pitch:", track.instrument.pitch);
                parse_field!(line, "pan:", track.instrument.pan);
                parse_field!(line, "detune:", track.instrument.detune);
                parse_field!(line, "swing:", track.swing);
            }
        }

        Ok(track)
    }
}