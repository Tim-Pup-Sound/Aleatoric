# Aleatoric

A small Rust program that generates random ("aleatoric") music: it picks a song
structure, chord loops, a key, a tempo and a melody, then performs the result
with sawtooth waves. By default it plays through the speakers; with
`--output FILE.wav` it writes a mono / 48000 sps / 16-bit WAV file instead.

## Build and run

```sh
cargo build --release

# Play a random song on the speakers:
cargo run

# Write a WAV instead of playing:
cargo run -- --output ALEATORIC.wav

# Reproduce a specific song and enable the extras:
cargo run -- --seed 42 --bass --harmony --rhythm --drums -o ALEATORIC.wav
```

## Options

| Flag                | Effect                                                           |
| ------------------- | ---------------------------------------------------------------- |
| `-o, --output FILE` | write a mono 48000sps 16-bit WAV instead of playing live         |
| `--seed N`          | seed the RNG so a run is reproducible                            |
| `--bass`            | bass line: chord root, two octaves down, held a whole measure    |
| `--harmony`         | add the closest chord note below each melody note                |
| `--rhythm`          | random rhythmic pattern per verse / chorus instead of eighths    |
| `--drums`           | white-noise percussion using one fixed one-measure pattern       |
| `-h, --help`        | usage                                                            |

## Structure

- **Song structure**: one of `AABB/CC`, `ABAB/CD`, `AB/CDDD` chosen at random.
  Each letter is a "line" (a four-chord loop); repeated letters reproduce the
  exact same line. The `/` separates verse from chorus (only used by `--rhythm`).
- **Line structure**: each distinct letter is assigned a random four-chord loop
  from the ten candidates, sampled without replacement so no two letters share a
  loop.
- **Key**: a random root note in A3..A4 inclusive (MIDI 57..69).
- **Tempo**: a random 80..160 bpm, common time (4 beats / measure, 8 eighth
  notes / measure).
- **Melody**: per note, with probability 0.8 a tone of the current chord, else
  another major-scale tone; all notes folded into the first octave of the key..

## Crates used

- `hound` - WAV encoding
- `rodio` - live audio playback
- `rand` - randomness

## How it went

It went smoothly. The mixing-buffer design (everything adds into one `Vec<f32>`)
made the bonus tracks - bass, harmony, rhythm patterns and drums - easy to layer
on top of the core melody. Short per-note fades remove the clicks.