use std::collections::HashMap;
use crate::error::SynthError;
use crate::track::{MelodyTrack, LoopPoint};
use crate::effects::{ReverbParams, DelayParams, DistortionParams, FilterParams, FilterType};

#[derive(Debug, Clone, Default)]
pub struct TrackOverrides {
    pub volume: Option<f32>,
    pub pitch: Option<f32>,
    pub tempo: Option<f32>,
    pub pan: Option<f32>,
    pub reverb: Option<ReverbParams>,
    pub delay: Option<DelayParams>,
    pub distortion: Option<DistortionParams>,
    pub filter: Option<FilterParams>,
}

#[derive(Debug, Clone)]
pub struct Arrangement {
    pub name: String,
    pub tracks: Vec<(MelodyTrack, f32, TrackOverrides)>, // TrackOverrides
    pub total_length: f32,
    pub loop_point: Option<LoopPoint>,
    pub master_tempo: Option<f32>,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
}

impl Arrangement {
    pub fn from_bmi(content: &str, mel_cache: &HashMap<String, MelodyTrack>) -> Result<Self, SynthError> {
        let mut arrangement = Arrangement {
            name: "song".to_string(),
            tracks: Vec::new(),
            total_length: 0.0,
            loop_point: None,
            master_tempo: None,
            fade_in: None,
            fade_out: None,
        };

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            if let Some(value) = line.strip_prefix("name:") {
                arrangement.name = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("master_tempo:") {
                arrangement.master_tempo = value.trim().parse().ok();
            } else if let Some(value) = line.strip_prefix("fade_in:") {
                arrangement.fade_in = value.trim().parse().ok();
            } else if let Some(value) = line.strip_prefix("fade_out:") {
                arrangement.fade_out = value.trim().parse().ok();
            } else if let Some(value) = line.strip_prefix("loop:") {
                let parts: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    arrangement.loop_point = Some(LoopPoint {
                        start: parts[0].parse().unwrap_or(0.0),
                        end: parts[1].parse().unwrap_or(arrangement.total_length),
                    });
                }
            } else if let Some(value) = line.strip_prefix("track:") {
                let parts: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 2 {
                    let mel_file = parts[0];
                    let start_time: f32 = parts[1].parse()
                        .map_err(|_| SynthError::ParseError("Invalid start time".to_string()))?;
                    
                    let mut overrides = TrackOverrides::default();
                    
                    for override_str in parts.iter().skip(2) {
                        if let Some((key, val)) = override_str.split_once('=') {
                            let key = key.trim();
                            let val = val.trim();
                            
                            match key {
                                "volume" | "vol" => {
                                    overrides.volume = val.parse().ok();
                                }
                                "pitch" => {
                                    overrides.pitch = val.parse().ok();
                                }
                                "tempo" => {
                                    overrides.tempo = val.parse().ok();
                                }
                                "pan" => { 
                                    overrides.pan = val.parse().ok();
                                }
                                "filter" => {
                                    let vals: Vec<&str> = val.split(':').collect();
                                    if vals.len() >= 3 {
                                        let filter_type = match vals[0].to_lowercase().as_str() {
                                            "lowpass" | "lp" => FilterType::LowPass,
                                            "highpass" | "hp" => FilterType::HighPass,
                                            "bandpass" | "bp" => FilterType::BandPass,
                                            _ => FilterType::LowPass,
                                        };
                                        overrides.filter = Some(FilterParams {
                                            filter_type,
                                            cutoff: vals[1].parse().unwrap_or(1000.0),
                                            resonance: vals[2].parse().unwrap_or(0.7),
                                        });
                                    }
                                }
                                "reverb" => {
                                    let vals: Vec<&str> = val.split(':').collect();
                                    if vals.len() >= 4 {
                                        overrides.reverb = Some(ReverbParams {
                                            room_size: vals[0].parse().unwrap_or(0.5),
                                            damping: vals[1].parse().unwrap_or(0.5),
                                            wet: vals[2].parse().unwrap_or(0.3),
                                            width: vals[3].parse().unwrap_or(1.0),
                                        });
                                    }
                                }
                                "delay" => {
                                    let vals: Vec<&str> = val.split(':').collect();
                                    if vals.len() >= 3 {
                                        overrides.delay = Some(DelayParams {
                                            time: vals[0].parse().unwrap_or(0.25),
                                            feedback: vals[1].parse().unwrap_or(0.4),
                                            wet: vals[2].parse().unwrap_or(0.3),
                                        });
                                    }
                                }
                                "distortion" | "dist" => {
                                    let vals: Vec<&str> = val.split(':').collect();
                                    if vals.len() >= 3 {
                                        overrides.distortion = Some(DistortionParams {
                                            drive: vals[0].parse().unwrap_or(2.0),
                                            tone: vals[1].parse().unwrap_or(0.7),
                                            wet: vals[2].parse().unwrap_or(0.5),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    
                    if let Some(track) = mel_cache.get(mel_file) {
                        let mut modified_track = track.clone();
                        
                        if overrides.tempo.is_some() {
                            modified_track.tempo = overrides.tempo.unwrap();
                        }
                        
                        if let Some(master_tempo) = arrangement.master_tempo {
                            modified_track.tempo = master_tempo;
                        }
                        
                        arrangement.tracks.push((modified_track, start_time, overrides));
                        let end_time = start_time + track.length;
                        if end_time > arrangement.total_length {
                            arrangement.total_length = end_time;
                        }
                    } else {
                        eprintln!("Warning: Track not found in cache: \'{}\' Skipping track", mel_file);
                    }
                }
            }
        }
        // Return error only if the arrangement has no valid tracks
        if arrangement.tracks.is_empty() {
            return Err(SynthError::InvalidInstrument(
                "Arrangement has no valid tracks".to_string()
            ));
        }

        Ok(arrangement)
    }
}