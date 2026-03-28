use std::sync::Arc;
use crate::waveform::WaveformType;
use crate::effects::EffectsChain;

#[derive(Debug, Clone)]
pub struct SampleData {
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
}

#[derive(Debug, Clone)]
pub enum InstrumentSource {
    Synthesized(WaveformType),
    Sample(SampleData),
}

#[derive(Debug, Clone)]
pub struct Instrument {
    pub name: String,
    pub source: InstrumentSource,
    pub attack: f32, // ADSR envelope parameters
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub volume: f32,
    pub pitch: f32,
    pub pan: f32, // -1.0 left, 1.0 right
    pub detune: f32, // Pitch offset in cents for detuning 
    pub effects: EffectsChain,
}

impl Default for Instrument {
    fn default() -> Self {
        Instrument {
            name: "Boomie".to_string(),
            source: InstrumentSource::Synthesized(WaveformType::Sine),
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.2,
            volume: 0.5,
            pitch: 1.0,
            pan: 0.0,
            detune: 0.0,
            effects: EffectsChain::default(),
        }
    }
}

#[derive(Debug, Clone)] // overrides
pub struct Note {
    pub pitch: f32,
    pub duration: f32,
    pub velocity: f32,
    pub pan: Option<f32>,
    pub slide_to: Option<f32>,
}

// Chord struc for playing multiple notes
#[derive(Debug, Clone)]
pub struct Chord {
    pub pitches: Vec<f32>,
    pub duration: f32,
    pub velocity: f32,
}

// Sequence element enum to support notes, chords, and rests
#[derive(Debug, Clone)]
pub enum SequenceElement {
    Note(Note),
    Chord(Chord),
    Rest(f32),
}