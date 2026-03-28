#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaveformType {
    Sine,
    Square,
    Triangle,
    Sawtooth,
    Noise,
}

impl WaveformType {
    pub fn generate_sample(&self, phase: f32) -> f32 { // Phase should be in the range [0.0, 1.0)
        match self {
            WaveformType::Sine => (phase * std::f32::consts::TAU).sin(),
            WaveformType::Square => if (phase * 2.0) % 1.0 < 0.5 { 1.0 } else { -1.0 },
            WaveformType::Sawtooth => (phase * 2.0) % 1.0 * 2.0 - 1.0,
            WaveformType::Noise => fastrand::f32() * 2.0 - 1.0, // WaveformType::Noise => (((phase * 1235.647).sin() * 43758.5453).fract() * 2.0 - 1.0), <- possible replacement
            WaveformType::Triangle => {
                let p = (phase * 2.0) % 1.0;
                if p < 0.5 { p * 4.0 - 1.0 } else { 3.0 - p * 4.0 }
            }
        }
    }

    // Stable ID written into GpuNoteData.waveform_type.
    // Only defined for waveforms that have a GPU path.
    #[cfg(feature = "gpu")]
    pub fn gpu_id(&self) -> Option<u32> {
        match self {
            WaveformType::Sine     => Some(0),
            WaveformType::Square   => Some(1),
            WaveformType::Triangle => Some(2),
            WaveformType::Sawtooth => Some(3),
            WaveformType::Noise    => None,
        }
    }

    // Generates a WGSL-case block for this waveform.
    // The result is intended to be inserted into the generated
    // wave(phase, wf) switch statement.
    // Returns None if the waveform is CPU-only and has no GPU equivalent.
    #[cfg(feature = "gpu")]
    pub fn wgsl_case(&self) -> Option<String> {
        let id = self.gpu_id()?;
        let body = match self {
            WaveformType::Sine =>
                "return sin(TAU * phase);".to_string(),
            WaveformType::Square =>
                "return select(-1.0, 1.0, fract(phase * 2.0) < 0.5);".to_string(),
            WaveformType::Triangle =>
                "let p = fract(phase * 2.0); \
                 return select(3.0 - p * 4.0, p * 4.0 - 1.0, p < 0.5);".to_string(),
            WaveformType::Sawtooth =>
                "return fract(phase * 2.0) * 2.0 - 1.0;".to_string(),
            WaveformType::Noise => return None,
        };
        Some(format!("case {id}u: {{ {body} }}"))
    }
}



