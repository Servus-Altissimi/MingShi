use crate::error::SynthError;

// TODO: Use standard MIDI formula

pub fn parse_note(note_str: &str) -> Result<f32, SynthError> {
    let note_str = note_str.to_uppercase();
    let mut freq = match note_str.chars().next() {
        Some('C') => 16.35,
        Some('D') => 18.35,
        Some('E') => 20.60,
        Some('F') => 21.83,
        Some('G') => 24.50,
        Some('A') => 27.50,
        Some('B') => 30.87,
        _ => return Err(SynthError::ParseError("Invalid note".to_string())),
    };

    let second_char = note_str.chars().nth(1);
    let has_accidental = matches!(second_char, Some('#') | Some('S') | Some('B') | Some('F'));
    
    if has_accidental {
        match second_char {
            Some('#') | Some('S') => freq *= 1.059463,
            Some('B') | Some('F') => freq *= 0.943874,
            _ => {}
        }
    }

    let octave_start = if has_accidental { 2 } else { 1 };
    let octave_str = note_str[octave_start..].trim();

    if !octave_str.is_empty() {
        if let Ok(octave) = octave_str.parse::<i32>() {
            freq *= 2.0_f32.powi(octave);
        }
    }

    Ok(freq)

}
