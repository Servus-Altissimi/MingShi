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
- [x] MIDI support

## Why?

This project originally came to be for my game engine "Liefde". This project has long been discontinued however. Now it is meant to be an extensive synthesizer for any audio project. Most new features will be targeted towards supplementing my wip audiovisual tool [UmbraFex](https://github.com/servus-altissimi/umbrafex)

There's also a web demo made with Dioxus available at [audio.constringo.com](https://audio.constringo.com)!

## Features

- **Audio Synthesis**: Sine, Square, Triangle, Sawtooth, and Noise waveforms. Sample-based playback with pitch adjustment and interpolation. Full ADSR envelope shaping per instrument, chord support, pitch slides, and per-note pan control. Real-time low-latency output via `cpal`.

- **Effects**: Freeverb-based reverb, delay with feedback, waveshaping distortion with tone control, and biquad filters (lowpass, highpass, bandpass). Effects are chained and applied in sequence.

- **GPU Acceleration**: Offload waveform generation to the GPU via WGPU. Custom WGSL shaders supported for fully custom synthesis pipelines. Sample-based instruments, Noise, and stateful effects fall back to CPU automatically.

- **Dynamic Playback**: Change volume, pitch, and track states during playback. Crossfading between arrangements, track muting, looping, fade in/out, and gradual parameter interpolation.

- **MIDI**: Load and play Standard MIDI Files (Format 0, 1, 2). General MIDI program numbers are mapped to appropriate waveforms and ADSR settings automatically.

## Installation

```toml
[dependencies]
mingshi = "0.1"
```

Or via cli:
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
| `load_midi(name_prefix, path)` | Parse a MIDI file and cache its tracks |
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

---

## File Format Reference

### Melody File (`.mel`)

Melody files define a single instrument track. Metadata, instrument settings, effects, and the note sequence all live in the same file.

**Metadata**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `name:` | Track name (used for identification) | `"melody"` |
| `tempo:` | BPM | `120` |
| `time_sig:` | Time signature as `numerator/denominator` | `4/4` |
| `swing:` | Swing feel | `0.0` (straight) |
| `loop:` | Loop points in seconds: `start, end` | none |

**Instrument**

| Parameter | Description | Values |
|-----------|-------------|--------|
| `waveform:` | Synthesized waveform type | `sine`, `square`, `triangle`, `sawtooth`, `noise` |
| `sample:` | Reference to a loaded sample by name | sample name string |
| `volume:` | Base amplitude | 0.0-1.0+ |
| `pitch:` | Pitch multiplier | any float > 0 |
| `pan:` | Stereo position | -1.0 (left) to 1.0 (right) |
| `detune:` | Pitch offset in cents | any float |
| `attack:` | Attack time in seconds | 0.0+ |
| `decay:` | Decay time in seconds | 0.0+ |
| `sustain:` | Sustain level | 0.0-1.0 |
| `release:` | Release time in seconds | 0.0+ |

**Sequence**

Notes:
```
note: PITCH, DURATION, VELOCITY [, pan=FLOAT] [, slide=NOTE]
```

Chords (notes separated by `+`):
```
chord: NOTE1+NOTE2+NOTE3, DURATION, VELOCITY
```

Rests:
```
rest: DURATION
```

Durations are in beats. Velocity is 0.0-1.0.

**Effects**

| Effect | Syntax | Parameters |
|--------|--------|------------|
| Filter | `filter: TYPE, CUTOFF, RESONANCE` | Type: `lowpass`/`lp`, `highpass`/`hp`, `bandpass`/`bp`; Cutoff in Hz; Resonance is Q factor |
| Reverb | `reverb: ROOM_SIZE, DAMPING, WET, WIDTH` | All 0.0-1.0 |
| Delay | `delay: TIME, FEEDBACK, WET` | Time in seconds |
| Distortion | `distortion: DRIVE, TONE, WET` | Drive: 1.0+ |

**Example**
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

---

### Arrangement File (`.bmi`)

Arrangement files combine multiple melody tracks with timing, per-track overrides, and global settings.

**Metadata**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `name:` | Arrangement name | `"song"` |
| `master_tempo:` | Override tempo for all tracks | none |
| `fade_in:` | Fade in duration in seconds | none |
| `fade_out:` | Fade out duration in seconds | none |
| `loop:` | Arrangement loop points: `start, end` | none |

**Tracks**
```
track: MELODY_FILE, START_TIME [, OVERRIDES...]
```

`MELODY_FILE` is the name the file was cached under. `START_TIME` is in seconds. Overrides are optional and space-separated.

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

**Example**
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

---

### MIDI

MingShi can load Standard MIDI Files directly. Tracks are parsed and converted into `MelodyTrack`s that work the same as any `.mel` file.

```rust
let mut engine = SynthEngine::new()?;

// Single-track MIDI (Format 0), cached under "my_song"
let keys = engine.load_midi("my_song", "song.mid")?;

// Multi-track MIDI (Format 1/2), cached as "song_track_0", "song_track_1", etc.
let keys = engine.load_midi("song", "multitrack.mid")?;

// Use the returned keys in an arrangement or load a .bmi that references them
let arrangement = engine.load_arrangement("song.bmi")?;
engine.play_arrangement(arrangement)?;
```

`load_midi` returns a `Vec<String>` of the cache keys it registered, so you can reference them directly if you're building arrangements in code.

General MIDI program numbers are automatically mapped to appropriate waveforms and ADSR settings. The mapping is approximate but covers all 128 GM programs.

**What's supported:** Format 0, 1, and 2; running status; velocity-0 note-off; SysEx skip; all meta events; tempo changes and time signature events.

**What's not supported:** SMPTE timecode division; pitch bend, aftertouch, and CC messages (filtered out); percussion channel 9 is parsed as pitched notes rather than drums.

All tracks are output at a fixed 120 BPM internally, with real timing preserved via tempo map conversion.

---

## Note Parsing

Notes follow standard music notation. Sharps can be written as `#` or `S`, flats as `b`, `F`, or `B`. Octave numbers follow the note name, with C4 being middle C. Frequency is calculated using the standard MIDI formula: `f = 440 × 2^((n - 69) / 12)`.

Examples: `C4`, `D#5`, `Gb3`, `EB2`, `A3`

---

## GPU Acceleration

Enable with the `gpu` feature flag:
```toml
[dependencies]
mingshi = { version = "0.1", features = ["gpu"] }
```

The `GpuSynthEngine` mirrors most of `SynthEngine`'s API but dispatches waveform synthesis to the GPU via WGPU. Sample-based instruments, the Noise waveform, and all stateful effects always run on the CPU. Arrangement-level fading and normalization also happen on the CPU after readback.

| Waveform | GPU Path |
|----------|----------|
| Sine | Yes |
| Square | Yes |
| Triangle | Yes |
| Sawtooth | Yes |
| Noise | No |
| Sample-based | No |

### GpuSynthEngine API

| Function | Description |
|----------|-------------|
| `GpuSynthEngine::new()` | Initialise wgpu instance, adapter, and device (high-performance preference) |
| `load_sample(name, path)` | Load a `.wav` file (delegates to CPU cache) |
| `load_melody(name, path)` | Parse and cache a `.mel` file |
| `load_arrangement(path)` | Load a `.bmi` arrangement file |
| `synthesize_arrangement(arrangement)` | Render arrangement to `Vec<f32>` using GPU where possible |
| `synthesize_arrangement_with_params(arrangement, params)` | Same, with runtime `DynamicParameters` |
| `synthesize_audio_shader(name, samples, rate, duration)` | Dispatch a raw named WGSL shader and return stereo `(left, right)` buffers |

### Custom Shaders

Shaders can be loaded from disk and referenced by name:
```rust
engine.load_shader("synth", "shaders/synth.wgsl")?;
```

The built-in fallback shader (`__mingshi_synth`) is auto-generated from the active waveform set and can be shadowed by loading a custom shader under `DEFAULT_SYNTH_SHADER_NAME`.

Shaders dispatched via `synthesize_audio_shader` must expose the `AudioUniforms` binding at `@group(0) @binding(0)` and a r/w `array<f32>` output at `@group(0) @binding(1)`, sized to at least `num_samples * 2`.

| Field | Type | Description |
|-------|------|-------------|
| `duration` | `f32` | Total duration in seconds |
| `sample_rate` | `f32` | Sample rate in Hz |
| `num_samples` | `u32` | Total number of samples to generate |
| `_pad` | `f32` | Padding for alignment |

---
