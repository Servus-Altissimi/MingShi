use std::fmt;

impl std::error::Error for SynthError {}

#[derive(Debug, Clone)]
pub enum SynthError {
    ParseError(String),
    FileError(String),
    AudioError(String),
    InvalidInstrument(String),
}

impl fmt::Display for SynthError { // TODO, expand
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SynthError::ParseError(msg) => write!(f, "Parsing Error: {}", msg),
            SynthError::FileError(msg) => write!(f, "File Error: {}", msg),
            SynthError::AudioError(msg) => write!(f, "Audio Error: {}", msg),
            SynthError::InvalidInstrument(msg) => write!(f, "Invalid Instrument Error: {}", msg),
        }
    }
}