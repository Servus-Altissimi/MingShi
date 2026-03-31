use crate::error::SynthError;

// Uses the standard MIDI formula: f = 440 × 2^((n − 69) / 12)

pub fn parse_note(note_str: &str) -> Result<f32, SynthError> {
    let note_str = note_str.to_uppercase();

    let semitone: i32 = match note_str.chars().next() {
        Some('C') =>  0,
        Some('D') =>  2,
        Some('E') =>  4,
        Some('F') =>  5,
        Some('G') =>  7,
        Some('A') =>  9,
        Some('B') => 11,
        _ => return Err(SynthError::ParseError("Invalid note name".to_string())),
    };

    let second = note_str.chars().nth(1);
    let has_accidental = matches!(second, Some('#') | Some('S') | Some('B') | Some('F'));

    let semitone = match second {
        Some('#') | Some('S') => semitone + 1,
        Some('B') | Some('F') => semitone - 1,
        _ => semitone,
    };

    let octave_start = if has_accidental { 2 } else { 1 };
    let octave_str = note_str[octave_start..].trim();
    let octave: i32 = if octave_str.is_empty() {
        4 // default to octave 4
    } else {
        octave_str.parse::<i32>().map_err(|_| {
            SynthError::ParseError(format!("Invalid octave: '{octave_str}'"))
        })?
    };
  
    let midi_note = (octave + 1) * 12 + semitone;

    if !(0..=127).contains(&midi_note) {
        return Err(SynthError::ParseError(format!(
            "MIDI note {midi_note} out of range 0–127 (got {note_str})"
        )));
    }
 
    let freq = 440.0_f32 * 2.0_f32.powf((midi_note - 69) as f32 / 12.0);
    Ok(freq)
}
