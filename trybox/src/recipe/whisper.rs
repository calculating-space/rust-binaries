//! whisper: OpenAI's Whisper speech-to-text models, running on the Mac's GPU through MLX

use super::matrix::*;
use super::{Example, Recipe};
use crate::backend::Backend;
use speccheck::{Check, GpuKind, Requirements, Severity};

pub const RECIPE: Recipe = Recipe {
    name: "whisper",
    summary: "OpenAI's Whisper speech-to-text models, running on the Mac's GPU through MLX",
    what: "Whisper is OpenAI's open speech recognition model family, from an 80 MB tiny model to the 3 GB large-v3. It transcribes almost any language, translates speech to English, and gives timestamps down to the word. mlx-whisper is Apple's port to MLX, so on a Mac it runs on the GPU and turns a minute of speech into text in a few seconds, offline, with nothing leaving your machine.",
    repo: "https://github.com/ml-explore/mlx-examples/tree/main/whisper",
    backend: Backend::Uv,
    packages: &["mlx-whisper"],
    python: "3.12",
    hello: Example {
        title: "Type something, hear it, see it come back",
        learn: "Setup first: the tiny model (about 80 MB, once) is fetched and warmed up. Then a prompt: each line you type is spoken by your Mac (turn the volume up) and Whisper writes down what it heard. An empty line stops; the last line you typed becomes hello.aiff for the next steps.",
        run: "python -c 'import subprocess, os\nprint(\"loading whisper...\", end=\"\", flush=True); import mlx_whisper\nsubprocess.run([\"say\", \"-o\", \"warmup.aiff\", \"ready\"])\nmlx_whisper.transcribe(\"warmup.aiff\", path_or_hf_repo=\"mlx-community/whisper-tiny\"); print(\" ready\")\nprint(\"\\nType a sentence and press Enter: your Mac says it, Whisper writes it back. Empty line to stop.\")\ndef hear(text):\n    subprocess.run([\"say\", \"-o\", \"hello.aiff\", text]); subprocess.run([\"afplay\", \"hello.aiff\"])\n    print(\"heard:\", mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=\"mlx-community/whisper-tiny\")[\"text\"].strip())\ntry:\n    while (line := input(\"> \").strip()): hear(line)\nexcept EOFError: pass\nif not os.path.exists(\"hello.aiff\"): hear(\"Whisper turns speech into text. This sentence was spoken by your Mac.\")'",
        expect: "Your sentence out loud, then `heard:` and the same words in writing. Try a tongue twister or a name it might get wrong.",
        models: &["mlx-community/whisper-tiny"],
    },
    tour: &[
        Example {
            title: "Subtitles you can drop into a video",
            learn: "The last sentence you typed, as an SRT subtitle file. VTT, TSV and JSON work the same way; `--verbose False` keeps the terminal quiet.",
            run: "mlx_whisper hello.aiff --model mlx-community/whisper-tiny --output-format srt --verbose False && cat hello.srt",
            expect: "Numbered cues with start and end times, the format every video player reads.",
            models: &["mlx-community/whisper-tiny"],
        },
        Example {
            title: "When was each word said",
            learn: "The Python API returns segments and, on request, a timestamp for every word. Karaoke captions and audio editors are built on this.",
            run: "python -c 'import mlx_whisper\nr = mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=\"mlx-community/whisper-tiny\", word_timestamps=True)\nfor s in r[\"segments\"]:\n    print(\"%6.2fs  %s\" % (s[\"start\"], \" \".join(\"%s@%.2f\" % (w[\"word\"].strip(), w[\"start\"]) for w in s[\"words\"])))'",
            expect: "One line per sentence, each word tagged with the second it starts.",
            models: &["mlx-community/whisper-tiny"],
        },
        Example {
            title: "A minute of speech, timed",
            learn: "Generates about 70 seconds of speech, plays the first five, and transcribes all of it with the tiny model, reporting how many times faster than real time it ran. Whisper works in 30-second windows, so this is the first clip that needs several.",
            run: "python -c 'p = \"The quick brown fox jumps over the lazy dog. Unified memory lets the CPU and GPU share one pool, \"\np += \"so nothing is copied. Whisper listens in thirty second windows and writes what it hears. \"\nopen(\"long.txt\", \"w\").write(p * 6)' &&\nsay -o long.aiff -f long.txt && afplay -t 5 long.aiff &&\npython -c 'import mlx_whisper, time\nm = \"mlx-community/whisper-tiny\"; mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=m)\nt = time.perf_counter(); r = mlx_whisper.transcribe(\"long.aiff\", path_or_hf_repo=m, language=\"en\"); d = time.perf_counter() - t\nprint(\"%.0fs of audio in %.1fs: %.0fx real time\" % (r[\"segments\"][-1][\"end\"], d, r[\"segments\"][-1][\"end\"] / d))'",
            expect: "About 70 seconds of audio done in a few seconds, 10x real time or better.",
            models: &["mlx-community/whisper-tiny"],
        },
        Example {
            title: "The big model",
            learn: "Fetches large-v3-turbo (about 1.5 GB, once), the model most transcription services run, and times it on the same minute. Slower than tiny, far more accurate on real recordings.",
            run: "python -c 'import mlx_whisper, time\nm = \"mlx-community/whisper-large-v3-turbo\"; mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=m)\nt = time.perf_counter(); r = mlx_whisper.transcribe(\"long.aiff\", path_or_hf_repo=m, language=\"en\"); d = time.perf_counter() - t\nprint(\"%.0fs of audio in %.1fs: %.0fx real time\" % (r[\"segments\"][-1][\"end\"], d, r[\"segments\"][-1][\"end\"] / d))'",
            expect: "The same line for turbo, still several times faster than real time.",
            models: &["mlx-community/whisper-large-v3-turbo"],
        },
        Example {
            title: "Another language, detected automatically",
            learn: "Speaks a French sentence with whichever French system voice is installed, plays it, and lets Whisper work out the language.",
            run: "v=$(say -v '?' | grep -m1 ' fr_' | sed 's/ *fr_.*//') &&\nsay -v \"$v\" -o french.aiff 'Le chat dort sur le canapé pendant que la pluie tombe dehors.' &&\nafplay french.aiff &&\nmlx_whisper french.aiff --model mlx-community/whisper-large-v3-turbo --verbose False && cat french.txt",
            expect: "You hear the French sentence, then `Detected language: French` and the sentence transcribed in French.",
            models: &["mlx-community/whisper-large-v3-turbo"],
        },
        Example {
            title: "Translate to English while transcribing",
            learn: "Fetches the medium model (about 1.4 GB, once) and asks it to translate. Turbo cannot: it was trained for transcription only. Medium and large-v3 can.",
            run: "mlx_whisper french.aiff --model mlx-community/whisper-medium-mlx --task translate --verbose False --output-name french-en && cat french-en.txt",
            expect: "`The cat sleeps on the couch while the rain falls outside.`",
            models: &["mlx-community/whisper-medium-mlx"],
        },
        Example {
            title: "Your own voice",
            learn: "Records five seconds from the microphone with ffmpeg (macOS asks for permission the first time), plays it back, then transcribes with the big model. Say anything.",
            run: "echo 'recording 5 seconds, speak now' &&\nffmpeg -y -nostdin -loglevel error -f avfoundation -i ':0' -t 5 me.wav &&\nafplay me.wav &&\nmlx_whisper me.wav --model mlx-community/whisper-large-v3-turbo --verbose False && cat me.txt",
            expect: "Your own voice played back, then what you said, in writing.",
            models: &["mlx-community/whisper-large-v3-turbo"],
        },
    ],
    suggestions: &[
        "Run the full large-v3 (mlx-community/whisper-large-v3-mlx, about 2.9 GB) on long.aiff and compare speed and word errors with turbo and tiny.",
        "Try the 4-bit and 8-bit variants (mlx-community/whisper-large-v3-turbo-4bit, -8bit): size on disk, speed, and whether the transcript changes.",
        "Transcribe a real recording the user provides (anything ffmpeg reads: a podcast, a voice memo, a video) and write SRT subtitles for it.",
        "Use --initial-prompt with names or jargon Whisper misspells and show the transcript before and after.",
        "Batch a folder of files into one output directory with a single mlx_whisper call and report the overall real-time factor.",
        "If whisper.cpp (whisper-cli) is installed on the host, run the same file there for a like-for-like baseline; otherwise say so and skip.",
    ],
    caveats: &[
        "Every audio file is decoded through the host's ffmpeg; without it nothing transcribes.",
        "large-v3-turbo does not translate; use whisper-medium-mlx or whisper-large-v3-mlx for --task translate.",
        "First run of a model includes download and Metal shader compile time; report the second run.",
        "Whisper hallucinates on silence and music. Say so when a transcript contains words that were not spoken.",
        "Model downloads go into the sandbox and are deleted with it. Say the size before pulling anything over a few hundred MB.",
    ],
    requirements,
};

fn requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "whisper".into(),
        rules: vec![
            rule(
                platform(&[("macos", "aarch64")]),
                Severity::Blocks,
                "no MLX wheels for Intel Macs or Windows",
            ),
            rule(
                Check::Gpu {
                    any_of: vec![GpuKind::Metal],
                },
                Severity::Pointless,
                "without the Metal GPU every model runs on the CPU, many times slower",
            ),
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(
                tool("ffmpeg"),
                Severity::Blocks,
                "mlx-whisper decodes every audio file through it (brew install ffmpeg)",
            ),
            rule(
                tool("say"),
                Severity::Blocks,
                "the tour makes its audio with the macOS say command",
            ),
            rule(
                tool("afplay"),
                Severity::Blocks,
                "and plays every clip through the speakers before transcribing it",
            ),
            rule(
                mem(8),
                Severity::Blocks,
                "large-v3-turbo needs about 3 GB while running, plus headroom",
            ),
            rule(
                disk(5),
                Severity::Blocks,
                "packages (about 1 GB) plus tiny (80 MB), turbo (1.5 GB) and medium (1.4 GB)",
            ),
            rule(
                voice("fr"),
                Severity::Recommended,
                "tour steps 5 and 6 speak a French sentence; any French voice will do",
            ),
            rule(
                microphone(),
                Severity::Recommended,
                "tour step 7 records five seconds from the microphone",
            ),
        ],
        tiers: vec![
            tier(
                "tiny model",
                "hello world and tour steps 1-3",
                vec![disk(2)],
            ),
            tier(
                "large model",
                "tour step 4 (turbo, 1.5 GB)",
                vec![mem(8), disk(5)],
            ),
            tier(
                "French",
                "tour steps 5-6 (a French voice; medium, 1.4 GB)",
                vec![voice("fr"), mem(8), disk(5)],
            ),
            tier(
                "your own voice",
                "tour step 7 (a microphone)",
                vec![microphone(), mem(8), disk(5)],
            ),
            tier(
                "full large-v3",
                "go further: 2.9 GB more for the biggest model",
                vec![mem(8), disk(8)],
            ),
        ],
    }
}
