//   ▄▄▄     ▄▄▄                  ▄▄▄▄▄           
//    ███▄ ▄███                  ██▀▀▀▀█▄ █▄      
//    ██ ▀█▀ ██   ▀▀ ▄        ▄▄ ▀██▄  ▄▀ ██    ▀▀
//    ██     ██   ██ ████▄ ▄████   ▀██▄▄  ████▄ ██
//    ██     ██   ██ ██ ██ ██ ██ ▄   ▀██▄ ██ ██ ██
//  ▀██▀     ▀██▄▄██▄██ ▀█▄▀████ ▀██████▀▄██ ██▄██
//                            ██                  
//                          ▀▀▀                   

pub mod error;
pub mod waveform;
pub mod instrument;
pub mod effects;
pub mod track;
pub mod arrangement;
pub mod engine;
pub mod utils;
pub mod midi;

#[cfg(feature = "gpu")]
pub mod gpu_synth;

pub use error::SynthError;
pub use waveform::WaveformType;
pub use instrument::{Instrument, InstrumentSource, SampleData, Note, Chord, SequenceElement};
pub use effects::{EffectsChain, ReverbParams, DelayParams, DistortionParams, FilterParams, FilterType, EffectsProcessor};
pub use track::{MelodyTrack, LoopPoint};
pub use arrangement::{Arrangement, TrackOverrides};
pub use engine::{SynthEngine, PlaybackState, DynamicParameters};
pub use midi::parse_midi_bytes;

#[cfg(feature = "gpu")]
pub use gpu_synth::{GpuSynthEngine, AudioUniforms};
