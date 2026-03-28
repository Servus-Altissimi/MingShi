//  ______   _______  _______  _______ _________ _______ 
// (  ___ \ (  ___  )(  ___  |(       )\__   __|(  ____ \
// | (   ) )| (   ) || (   ) || () () |   ) (   | (    \/
// | (__/ / | |   | || |   | || || || |   | |   | (__    
// |  __ (  | |   | || |   | || |(_)| |   | |   |  __)   
// | (  \ \ | |   | || |   | || |   | |   | |   | (      
// | )___) )| (___) || (___) || )   ( |___) (___| (____/\
// |/ \___/ (_______)(_______)|/     \|\_______/(_______/

pub mod error;
pub mod waveform;
pub mod instrument;
pub mod effects;
pub mod track;
pub mod arrangement;
pub mod engine;
pub mod utils;

#[cfg(feature = "gpu")]
pub mod gpu_synth;

pub use error::SynthError;
pub use waveform::WaveformType;
pub use instrument::{Instrument, InstrumentSource, SampleData, Note, Chord, SequenceElement};
pub use effects::{EffectsChain, ReverbParams, DelayParams, DistortionParams, FilterParams, FilterType, EffectsProcessor};
pub use track::{MelodyTrack, LoopPoint};
pub use arrangement::{Arrangement, TrackOverrides};
pub use engine::{SynthEngine, PlaybackState, DynamicParameters};

#[cfg(feature = "gpu")]
pub use gpu_synth::{GpuSynthEngine, AudioUniforms};
