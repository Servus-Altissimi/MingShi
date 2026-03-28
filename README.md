<div align="center">
  <table border="0" cellspacing="0" cellpadding="0">
    <tr>
      <td><img src="https://files.catbox.moe/qxf2b3.svg" alt="MingShi Logo" height="72" /></td>
      <td><h1>&nbsp;MingShi</h1></td>
    </tr>
  </table>
  <p><em>Compose, sculpt, and play music in real time!</em></p>

  [![Crates.io](https://img.shields.io/crates/v/mingshi?style=for-the-badge&color=a855f7)](https://crates.io/crates/mingshi)
  [![Downloads](https://img.shields.io/crates/d/mingshi?style=for-the-badge&label=downloads&color=ec4899)](https://crates.io/crates/mingshi)
  [![License](https://img.shields.io/badge/license-MIT-green?style=for-the-badge)](LICENSE)
  [![Rust](https://img.shields.io/badge/rust-1.89+-orange?style=for-the-badge)](https://www.rust-lang.org/)

</div>

## Roadmap
- [x] WGSL Shader support
- [x] WASM Support
- [ ] MIDI support

## Why?

This project originally came to be for my game engine "Liefde". This project has long been discontinued however. Now it is meant to be an extensive synthesizer for any audio project. Most new features will be targeted towards supplementing my wip audiovisitory tool [UmbraFex](https://github.com/servus-altissimi/umbrafex)

## Features

### Audio Synthesis
- **Waveform types**: Sine, Square, Triangle, Sawtooth, and Noise
- **Sample based playback**: Load and play WAV files with pitch adjustment and interpolation
- **ADSR envelope shaping**: Full Attack, Decay, Sustain, Release control per instrument
- **Real-time synthesis**: Low-latency audio output using `cpal`
- **Chord support**: Play multiple notes at once
- **Pitch slides**: Smooth pitch transitions between individual notes
- **Per-note parameters**: Individual pan and slide control for each note

### Effects Processing
- **Reverb**: Freeverb based algorithm with room size, damping, wet/dry mix, and stereo width controls
- **Delay**: Configurable delay time, feedback, and wet/dry mix with feedback loop
- **Distortion**: Waveshaping distortion with drive, tone control (lowpass filtering), and wet/dry mix
- **Filters**: Biquad filters supporting lowpass, highpass, and bandpass modes with cutoff and resonance control
- **Effects chain**: Process audio through multiple effects in sequence

### GPU Acceleration
- **WGPU-powered synthesis**: Offload waveform generation (Sine, Square, Triangle, Sawtooth) to the GPU 
- **Custom WGSL shaders**: Load and dispatch shaders for fully custom synthesis pipelines
- **Automatic CPU fallback**: Sample-based instruments, Noise, and all stateful effects run on the CPU silently

### Dynamic Playback Control
- **Real-time parameter adjustment**: Change volume, pitch, and track states during playback
- **Crossfading**: Smooth transitions between different arrangements
- **Track muting**: Enable/disable individual tracks on the fly
- **Looping**: Support for arrangement level and track level loop points
- **Fade in/out**: Automatic fade envelopes for arrangement start and end
- **Master controls**: Global volume and pitch adjustment
- **Parameter interpolation**: Gradual volume changes over time

## Installation

Add MingShi to your `Cargo.toml`:
```toml
[dependencies]
mingshi = "0.1"
```

Or add via cargo:
```bash
cargo add mingshi
```

## Example
```rust
use mingshi::*;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut engine = SynthEngine::new()?;
    
    // Load samples
    engine.load_sample("kick", "samples/kick.wav")?;
        
    // Play arrangement
    let arrangement = engine.load_arrangement("songs/song.bmi")?;
    engine.play_arrangement(arrangement)?;
    engine.set_loop_enabled(true);
    
    engine.set_master_volume(0.8);
    engine.set_track_volume("bass", 1.2);
    std::thread::sleep(std::time::Duration::from_secs(30));
    engine.stop();
    
    Ok(())
}
```

## API

### Core Functions

| Function | Description |
|----------|-------------|
| `SynthEngine::new()` | Create a new synthesizer engine with default audio device |
| `load_sample(name, path)` | Load a `.wav` file into the sample cache |
| `load_melody(name, path)` | Parse and cache a `.mel` file |
| `load_arrangement(path)` | Load a `.bmi` arrangement file |
| `get_sample_cache()` | Get reference to loaded samples |
| `play_arrangement(arrangement)` | Start playback of an arrangement |
| `stop()` | Stop playback and clean up audio stream |
| `pause()` | Pause playback without stopping |
| `resume()` | Resume paused playback |
| `synthesize_arrangement(arrangement)` | Render arrangement to audio buffer |

### Playback Control

| Function | Description |
|----------|-------------|
| `set_loop_enabled(enabled)` | Enable/disable looping |
| `crossfade_to(arrangement, duration)` | Smoothly transition to new arrangement |
| `get_playback_position()` | Get current playback time in seconds |
| `get_playback_state()` | Get current state: `Playing`, `Paused`, or `Stopped` |

### Dynamic Parameters

| Function | Description | Range |
|----------|-------------|-------|
| `set_master_volume(volume)` | Set global volume | 0.0-2.0 |
| `set_master_pitch(pitch)` | Set global pitch multiplier | 0.5-2.0 |
| `set_track_enabled(name, enabled)` | Toggle a specific track | boolean |
| `set_track_volume(name, volume)` | Set track volume | 0.0-2.0 |
| `interpolate_track_volume(name, target, duration)` | Gradual volume change over time | target: 0.0-2.0, duration: seconds |

## File Format Reference

### Melody File (`.mel`)

#### Metadata

| Parameter | Description | Default |
|-----------|-------------|---------|
| `name:` | Track name (used for identification) | `"melody"` |
| `tempo:` | BPM | `120` |
| `time_sig:` | Time signature as `numerator/denominator` | `4/4` |
| `swing:` | Swing feel | `0.0` (straight) |
| `loop:` | Loop points in seconds: `start, end` | none |

#### Instrument Configuration

| Parameter | Description | Values/Range |
|-----------|-------------|--------------|
| `waveform:` | Synthesized waveform type | `sine`, `square`, `triangle`, `sawtooth`, `noise` |
| `sample:` | Reference to loaded sample by name | sample name string |
| `volume:` | Base amplitude | 0.0-1.0+ |
| `pitch:` | Pitch multiplier | any float > 0 |
| `pan:` | Stereo position | -1.0 (left) to 1.0 (right) |
| `detune:` | Pitch offset in cents | any float |

#### ADSR Envelope

| Parameter | Description | Range |
|-----------|-------------|-------|
| `attack:` | Attack time in seconds | 0.0+ |
| `decay:` | Decay time in seconds | 0.0+ |
| `sustain:` | Sustain level | 0.0-1.0 |
| `release:` | Release time in seconds | 0.0+ |

#### Sequence Elements

**Notes:**
```
note: PITCH, DURATION, VELOCITY [, PARAMS...]
```

| Parameter | Description | Example |
|-----------|-------------|---------|
| `PITCH` | Note name | `C4`, `D#5`, `Gb3` |
| `DURATION` | Length in beats | `1.0`, `0.5`, `2.0` |
| `VELOCITY` | Note volume | `0.8` (0.0-1.0) |
| `pan=` | Override stereo position | `pan=0.5` |
| `slide=` | Pitch slide target note | `slide=E4` |

**Chords:**
```
chord: NOTE1+NOTE2+NOTE3, DURATION, VELOCITY
```

| Component | Description |
|-----------|-------------|
| Notes | Multiple notes separated by `+` |
| Duration | Length in beats (applies to entire chord) |
| Velocity | Volume (applies to entire chord) |

**Rests:**
```
rest: DURATION
```
Silence for specified duration in beats.

#### Effects

| Effect | Syntax | Parameters |
|--------|--------|------------|
| Filter | `filter: TYPE, CUTOFF, RESONANCE` | Type: `lowpass`/`lp`, `highpass`/`hp`, `bandpass`/`bp`<br>Cutoff: Hz<br>Resonance: Q factor (0.1-10.0) |
| Reverb | `reverb: ROOM_SIZE, DAMPING, WET, WIDTH` | All parameters: 0.0-1.0 |
| Delay | `delay: TIME, FEEDBACK, WET` | Time: seconds<br>Feedback: 0.0-1.0<br>Wet: 0.0-1.0 |
| Distortion | `distortion: DRIVE, TONE, WET` | Drive: 1.0+<br>Tone: 0.0-1.0<br>Wet: 0.0-1.0 |

#### Example
```
name: bass
tempo: 120
waveform: sawtooth
volume: 0.9
attack: 0.01
decay: 0.2
sustain: 0.7
release: 0.3
pan: -0.3

note: C2, 1.0, 0.8
note: E2, 0.5, 0.9, slide=G2
chord: C2+E2+G2, 2.0, 0.7
rest: 0.5

filter: lowpass, 800, 0.5
reverb: 0.3, 0.4, 0.2, 0.9
```

### Bundles Music Index files/Arrangements (`.bmi`)

#### Metadata

| Parameter | Description | Default |
|-----------|-------------|---------|
| `name:` | Arrangement name | `"song"` |
| `master_tempo:` | Override tempo for all tracks | none |
| `fade_in:` | Fade in duration in seconds | none |
| `fade_out:` | Fade out duration in seconds | none |
| `loop:` | Arrangement loop points: `start, end` | none |

#### Tracks
```
track: MELODY_FILE, START_TIME [, OVERRIDES...]
```

| Component | Description |
|-----------|-------------|
| `MELODY_FILE` | Name of cached melody file |
| `START_TIME` | When track begins (seconds) |
| `OVERRIDES` | Optional parameter overrides |

#### Track Override Parameters

| Override | Syntax | Description |
|----------|--------|-------------|
| Volume | `volume=0.8` or `vol=0.8` | Track volume multiplier |
| Pitch | `pitch=1.2` | Pitch multiplier |
| Tempo | `tempo=140` | Override track tempo |
| Pan | `pan=0.5` | Override pan position |
| Filter | `filter=TYPE:CUTOFF:RESONANCE` | Add/override filter |
| Reverb | `reverb=ROOM:DAMP:WET:WIDTH` | Add/override reverb |
| Delay | `delay=TIME:FEEDBACK:WET` | Add/override delay |
| Distortion | `distortion=DRIVE:TONE:WET` or `dist=...` | Add/override distortion |

#### Example
```
name: Song
master_tempo: 120
fade_in: 2.0
fade_out: 3.0

track: bass.mel, 0.0, volume=1.2
track: melody.mel, 2.0, pitch=1.0, reverb=0.6:0.5:0.3:1.0
track: drums.mel, 4.0, filter=lowpass:800:0.5
track: kick.mel, 8.0, dist=3.0:0.8:0.7, pan=0.3

loop: 0.0, 16.0
```

## Note Parsing

Notes follow standard music notations:

| Component | Description | Examples |
|-----------|-------------|----------|
| **Base notes** | C, D, E, F, G, A, B | `C`, `D`, `E` |
| **Sharps** | `#` or `S` suffix | `C#4`, `DS5` |
| **Flats** | `b`, `F`, or `B` suffix | `Db3`, `EF4`, `GB2` |
| **Octaves** | Number suffix (C4 = middle C) | `C4`, `A3`, `E5` |

**Frequency calculation:**
- Base frequencies start at C0 = 16.35 Hz
- Each semitone multiplies by 2^(1/12)
- Each octave doubles the frequency

## Details

### Audio Engine

| Feature | Implementation |
|---------|----------------|
| **Backend** | `cpal` for cross-platform audio |
| **Sample rate** | System default (typically 44.1kHz or 48kHz) |
| **Bit depth** | 32-bit float processing |

### Effects Implementation Details

| Effect | Algorithm |
|--------|-----------|
| **Reverb** | Freeverb with 8 comb filters and 4 allpass filters |
| **Delay** | Circular buffer with feedback loop |
| **Distortion** | Cubic waveshaping with tone control lowpass filter |
| **Filters** | Biquad IIR filters with proper coefficient calculation |

## GPU Acceleration

MingShi includes an optional GPU-accelerated synthesis engine powered by [WGPU](https://wgpu.rs/). Enable it with the `gpu` feature flag:
```toml
[dependencies]
mingshi = { version = "0.1", features = ["gpu"] }
```

### Architecture

| Layer | Responsibility |
|-------|----------------|
| **GPU path** | Synthesized waveforms (Sine, Square, Triangle, Sawtooth) |
| **CPU fallback** | Sample-based instruments and the Noise waveform |
| **CPU post-process** | Stateful effects (reverb, delay, distortion, filter) |
| **CPU post-process** | Arrangement-level fade and normalisation |

### GpuSynthEngine API

| Function | Description |
|----------|-------------|
| `GpuSynthEngine::new()` | Initialise wgpu instance, adapter, and device (high-performance preference) |
| `load_sample(name, path)` | Load a `.wav` file (delegates to CPU cache) |
| `load_melody(name, path)` | Parse and cache a `.mel` file |
| `load_arrangement(path)` | Load a `.bmi` arrangement file |
| `get_sample_cache()` | Get reference to loaded samples |
| `synthesize_arrangement(arrangement)` | Render arrangement to `Vec<f32>` using GPU where possible |
| `synthesize_arrangement_with_params(arrangement, params)` | Same, with runtime `DynamicParameters` |
| `synthesize_audio_shader(name, samples, rate, duration)` | Dispatch a raw named WGSL shader and return stereo `(left, right)` buffers |

### Custom Shaders

Shaders can be loaded from disk and referenced by name. The built-in fallback shader (`__boomie_synth`) is auto-generated from the active waveform set. The built-in name can be shadowed by loading a custom shader under `DEFAULT_SYNTH_SHADER_NAME`.

Shaders dispatched via `synthesize_audio_shader` must expose the `AudioUniforms` binding contract described below.

### AudioUniforms Binding Contract

Custom shaders must bind a uniform buffer at `@group(0) @binding(0)` matching this layout:

| Field | Type | Description |
|-------|------|-------------|
| `duration` | `f32` | Total duration in seconds |
| `sample_rate` | `f32` | Sample-rate in Hz |
| `num_samples` | `u32` | Total number of samples to generate |
| `_pad` | `f32` | Padding for alignment |

The storage output buffer must be bound at `@group(0) @binding(1)` as a read/write `array<f32>`, sized to at least `num_samples * 2` elements.

### Waveform GPU Eligibility

| Waveform | GPU Path | Notes |
|----------|----------|-------|
| Sine | Yes | |
| Square | Yes | |
| Triangle | Yes | |
| Sawtooth | Yes | |
| Noise | No | Stateful RNG has no GPU equivalent as far as I know|
| Sample-based | No | CPU only |
