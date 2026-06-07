// Aleatoric - a random ("aleatoric") music generator.
//
// It picks a song structure, chord loops, a key, a tempo and a melody, then
// performs the result with sawtooth waves. By default it plays on the speakers;
// with --output FILE.wav it writes a mono 48000sps 16-bit WAV instead.

use std::env;
use std::process;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

const SAMPLE_RATE: u32 = 48_000;
const SR: f64 = SAMPLE_RATE as f64;

// Major-scale offsets
const MAJOR_SCALE: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

// The ten candidate four-chord loops, as roman-numeral strings.
const CHORD_LOOPS: [[&str; 4]; 10] = [
    ["I", "IV", "ii", "V"],
    ["I", "vi", "ii", "V"],
    ["I", "iii", "IV", "iv"],
    ["I", "V", "ii", "V"],
    ["I", "vi", "IV", "V"],
    ["IV", "I", "vi", "IV"],
    ["I", "V", "vi", "I"],
    ["I", "IV", "iv", "I"],
    ["IV", "V", "I", "I"],
    ["vi", "IV", "I", "V"],
];

// Song structures; the slash separates verse from chorus.
const SONG_STRUCTURES: [&str; 3] = ["AABB/CC", "ABAB/CD", "AB/CDDD"];

// Triad semitone offsets from the key root for each roman numeral.
fn chord_semitones(name: &str) -> [i32; 3] {
    match name {
        "I" => [0, 4, 7],
        "ii" => [2, 5, 9],
        "iii" => [4, 7, 11],
        "IV" => [5, 9, 12],
        "iv" => [5, 8, 12],
        "V" => [7, 11, 14],
        "vi" => [9, 12, 16],
        other => panic!("unknown chord: {other}"),
    }
}

fn midi_to_freq(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

struct Options {
    output: Option<String>,
    seed: Option<u64>,
    bass: bool,
    harmony: bool,
    rhythm: bool,
    drums: bool,
}

fn parse_args() -> Options {
    let mut opts = Options {
        output: None,
        seed: None,
        bass: false,
        harmony: false,
        rhythm: false,
        drums: false,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                opts.output = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--output requires a FILENAME.wav argument");
                    process::exit(2);
                }));
            }
            "--seed" => {
                let s = args.next().unwrap_or_else(|| {
                    eprintln!("--seed requires a number");
                    process::exit(2);
                });
                opts.seed = Some(s.parse().unwrap_or_else(|_| {
                    eprintln!("--seed must be an integer");
                    process::exit(2);
                }));
            }
            "--bass" => opts.bass = true,
            "--harmony" => opts.harmony = true,
            "--rhythm" => opts.rhythm = true,
            "--drums" => opts.drums = true,
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_help();
                process::exit(2);
            }
        }
    }
    opts
}

fn print_help() {
    eprintln!(
        "aleatoric - generate random sawtooth music\n\n\
         USAGE:\n    aleatoric [OPTIONS]\n\n\
         OPTIONS:\n\
         \x20   -o, --output FILE.wav   write a mono 48000sps 16-bit WAV instead of playing\n\
         \x20       --seed N            seed the RNG for reproducible output\n\
         \x20       --bass              add a bass line (chord root, two octaves down)\n\
         \x20       --harmony           add the closest chord note below each melody note\n\
         \x20       --rhythm            use random rhythmic patterns instead of plain eighths\n\
         \x20       --drums             add a white-noise percussion track\n\
         \x20   -h, --help             show this help"
    );
}

const FADE_SECS: f64 = 0.004; // short fade

fn add_sawtooth(buf: &mut [f32], start: usize, freq: f64, dur_samples: usize, amp: f32) {
    let fade = ((FADE_SECS * SR) as usize).min(dur_samples / 2).max(1);
    let inc = freq / SR;
    let mut phase = 0.0f64;
    for i in 0..dur_samples {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        phase += inc;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        let env = if i < fade {
            i as f32 / fade as f32
        } else if i >= dur_samples - fade {
            (dur_samples - i) as f32 / fade as f32
        } else {
            1.0
        };
        let saw = (2.0 * phase - 1.0) as f32;
        buf[idx] += amp * env * saw;
    }
}

fn add_noise(buf: &mut [f32], start: usize, dur_samples: usize, amp: f32, rng: &mut StdRng) {
    // White-noise burst with a fast exponential decay.
    for i in 0..dur_samples {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        let env = (-(i as f32) / (dur_samples as f32 * 0.25)).exp();
        let n: f32 = rng.gen_range(-1.0..1.0);
        buf[idx] += amp * env * n;
    }
}

// Choose a one-measure rhythm as eighth-note durations that sum to 8.
fn random_pattern(rng: &mut StdRng) -> Vec<usize> {
    const PATTERNS: [&[usize]; 6] = [
        &[2, 2, 2, 2],
        &[1, 1, 2, 1, 1, 2],
        &[2, 1, 1, 2, 1, 1],
        &[3, 1, 3, 1],
        &[1, 1, 1, 1, 2, 2],
        &[4, 2, 2],
    ];
    PATTERNS[rng.gen_range(0..PATTERNS.len())].to_vec()
}

// Pitch for a melody note in this chord,
// constrained to the first octave of the major scale.
fn pick_melody_offset(chord: &str, rng: &mut StdRng) -> i32 {
    if rng.gen::<f64>() < 0.8 {
        // A note of the current chord, folded into the first octave.
        let tones = chord_semitones(chord);
        let t = tones[rng.gen_range(0..tones.len())];
        t.rem_euclid(12)
    } else {
        // Another note from the major scale (not a chord tone if possible).
        let tones: Vec<i32> = chord_semitones(chord)
            .iter()
            .map(|t| t.rem_euclid(12))
            .collect();
        let choices: Vec<i32> = MAJOR_SCALE
            .iter()
            .copied()
            .filter(|s| !tones.contains(s))
            .collect();
        let pool = if choices.is_empty() {
            MAJOR_SCALE.to_vec()
        } else {
            choices
        };
        pool[rng.gen_range(0..pool.len())]
    }
}

// The closest chord note strictly below a given melody midi note.
fn closest_chord_note_below(chord: &str, key_root: i32, melody_midi: i32) -> Option<i32> {
    let mut best: Option<i32> = None;
    for &tone in chord_semitones(chord).iter() {
        // Chord tone across a few octaves.
        for oct in -3..=3 {
            let note = key_root + tone + 12 * oct;
            if note < melody_midi {
                best = Some(best.map_or(note, |b| b.max(note)));
            }
        }
    }
    best
}

struct Song {
    key_root: i32,
    bpm: f64,
    structure: String,
    // distinct letters in order of first appearance, with their chord loop
    loops: Vec<(char, [&'static str; 4])>,
}

// Render a single line (4 measures) into its own buffer.
#[allow(clippy::too_many_arguments)]
fn render_line(
    chords: &[&str; 4],
    key_root: i32,
    eighth_samples: usize,
    pattern: &[usize],
    opts: &Options,
    drum_pattern: &[bool],
    rng: &mut StdRng,
) -> Vec<f32> {
    let measure_samples = eighth_samples * 8;
    let line_len = measure_samples * 4;
    let mut buf = vec![0.0f32; line_len];

    for (m, &chord) in chords.iter().enumerate() {
        let m_start = m * measure_samples;

        // --- Bass: chord root two octaves down, held for the whole measure.
        if opts.bass {
            let root = key_root + chord_semitones(chord)[0] - 24;
            add_sawtooth(
                &mut buf,
                m_start,
                midi_to_freq(root as f64),
                measure_samples,
                0.38,
            );
        }

        // --- Melody (plus optional harmony), following the rhythm pattern.
        let mut pos = 0usize; // in eighth-note units within the measure
        for &dur in pattern {
            let off = pick_melody_offset(chord, rng);
            let melody_midi = key_root + off;
            let start = m_start + pos * eighth_samples;
            let dur_samples = dur * eighth_samples;
            add_sawtooth(
                &mut buf,
                start,
                midi_to_freq(melody_midi as f64),
                dur_samples,
                0.5,
            );

            if opts.harmony {
                if let Some(h) = closest_chord_note_below(chord, key_root, melody_midi) {
                    add_sawtooth(&mut buf, start, midi_to_freq(h as f64), dur_samples, 0.28);
                }
            }

            pos += dur;
            if pos >= 8 {
                break;
            }
        }

        // --- Drums: white-noise hits at the chosen one-measure positions.
        if opts.drums {
            for (i, &hit) in drum_pattern.iter().enumerate() {
                if hit {
                    let start = m_start + i * eighth_samples;
                    add_noise(&mut buf, start, eighth_samples, 0.35, rng);
                }
            }
        }
    }

    buf
}

fn build_song(opts: &Options, rng: &mut StdRng) -> (Song, Vec<f32>) {
    // Structure, key and tempo.
    let structure = SONG_STRUCTURES[rng.gen_range(0..SONG_STRUCTURES.len())].to_string();
    let key_root = rng.gen_range(57..=69); // A3 (57) .. A4 (69)
    let bpm = rng.gen_range(80.0..=160.0);

    // Distinct letters (excluding the slash), in order of appearance.
    let mut letters: Vec<char> = Vec::new();
    for c in structure.chars() {
        if c != '/' && !letters.contains(&c) {
            letters.push(c);
        }
    }

    // Assign each letter a distinct chord loop.
    let mut loop_indices: Vec<usize> = (0..CHORD_LOOPS.len()).collect();
    loop_indices.shuffle(rng);
    let loops: Vec<(char, [&'static str; 4])> = letters
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, CHORD_LOOPS[loop_indices[i]]))
        .collect();

    // Tempo -> eighth-note length in samples (common time, 4 beats/measure).
    let beat_secs = 60.0 / bpm;
    let eighth_samples = (beat_secs / 2.0 * SR).round() as usize;

    // Rhythm patterns: one for the verse, one for the chorus.
    let (verse_pat, chorus_pat) = if opts.rhythm {
        (random_pattern(rng), random_pattern(rng))
    } else {
        (vec![1; 8], vec![1; 8])
    };

    // Which letters belong to the chorus (after the slash).
    let chorus_letters: Vec<char> = match structure.split_once('/') {
        Some((_, chorus)) => chorus.chars().collect(),
        None => Vec::new(),
    };

    // One fixed one-measure drum pattern for the whole song.
    let drum_pattern: Vec<bool> = if opts.drums {
        (0..8).map(|_| rng.gen::<f64>() < 0.5).collect()
    } else {
        vec![false; 8]
    };

    // Render each distinct letter's line once; identical letters reuse it.
    let mut rendered: Vec<(char, Vec<f32>)> = Vec::new();
    for (c, chords) in &loops {
        let pat = if chorus_letters.contains(c) {
            &chorus_pat
        } else {
            &verse_pat
        };
        let line = render_line(chords, key_root, eighth_samples, pat, opts, &drum_pattern, rng);
        rendered.push((*c, line));
    }

    // Concatenate lines according to the structure (ignoring the slash).
    let mut samples: Vec<f32> = Vec::new();
    for c in structure.chars() {
        if c == '/' {
            continue;
        }
        let line = &rendered.iter().find(|(rc, _)| *rc == c).unwrap().1;
        samples.extend_from_slice(line);
    }

    let song = Song {
        key_root,
        bpm,
        structure,
        loops,
    };
    (song, samples)
}

// Scale the mixed buffer to a safe peak and convert to 16-bit samples.
fn to_i16(samples: &[f32]) -> Vec<i16> {
    let peak = samples.iter().fold(0.0f32, |a, &s| a.max(s.abs()));
    let gain = if peak > 0.0 { 0.9 / peak } else { 1.0 };
    samples
        .iter()
        .map(|&s| (s * gain * i16::MAX as f32).round() as i16)
        .collect()
}

fn write_wav(path: &str, samples: &[i16]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample(s)?;
    }
    writer.finalize()
}

fn play(samples: Vec<i16>) -> Result<(), Box<dyn std::error::Error>> {
    use rodio::buffer::SamplesBuffer;
    use rodio::{OutputStream, Sink};

    let (_stream, handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&handle)?;
    sink.append(SamplesBuffer::new(1, SAMPLE_RATE, samples));
    sink.sleep_until_end();
    Ok(())
}

fn main() {
    let opts = parse_args();
    let mut rng = match opts.seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_entropy(),
    };

    let (song, samples) = build_song(&opts, &mut rng);
    let pcm = to_i16(&samples);

    // Report what was generated.
    let note_names = [
        "A", "Bb", "B", "C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#",
    ];
    let key_name = note_names[((song.key_root - 57).rem_euclid(12)) as usize];
    let octave = (song.key_root / 12) - 1;
    eprintln!(
        "Aleatoric: structure {}, key {}{}, tempo {:.0} bpm",
        song.structure, key_name, octave, song.bpm
    );
    for (c, chords) in &song.loops {
        eprintln!("  line {}: {}", c, chords.join("-"));
    }
    let secs = samples.len() as f64 / SR;
    eprintln!("  duration {:.1}s", secs);

    match &opts.output {
        Some(path) => match write_wav(path, &pcm) {
            Ok(()) => eprintln!("Wrote {path}"),
            Err(e) => {
                eprintln!("failed to write {path}: {e}");
                process::exit(1);
            }
        },
        None => {
            if let Err(e) = play(pcm) {
                eprintln!("failed to play audio: {e}");
                eprintln!("(try --output FILE.wav to write a file instead)");
                process::exit(1);
            }
        }
    }
}
