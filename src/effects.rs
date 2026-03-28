use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub wet: f32,
    pub width: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        ReverbParams {
            room_size: 0.5,
            damping: 0.5,
            wet: 0.3,
            width: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DelayParams {
    pub time: f32,
    pub feedback: f32,
    pub wet: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        DelayParams {
            time: 0.25,
            feedback: 0.4,
            wet: 0.3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DistortionParams {
    pub drive: f32,
    pub tone: f32,
    pub wet: f32,
}

impl Default for DistortionParams {
    fn default() -> Self {
        DistortionParams {
            drive: 2.0,
            tone: 0.7,
            wet: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterParams {
    pub cutoff: f32, // Cutoff frequency in Hz
    pub resonance: f32, // Q factor
    pub filter_type: FilterType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    LowPass, 
    HighPass,
    BandPass,
}

#[derive(Debug, Clone)]
pub struct EffectsChain {
    pub reverb: Option<ReverbParams>,
    pub delay: Option<DelayParams>,
    pub distortion: Option<DistortionParams>,
    pub filter: Option<FilterParams>,
}

impl EffectsChain {
    pub fn has_any(&self) -> bool {
        self.reverb.is_some() || self.delay.is_some() || self.distortion.is_some() || self.filter.is_some()
    }
}


impl Default for EffectsChain {
    fn default() -> Self {
        EffectsChain {
            reverb: None,
            delay: None,
            distortion: None,
            filter: None,
        }
    }
}

pub struct EffectsProcessor {
    sample_rate: f32,
    comb_buffers: Vec<VecDeque<f32>>,
    comb_filter_state: Vec<f32>,
    allpass_buffers: Vec<VecDeque<f32>>,
    delay_buffer: VecDeque<f32>,
    lowpass_state: f32,
    filter_state: (f32, f32), // Biquad filter state (y[n-1], y[n-2])
}

impl EffectsProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0; 
        let comb_delays = vec![ // Freeverb design, 8 combs
            (1116.0 * scale) as usize,
            (1188.0 * scale) as usize,
            (1277.0 * scale) as usize,
            (1356.0 * scale) as usize,
            (1422.0 * scale) as usize,
            (1491.0 * scale) as usize,
            (1557.0 * scale) as usize,
            (1617.0 * scale) as usize,
        ];

        let allpass_delays = vec![
            (556.0 * scale) as usize,
            (441.0 * scale) as usize,
            (341.0 * scale) as usize,
            (225.0 * scale) as usize,
        ];

        EffectsProcessor {
            sample_rate,
            comb_buffers: comb_delays.iter()
                .map(|&size| VecDeque::from(vec![0.0; size]))
                .collect(),
            comb_filter_state: vec![0.0; 8],
            allpass_buffers: allpass_delays.iter()
                .map(|&size| VecDeque::from(vec![0.0; size]))
                .collect(),
            delay_buffer: VecDeque::from(vec![0.0; (sample_rate * 2.0) as usize]),
            lowpass_state: 0.0,
            filter_state: (0.0, 0.0),
        }
    }

    pub fn process(&mut self, input: f32, effects: &EffectsChain) -> f32 {
        let mut output = input;

        // Apply filter first in the chain for cleaner frequency shaping
        if let Some(filter) = &effects.filter {
            output = self.apply_filter(output, filter);
        }

        if let Some(dist) = &effects.distortion {
            output = self.apply_distortion(output, dist);
        }

        if let Some(delay) = &effects.delay {
            output = self.apply_delay(output, delay);
        }

        if let Some(reverb) = &effects.reverb {
            output = self.apply_reverb(output, reverb);
        }

        output
    }

    // Biquad filter implementation for lowpass/highpass/bandpass
    fn apply_filter(&mut self, input: f32, params: &FilterParams) -> f32 {
        let omega = std::f32::consts::TAU * params.cutoff / self.sample_rate;
        let alpha = omega.sin() * params.resonance;
        
        // Calculate biquad coefficients based on filter type
        let (b0, b1, b2, a0, a1, a2) = match params.filter_type {
            FilterType::LowPass => {
                let cos_omega = omega.cos();
                (
                    (1.0 - cos_omega) / 2.0,
                    1.0 - cos_omega,
                    (1.0 - cos_omega) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
            FilterType::HighPass => {
                let cos_omega = omega.cos();
                (
                    (1.0 + cos_omega) / 2.0,
                    -(1.0 + cos_omega),
                    (1.0 + cos_omega) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
            FilterType::BandPass => {
                let cos_omega = omega.cos();
                (
                    alpha,
                    0.0,
                    -alpha,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
        };

        // y[n] = (b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]) / a0
        let output = (b0 * input + b1 * self.filter_state.0 + b2 * self.filter_state.1
            - a1 * self.filter_state.0 - a2 * self.filter_state.1) / a0;

        self.filter_state.1 = self.filter_state.0;
        self.filter_state.0 = output;

        output
    }

    fn apply_distortion(&mut self, input: f32, params: &DistortionParams) -> f32 {
        let driven = input * params.drive;
        let distorted = if driven > 1.0 {
            2.0 / 3.0
        } else if driven < -1.0 {
            -2.0 / 3.0
        } else {
            driven - (driven.powi(3) / 3.0)
        };

        let alpha = 1.0 - params.tone;
        self.lowpass_state = self.lowpass_state * alpha + distorted * (1.0 - alpha);

        input * (1.0 - params.wet) + self.lowpass_state * params.wet
    }

    fn apply_delay(&mut self, input: f32, params: &DelayParams) -> f32 {
        let delay_samples = (params.time * self.sample_rate) as usize;
        let delay_samples = delay_samples.min(self.delay_buffer.len() - 1);

        let delayed = self.delay_buffer[delay_samples];

        Self::cycle_buffer(&mut self.delay_buffer, input + delayed * params.feedback);

        input * (1.0 - params.wet) + delayed * params.wet
    }

    fn apply_reverb(&mut self, input: f32, params: &ReverbParams) -> f32 {
        let mut output = 0.0;

        for i in 0..8 {
            let delayed = self.comb_buffers[i].back().copied().unwrap_or(0.0);
            
            self.comb_filter_state[i] = delayed * (1.0 - params.damping) + 
                                        self.comb_filter_state[i] * params.damping;
            
            let feedback = self.comb_filter_state[i] * params.room_size;
            
            Self::cycle_buffer(&mut self.comb_buffers[i], input + feedback);
            
            output += delayed;
        }

        output /= 8.0;

        for buffer in &mut self.allpass_buffers {
            let delayed = buffer.back().copied().unwrap_or(0.0);
            let new_val = output + delayed * 0.5;
            Self::cycle_buffer(buffer, new_val);
            output = delayed - output * 0.5;
        }

        input * (1.0 - params.wet) + output * params.wet
    }

    #[inline]
    fn cycle_buffer(buffer: &mut VecDeque<f32>, new_value: f32) {
        buffer.pop_back();
        buffer.push_front(new_value);
    }
}