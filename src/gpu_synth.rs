// GPU-accelerated-synthesis for Boomie! This is a very very very niche feature, and mostly
// tailored towards UmbraFex (See README). Functionality is partially inspired on that of ShaderToy.
// There aren't a lot of open-source examples to work with.
// I also heavily utilized Sonnet 4.6, though I tried to keep it clean.

// Architecture:
//   - GpuSynthEngine:  public API: mirrors SynthEngine but dispatches to wgpu compute.
//   - AudioUniforms:   public struct
//   - GpuNoteData      private per-note buffer uploaded to the GPU per track.
//   - SynthUniforms    private uniforms for the synth shader.
//
// Shader loading keeps the Boomie spirit:
//   - gpu.load_shader("synth", "shaders/synth.wgsl")
//   - The engine looks up synth (or the built-in fallback) when synthesizing.
//
// Built-in fallback shader (DEFAULT_SYNTH_SHADER_NAME):
//   - The waveform `wave()` switch is assembled from WaveformType::wgsl_case() snippets
//   - so adding a waveform only requires touching waveform.rs.
//
// CPU & GPU parity strategy:
//   - Synthesized instruments (non-Noise waveforms) -> GPU path.
//   - Sample-based instruments and Noise waveform   -> CPU fallback.
//   - Stateful effects                              -> always CPU-side post-readback.
//   - Arrangement-level fade & normalisation        -> CPU-side.

use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;

use bytemuck::{Pod, Zeroable};
use futures_channel::mpsc;
use wgpu::util::DeviceExt;

use crate::arrangement::Arrangement;
use crate::effects::EffectsProcessor;
use crate::engine::{DynamicParameters, SynthEngine};
use crate::error::SynthError;
use crate::instrument::{InstrumentSource, SampleData, SequenceElement};
use crate::track::MelodyTrack;
use crate::waveform::WaveformType;


#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct AudioUniforms {
    pub duration: f32,
    pub sample_rate: f32,
    pub num_samples: u32,
    pub _pad: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuNoteData {
    start_sample: u32,
    end_sample: u32,
    pitch: f32,
    velocity: f32,
    volume: f32,
    attack_samples: u32,
    decay_samples: u32,
    sustain_level: f32,
    release_start: u32,
    release_samples: u32,
    waveform_type: u32,
    slide_to_pitch: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SynthUniforms {
    sample_rate: f32,
    num_samples: u32,
    num_notes: u32,
    _pad: f32,
}


// Name for the built-in synth shader is registered in the cache, can be shadowed with `load_shader(DEFAULT_SYNTH_SHADER_NAME, PATH)`.
pub const DEFAULT_SYNTH_SHADER_NAME: &str = "__boomie_synth";

fn build_default_synth_shader(used_waveforms: &[WaveformType]) -> String {
    let mut seen = std::collections::HashSet::new();
    let cases: String = used_waveforms
        .iter()
        .filter(|wf| seen.insert(wf.gpu_id()))
        .filter_map(|wf| wf.wgsl_case())
        .map(|c| format!("        {c}\n"))
        .collect();

    format!(r#"
// Hecko (˶ᵔ ᵕ ᵔ˶)

struct SynthUniforms {{
    sample_rate: f32,
    num_samples: u32,
    num_notes:   u32,
    _pad:        f32,
}}

struct NoteData {{
    start_sample:    u32,
    end_sample:      u32,
    pitch:           f32,
    velocity:        f32,
    volume:          f32,
    attack_samples:  u32,
    decay_samples:   u32,
    sustain_level:   f32,
    release_start:   u32,
    release_samples: u32,
    waveform_type:   u32,
    slide_to_pitch:  f32,
}}

@group(0) @binding(0) var<uniform>             u:      SynthUniforms;
@group(0) @binding(1) var<storage, read>       notes:  array<NoteData>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

const TAU: f32 = 6.28318530718;

fn adsr(i: u32, note: NoteData) -> f32 {{
    let t   = i - note.start_sample;
    let dur = note.end_sample - note.start_sample;
    if t >= dur {{ return 0.0; }}
    if note.attack_samples > 0u && t < note.attack_samples {{
        return f32(t) / f32(note.attack_samples);
    }}
    let decay_end = note.attack_samples + note.decay_samples;
    if note.decay_samples > 0u && t < decay_end {{
        let p = f32(t - note.attack_samples) / f32(note.decay_samples);
        return 1.0 - p * (1.0 - note.sustain_level);
    }}
    if i < note.release_start {{ return note.sustain_level; }}
    if note.release_samples == 0u {{ return 0.0; }}
    let rt = i - note.release_start;
    if rt >= note.release_samples {{ return 0.0; }}
    return note.sustain_level * (1.0 - f32(rt) / f32(note.release_samples));
}}

fn wave(phase: f32, wf: u32) -> f32 {{
    switch (wf) {{
{cases}        default: {{ return 0.0; }}
    }}
}}

@compute @workgroup_size(64)
fn synth_main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let i = gid.x;
    if i >= u.num_samples {{ return; }}

    var out = 0.0;
    for (var n = 0u; n < u.num_notes; n++) {{
        let note = notes[n];
        if i < note.start_sample || i >= note.end_sample {{ continue; }}

        let env      = adsr(i, note);
        let t_note= f32(i - note.start_sample) / u.sample_rate;
        let dur_note = f32(note.end_sample - note.start_sample) / u.sample_rate;

        var phase: f32;
        if note.slide_to_pitch == note.pitch || dur_note <= 0.0 {{
            phase = note.pitch * t_note;
        }} else {{
            let dp = note.slide_to_pitch - note.pitch;
            phase  = note.pitch * t_note + (dp / (2.0 * dur_note)) * t_note * t_note;
        }}

        out += wave(phase, note.waveform_type) * env * note.velocity * note.volume;
    }}
    output[i] = out;
}}
"#)
}

pub struct GpuSynthEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    cpu: SynthEngine,
    shader_cache: HashMap<String, String>,
    pub sample_rate: f32,
}

impl GpuSynthEngine {
    pub async fn new() -> Result<Self, SynthError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| SynthError::AudioError(format!("GPU adapter request failed: {e}")))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| SynthError::AudioError(format!("GPU device creation failed: {e}")))?;

        let sample_rate = 44_100.0f32;
        let cpu = SynthEngine::new_offline(sample_rate);

        Ok(GpuSynthEngine { device, queue, cpu, shader_cache: HashMap::new(), sample_rate })
    }

    // Can be considered redundant
    pub fn load_sample(&mut self, name: &str, path: &str) -> Result<(), Box<dyn Error>> {
        self.cpu.load_sample(name, path)
    }

    pub fn load_melody(&mut self, name: &str, path: &str) -> Result<(), Box<dyn Error>> {
        self.cpu.load_melody(name, path)
    }

    pub fn load_arrangement(&self, path: &str) -> Result<Arrangement, SynthError> {
        self.cpu.load_arrangement(path)
    }


    // Load a WGSL shader from `path` and cache it under `name`.
    pub fn load_shader(&mut self, name: &str, path: &str) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| SynthError::FileError(e.to_string()))?;
        self.shader_cache.insert(name.to_string(), source);
        Ok(())
    }

    // Return the WGSL source for `name`, or build the default synth shader if
    // `name == DEFAULT_SYNTH_SHADER_NAME` and nothing has been loaded for it.
    fn resolve_shader(&self, name: &str, waveforms: &[WaveformType]) -> String {
        if let Some(src) = self.shader_cache.get(name) {
            return src.clone();
        }
        build_default_synth_shader(waveforms) // Only the built-in name gets a generated fallback: anything else is a programming error that will surface as a wgpu compile failure.
    }

    // Raw shader synthesis!
    //  - Compile and dispatch a named (pre-loaded) WGSL compute shader.
    //  - The shader must satisfy the AudioUniforms binding contract (see struct docs).
    pub async fn synthesize_audio_shader(
        &self,
        shader_name: &str,
        sample_count: u32,
        sample_rate: f32,
        duration: f32,
    ) -> Result<(Vec<f32>, Vec<f32>), SynthError> {
        let source = self.shader_cache.get(shader_name)
            .ok_or_else(|| SynthError::FileError(format!("Shader not loaded: '{shader_name}'")))?
            .clone();

        let n = sample_count.max(64);
        let buf_size = n as u64 * 2 * std::mem::size_of::<f32>() as u64;

        let shader = self.compile_shader(&source).await
            .map_err(SynthError::ParseError)?;

        let uni_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("au_uni"),
            contents: bytemuck::cast_slice(&[AudioUniforms {
                duration, sample_rate, num_samples: n, _pad: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let storage_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("au_storage"),
            size: buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("au_staging"),
            size: buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bgl = self.make_bgl("au_bgl", &[
            bgl_entry(0, wgpu::BufferBindingType::Uniform),
            bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
        ]);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("au_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uni_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: storage_buf.as_entire_binding() },
            ],
        });

        let pipeline = self.make_compute_pipeline(&shader, "audio_main", &bgl, "au_cp");
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("au_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((n + 63) / 64, 1, 1);
        }
        enc.copy_buffer_to_buffer(&storage_buf, 0, &staging_buf, 0, buf_size);
        self.queue.submit(Some(enc.finish()));

        let floats = self.readback_buffer(&staging_buf, buf_size).await
            .map_err(SynthError::AudioError)?;

        Ok((floats[..n as usize].to_vec(), floats[n as usize..].to_vec()))
    }

    pub async fn synthesize_arrangement(
        &self,
        arrangement: &Arrangement,
    ) -> Result<Vec<f32>, SynthError> {
        self.synthesize_arrangement_with_params(arrangement, &DynamicParameters::default()).await
    }

    pub async fn synthesize_arrangement_with_params(
        &self,
        arrangement: &Arrangement,
        params: &DynamicParameters,
    ) -> Result<Vec<f32>, SynthError> {
        let total_samples = (arrangement.total_length * self.sample_rate) as usize;
        let mut buffer = vec![0.0f32; total_samples];

        for (track, start_time, overrides) in &arrangement.tracks {
            let enabled = params.track_enabled.get(&track.name).copied().unwrap_or(true);
            if !enabled { continue; }
            let track_vol = params.track_volumes.get(&track.name).copied().unwrap_or(1.0);
            let start_samp = (*start_time * self.sample_rate) as usize;

            let mut t = track.clone();
            if let Some(v) = overrides.volume     { t.instrument.volume = v; }
            if let Some(p) = overrides.pitch       { t.instrument.pitch  = p * params.master_pitch; }
            if let Some(tm) = overrides.tempo      { t.tempo = tm; }
            if let Some(r) = &overrides.reverb     { t.instrument.effects.reverb     = Some(r.clone()); }
            if let Some(d) = &overrides.delay      { t.instrument.effects.delay      = Some(d.clone()); }
            if let Some(x) = &overrides.distortion { t.instrument.effects.distortion = Some(x.clone()); }
            if let Some(f) = &overrides.filter     { t.instrument.effects.filter     = Some(f.clone()); }
            t.instrument.volume *= track_vol;

            let track_total = (t.length * self.sample_rate) as usize;
            if track_total == 0 { continue; }

            let mut track_buf = if Self::gpu_waveform(&t).is_some() {
                match self.synthesize_track_gpu(&t, track_total).await {
                    Ok(buf) => buf,
                    Err(_) => {
                        let mut buf = vec![0.0f32; track_total];
                        self.cpu.synthesize_track_into(&mut buf, &t, 0);
                        buf
                    }
                }
            } else {
                let mut buf = vec![0.0f32; track_total];
                self.cpu.synthesize_track_into(&mut buf, &t, 0);
                buf
            };

            if t.instrument.effects.has_any() {
                let mut fx = EffectsProcessor::new(self.sample_rate);
                for s in track_buf.iter_mut() {
                    *s = fx.process(*s, &t.instrument.effects);
                }
            }

            for (i, &s) in track_buf.iter().enumerate() {
                if let Some(dst) = buffer.get_mut(start_samp + i) {
                    *dst += s * params.master_volume;
                }
            }
        }

        if let Some(fi) = arrangement.fade_in {
            let n = (fi * self.sample_rate) as usize;
            for i in 0..n.min(buffer.len()) { buffer[i] *= i as f32 / n as f32; }
        }
        if let Some(fo) = arrangement.fade_out {
            let n = (fo * self.sample_rate) as usize;
            let fs = buffer.len().saturating_sub(n);
            for i in fs..buffer.len() { buffer[i] *= (buffer.len() - i) as f32 / n as f32; }
        }
        if let Some(max) = buffer.iter().map(|v| v.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()) {
            if max > 1.0 { buffer.iter_mut().for_each(|s| *s /= max); }
        }

        Ok(buffer)
    }

    fn gpu_waveform(track: &MelodyTrack) -> Option<WaveformType> {
        match &track.instrument.source {
            InstrumentSource::Sample(_) => None,
            InstrumentSource::Synthesized(wf) => wf.gpu_id().map(|_| *wf),
        }
    }

    async fn synthesize_track_gpu(
        &self,
        track: &MelodyTrack,
        total_samples: usize,
    ) -> Result<Vec<f32>, String> {
        let wf = Self::gpu_waveform(track).ok_or("Track is not GPU-eligible")?;
        let wf_id = wf.gpu_id().unwrap();
        let sr = self.sample_rate;
        let beat = 60.0 / track.tempo;

        let mut gpu_notes: Vec<GpuNoteData> = Vec::new();
        let mut offset = 0usize;

        for element in &track.sequence {
            match element {
                SequenceElement::Note(note) => {
                    let dur_n = (note.duration * beat * sr) as usize;
                    let start = offset;
                    let end = (start + dur_n).min(total_samples);
                    if start < end {
                        let (att, dec, rel, rel_st) = adsr_samps(&track.instrument, end, sr);
                        gpu_notes.push(GpuNoteData {
                            start_sample: start as u32,
                            end_sample: end as u32,
                            pitch: note.pitch,
                            velocity: note.velocity,
                            volume: track.instrument.volume,
                            attack_samples: att,
                            decay_samples: dec,
                            sustain_level: track.instrument.sustain,
                            release_start: rel_st,
                            release_samples: rel,
                            waveform_type: wf_id,
                            slide_to_pitch: note.slide_to.unwrap_or(note.pitch),
                        });
                    }
                    offset += dur_n;
                }
                SequenceElement::Chord(chord) => {
                    let dur_n = (chord.duration * beat * sr) as usize;
                    let start = offset;
                    let end = (start + dur_n).min(total_samples);
                    if start < end {
                        let (att, dec, rel, rel_st) = adsr_samps(&track.instrument, end, sr);
                        let vol_per = track.instrument.volume / chord.pitches.len() as f32;
                        for &pitch in &chord.pitches {
                            gpu_notes.push(GpuNoteData {
                                start_sample: start as u32,
                                end_sample: end as u32,
                                pitch,
                                velocity: chord.velocity,
                                volume: vol_per,
                                attack_samples: att,
                                decay_samples: dec,
                                sustain_level: track.instrument.sustain,
                                release_start: rel_st,
                                release_samples: rel,
                                waveform_type: wf_id,
                                slide_to_pitch: pitch,
                            });
                        }
                    }
                    offset += dur_n;
                }
                SequenceElement::Rest(d) => { offset += (d * beat * sr) as usize; }
            }
        }

        // Nothing to synth, billions must die
        if gpu_notes.is_empty() { return Ok(vec![0.0; total_samples]); }

        // Resolve shader: use whatever is cached under DEFAULT_SYNTH_SHADER_NAME,
        // or build the default from waveform snippets if nothing was loaded.
        let shader_src = self.resolve_shader(DEFAULT_SYNTH_SHADER_NAME, &[wf]);
        let shader = self.compile_shader(&shader_src).await?;

        let n = total_samples as u32;
        let out_bytes = n as u64 * std::mem::size_of::<f32>() as u64;

        let uni_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sy_uni"),
            contents: bytemuck::cast_slice(&[SynthUniforms {
                sample_rate: sr,
                num_samples: n,
                num_notes: gpu_notes.len() as u32,
                _pad: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let notes_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sy_notes"),
            contents: bytemuck::cast_slice(&gpu_notes),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sy_out"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sy_stage"),
            size: out_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bgl = self.make_bgl("sy_bgl", &[
            bgl_entry(0, wgpu::BufferBindingType::Uniform),
            bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: true  }),
            bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: false }),
        ]);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sy_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uni_buf.as_entire_binding()    },
                wgpu::BindGroupEntry { binding: 1, resource: notes_buf.as_entire_binding()  },
                wgpu::BindGroupEntry { binding: 2, resource: output_buf.as_entire_binding() },
            ],
        });

        let pipeline = self.make_compute_pipeline(&shader, "synth_main", &bgl, "sy_cp");
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sy_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((n + 63) / 64, 1, 1);
        }
        enc.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, out_bytes);
        self.queue.submit(Some(enc.finish()));

        self.readback_buffer(&staging_buf, out_bytes).await
    }

    async fn compile_shader(&self, source: &str) -> Result<wgpu::ShaderModule, String> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("boomie_sh"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source.to_string())),
        });
        let info = shader.get_compilation_info().await;
        let errs: Vec<String> = info.messages.iter()
            .filter(|m| m.message_type == wgpu::CompilationMessageType::Error)
            .map(|m| format!("line {}: {}", m.location.map_or(0, |l| l.line_number), m.message))
            .collect();
        let _ = scope.pop().await;
        if !errs.is_empty() { return Err(errs.join("\n")); }
        Ok(shader)
    }

    fn make_bgl(&self, label: &str, entries: &[wgpu::BindGroupLayoutEntry]) -> wgpu::BindGroupLayout {
        self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries,
        })
    }

    fn make_compute_pipeline(
        &self,
        shader: &wgpu::ShaderModule,
        entry: &str,
        bgl: &wgpu::BindGroupLayout,
        label: &str,
    ) -> wgpu::ComputePipeline {
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[bgl],
            immediate_size: 0,
        });
        self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            module: shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    }

    async fn readback_buffer(&self, staging: &wgpu::Buffer, size: u64) -> Result<Vec<f32>, String> {
        let (tx, mut rx) = mpsc::unbounded::<Result<(), wgpu::BufferAsyncError>>();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.unbounded_send(r); });

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.device.poll(wgpu::Maintain::Wait);
            match rx.try_recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(format!("Buffer map error: {e:?}")),
                Err(_) => return Err("Buffer map callback not received after Wait poll".into()),
            }
        }

        #[cfg(target_arch = "wasm32")]
        loop {
            match rx.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(e)) => return Err(format!("Buffer map error: {e:?}")),
                Err(_) => gloo_timers::future::TimeoutFuture::new(1).await,
            }
        }

        let view = staging.slice(..).get_mapped_range();
        let floats = bytemuck::cast_slice::<u8, f32>(&view).to_vec();
        drop(view);
        staging.unmap();
        Ok(floats)
    }

    pub fn shader_cache_insert(&mut self, name: &str, src: String) {
        self.shader_cache.insert(name.to_string(), src);
    }

    pub fn get_sample_cache(&self) -> &HashMap<String, SampleData> {
        self.cpu.get_sample_cache()
    }
}


fn bgl_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn adsr_samps(instr: &crate::instrument::Instrument, end: usize, sr: f32) -> (u32, u32, u32, u32) {
    let att = (instr.attack  * sr) as u32;
    let dec = (instr.decay   * sr) as u32;
    let rel = (instr.release * sr) as u32;
    let rel_st = end.saturating_sub(rel as usize) as u32;
    (att, dec, rel, rel_st)
}
