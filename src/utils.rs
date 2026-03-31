use crate::error::SynthError;

// Uses the standard MIDI formula: f = 440 × 2^((n − 69) / 12)

pub fn parse_note(note_str: &str) -> Result<f32, SynthError> {
    let chars: Vec<char> = note_str.to_uppercase().chars().collect();

    let semitone: i32 = match chars.first() {
        Some('C') =>  0,
        Some('D') =>  2,
        Some('E') =>  4,
        Some('F') =>  5,
        Some('G') =>  7,
        Some('A') =>  9,
        Some('B') => 11,
        Some(c)   => return Err(SynthError::ParseError(format!("Invalid note name: {c}"))),
        None      => return Err(SynthError::ParseError("Empty note string".to_string())),};

    let (semitone, octave_start) = match chars.get(1) {
        Some('#') | Some('S') => (semitone + 1, 2),
        Some('B') | Some('F') => (semitone - 1, 2),
        _                     => (semitone,     1),
    };

    let octave_str: String = chars[octave_start..].iter().collect();
    let octave_str = octave_str.trim();

    let octave: i32 = if octave_str.is_empty() {
        4 // default to octave 4
    } else {
        octave_str.parse::<i32>().map_err(|_| {
            SynthError::ParseError(format!("Invalid octave: '{octave_str}'"))
        })?
    };

    // Standard mapping: Accidentals that stray below 0 or above 127  are caught by the range check
    let midi_note = (octave + 1) * 12 + semitone;
    if !(0..=127).contains(&midi_note) {
        return Err(SynthError::ParseError(format!(
            "MIDI note {midi_note} out of range 0–127 (got {note_str})"
        )));
    }

    let freq = 440.0_f32 * 2.0_f32.powf((midi_note - 69) as f32 / 12.0);
    Ok(freq)
}
