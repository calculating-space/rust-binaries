use crate::backend::Backend;
use serde::Serialize;
use speccheck::{Check, GpuKind, Requirements, Rule, Severity, Tier, Verdict};

/// One thing to try: a title, why it matters, the command, what you should see.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Example {
    pub title: &'static str,
    /// What this step shows, in plain words. Mention downloads and durations here.
    pub learn: &'static str,
    /// Shell command, valid inside the sandbox.
    pub run: &'static str,
    /// What success looks like.
    pub expect: &'static str,
}

/// A recipe is a guided exploration of one project: what it is, a hello world,
/// a tour from easy to advanced, and what the resident agent should propose.
/// Environment details (backend, packages) are here too, but the user never
/// needs to look at them.
#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub name: &'static str,
    pub summary: &'static str,
    /// A short paragraph: what the project is, who makes it, why you would care.
    pub what: &'static str,
    pub repo: &'static str,
    pub backend: Backend,
    pub packages: &'static [&'static str],
    pub python: &'static str,
    pub hello: Example,
    /// Ordered from first steps to showing off what the project can do.
    pub tour: &'static [Example],
    /// Open-ended experiments the agent proposes once the tour is done.
    pub suggestions: &'static [&'static str],
    pub caveats: &'static [&'static str],
    /// What the machine needs, as a speccheck requirements matrix.
    #[serde(skip)]
    pub requirements: fn() -> Requirements,
}

fn rule(check: Check, severity: Severity, why: &str) -> Rule {
    Rule {
        check,
        severity,
        why: why.into(),
    }
}
fn tier(name: &str, enables: &str, checks: Vec<Check>) -> Tier {
    Tier {
        name: name.into(),
        enables: enables.into(),
        checks,
    }
}
fn platform(pairs: &[(&str, &str)]) -> Check {
    Check::Platform {
        any_of: pairs
            .iter()
            .map(|(o, a)| (o.to_string(), a.to_string()))
            .collect(),
    }
}
fn tool(name: &str) -> Check {
    Check::Tool { name: name.into() }
}
fn mem(gb: u64) -> Check {
    Check::MinMemoryGb { gb }
}
fn disk(gb: u64) -> Check {
    Check::MinDiskFreeGb { gb }
}
fn voice(language: &str) -> Check {
    Check::Voice {
        language: language.into(),
    }
}
fn microphone() -> Check {
    Check::AudioInput
}

fn bare_requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "bare".into(),
        rules: vec![
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(disk(1), Severity::Blocks, "room for an interpreter"),
        ],
        tiers: vec![],
    }
}

fn mlx_requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "mlx".into(),
        rules: vec![
            rule(
                platform(&[
                    ("macos", "aarch64"),
                    ("linux", "x86_64"),
                    ("linux", "aarch64"),
                ]),
                Severity::Blocks,
                "no MLX wheels for Intel Macs or Windows",
            ),
            rule(
                Check::Gpu {
                    any_of: vec![GpuKind::Metal],
                },
                Severity::Pointless,
                "without Apple Silicon MLX runs on the CPU and its whole point, unified memory on the GPU, is gone",
            ),
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(
                mem(8),
                Severity::Blocks,
                "the hello world and a 3B model need about 4 GB, plus headroom",
            ),
            rule(
                disk(5),
                Severity::Blocks,
                "packages (about 600 MB) plus the first 3B model (1.8 GB)",
            ),
            rule(
                mem(16),
                Severity::Recommended,
                "8B models in 4-bit want 6 GB of memory for the weights alone",
            ),
        ],
        tiers: vec![
            tier(
                "small models",
                "hello world and tour steps 1-5, 7, 8 (3B model)",
                vec![mem(8), disk(5)],
            ),
            tier("8B models", "tour step 6", vec![mem(16), disk(12)]),
            tier(
                "convert your own",
                "tour step 9 (downloads a 1 GB model, writes a 4-bit copy)",
                vec![disk(8)],
            ),
            tier(
                "30B-class models",
                "go further: ~17 GB of weights",
                vec![mem(32), disk(25)],
            ),
            tier(
                "70B-class models",
                "go further: ~40 GB of weights, tight even at 64 GB",
                vec![mem(64), disk(50)],
            ),
        ],
    }
}

fn whisper_requirements() -> Requirements {
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

fn torch_requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "torch".into(),
        rules: vec![
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(
                Check::Gpu {
                    any_of: vec![GpuKind::Metal, GpuKind::Cuda],
                },
                Severity::Pointless,
                "this recipe is about GPU acceleration; without a GPU every step runs on the CPU",
            ),
            rule(mem(8), Severity::Blocks, "torch plus a 4096x4096 workload"),
            rule(disk(4), Severity::Blocks, "torch wheels are large"),
        ],
        tiers: vec![tier("whole tour", "all steps", vec![mem(8), disk(4)])],
    }
}

pub const RECIPES: &[Recipe] = &[
    Recipe {
        name: "mlx",
        summary: "Apple's machine learning framework for Apple Silicon, with mlx-lm for running LLMs locally",
        what: "MLX is Apple's open-source array and machine learning framework built for Apple Silicon. It looks like NumPy and PyTorch but runs on the Mac's GPU using unified memory, so nothing is copied between CPU and GPU. mlx-lm sits on top and runs open-weight language models (Llama, Qwen, Mistral, and more) with one command. It is the fastest way to run and fine-tune LLMs on a Mac.",
        repo: "https://github.com/ml-explore/mlx",
        backend: Backend::Uv,
        packages: &["mlx", "mlx-lm", "huggingface_hub"],
        python: "3.12",
        hello: Example {
            title: "MLX sees the GPU",
            learn: "Imports MLX and asks which device it computes on.",
            run: "python -c 'import mlx.core as mx; print(\"mlx\", mx.__version__, \"on\", mx.default_device())'",
            expect: "A version and `Device(gpu, 0)`.",
        },
        tour: &[
            Example {
                title: "Arrays like NumPy, on the GPU",
                learn: "The API is NumPy-shaped. Work is lazy: nothing computes until you ask for a value.",
                run: "python -c 'import mlx.core as mx; a = mx.arange(1, 11); print(\"squares:\", (a * a).tolist()); print(\"mean:\", mx.mean(a).item())'",
                expect: "The squares 1 to 100 and a mean of 5.5.",
            },
            Example {
                title: "Talk to a language model",
                learn: "Downloads a 3B model in 4-bit (about 1.8 GB, once) and generates text. The last lines report tokens per second; that number is the point of MLX.",
                run: "mlx_lm.generate --model mlx-community/Llama-3.2-3B-Instruct-4bit --prompt 'Explain unified memory in one paragraph' --max-tokens 200",
                expect: "A paragraph of text, then prompt and generation tokens/sec and peak memory.",
            },
            Example {
                title: "Chat with it",
                learn: "An interactive chat loop on the same model. Type `q` to leave.",
                run: "mlx_lm.chat --model mlx-community/Llama-3.2-3B-Instruct-4bit",
                expect: "A prompt where you type and the model answers.",
            },
            Example {
                title: "How fast is the GPU really",
                learn: "A single large matrix multiply, timed after a warmup. Compare with the CPU number below.",
                run: "python -c 'import mlx.core as mx, time\na = mx.random.normal((4096, 4096)); mx.eval(a @ a)\nt = time.perf_counter(); mx.eval(a @ a); d = time.perf_counter() - t\nprint(f\"gpu: {d*1e3:.1f} ms, {2*4096**3/d/1e12:.2f} TFLOPS\")'",
                expect: "A few tens of milliseconds and a few TFLOPS on an M1 Max.",
            },
            Example {
                title: "Same multiply on the CPU",
                learn: "MLX can target the CPU too, with the same code and no copies thanks to unified memory.",
                run: "python -c 'import mlx.core as mx, time\nmx.set_default_device(mx.cpu)\na = mx.random.normal((4096, 4096)); mx.eval(a @ a)\nt = time.perf_counter(); mx.eval(a @ a); d = time.perf_counter() - t\nprint(f\"cpu: {d*1e3:.1f} ms, {2*4096**3/d/1e12:.2f} TFLOPS\")'",
                expect: "Noticeably slower than the GPU run.",
            },
            Example {
                title: "A bigger model",
                learn: "An 8B model in 4-bit (about 4.5 GB, once). Watch how tokens/sec drops as parameters grow. With 64 GB you can go much larger later.",
                run: "mlx_lm.generate --model mlx-community/Meta-Llama-3.1-8B-Instruct-4bit --prompt 'Explain unified memory in one paragraph' --max-tokens 200",
                expect: "Better prose, lower tokens/sec than the 3B model.",
            },
            Example {
                title: "Automatic differentiation",
                learn: "MLX computes gradients of plain Python functions, the building block for training.",
                run: "python -c 'import mlx.core as mx\nf = lambda x: (x ** 3).sum(); x = mx.array([1.0, 2.0, 3.0])\nprint(\"f(x) =\", f(x).item(), \" grad =\", mx.grad(f)(x).tolist())'",
                expect: "f(x) = 36 and a gradient of [3, 12, 27].",
            },
            Example {
                title: "Train a tiny model from scratch",
                learn: "Fits a line to noisy data with gradient descent, entirely on the GPU. This is the same loop that trains large networks.",
                run: "python -c 'import mlx.core as mx\nx = mx.random.normal((256,)); y = 3 * x + 1 + 0.1 * mx.random.normal((256,))\nw = mx.zeros((2,)); loss = lambda w: mx.mean((w[0] * x + w[1] - y) ** 2); g = mx.grad(loss)\nfor i in range(200): w = w - 0.1 * g(w)\nprint(\"learned slope %.2f intercept %.2f (truth 3, 1)\" % (w[0].item(), w[1].item()))'",
                expect: "Slope near 3 and intercept near 1.",
            },
            Example {
                title: "Convert and quantize a model yourself",
                learn: "Takes a model straight from Hugging Face (Qwen 0.5B, about 1 GB) and produces a 4-bit MLX version in the working dir. This is how the mlx-community models are made.",
                run: "mlx_lm.convert --hf-path Qwen/Qwen2.5-0.5B-Instruct -q --mlx-path ./qwen-0.5b-4bit\nmlx_lm.generate --model ./qwen-0.5b-4bit --prompt 'Say hello in three languages' --max-tokens 100",
                expect: "A conversion log, a new folder, and a reply from your own quantized model.",
            },
            Example {
                title: "Serve it as an OpenAI-compatible API",
                learn: "Starts a local server any OpenAI client can talk to. Runs until you press Ctrl-C; test it from another terminal with curl against http://localhost:8080/v1/chat/completions.",
                run: "mlx_lm.server --model mlx-community/Llama-3.2-3B-Instruct-4bit --port 8080",
                expect: "A server log line saying it is listening on port 8080.",
            },
        ],
        suggestions: &[
            "Run a 30B-class 4-bit model (about 17 GB) and compare tokens/sec with the 3B and 8B runs.",
            "Compare 4-bit against 8-bit quantization of the same model: speed, peak memory and output quality on a fixed prompt.",
            "Measure prompt processing separately from generation by feeding a long prompt (several thousand tokens) and a short one.",
            "Fine-tune a small model with LoRA on a few hundred example lines using mlx_lm.lora and see the behavior change.",
            "If llama.cpp or Ollama are installed on the host, run the same model there for a like-for-like baseline; otherwise say so and skip.",
        ],
        caveats: &[
            "Model downloads go into the sandbox and are deleted with it. Say the size before pulling anything over a few hundred MB.",
            "First run of any model includes download and Metal shader compile time; report the second run.",
            "Docker on macOS has no GPU access, so MLX numbers from a docker backend are CPU-only and not representative.",
        ],
        requirements: mlx_requirements,
    },
    Recipe {
        name: "whisper",
        summary: "OpenAI's Whisper speech-to-text models, running on the Mac's GPU through MLX",
        what: "Whisper is OpenAI's open speech recognition model family, from an 80 MB tiny model to the 3 GB large-v3. It transcribes almost any language, translates speech to English, and gives timestamps down to the word. mlx-whisper is Apple's port to MLX, so on a Mac it runs on the GPU and turns a minute of speech into text in a few seconds, offline, with nothing leaving your machine.",
        repo: "https://github.com/ml-explore/mlx-examples/tree/main/whisper",
        backend: Backend::Uv,
        packages: &["mlx-whisper"],
        python: "3.12",
        hello: Example {
            title: "Your Mac speaks, Whisper listens",
            learn: "Makes a short recording with the macOS `say` command, then transcribes it with the tiny model (about 80 MB, once).",
            run: "say -o hello.aiff 'Whisper turns speech into text. This sentence was spoken by your Mac.' &&\nmlx_whisper hello.aiff --model mlx-community/whisper-tiny",
            expect: "Detected language English, then both sentences back with timestamps. The transcript is also in hello.txt.",
        },
        tour: &[
            Example {
                title: "Subtitles you can drop into a video",
                learn: "The same transcript as an SRT file. VTT, TSV and JSON work the same way; `--verbose False` keeps the terminal quiet.",
                run: "mlx_whisper hello.aiff --model mlx-community/whisper-tiny --output-format srt --verbose False && cat hello.srt",
                expect: "Numbered cues with start and end times, the format every video player reads.",
            },
            Example {
                title: "When was each word said",
                learn: "The Python API returns segments and, on request, a timestamp for every word. Karaoke captions and audio editors are built on this.",
                run: "python -c 'import mlx_whisper\nr = mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=\"mlx-community/whisper-tiny\", word_timestamps=True)\nfor s in r[\"segments\"]:\n    print(\"%6.2fs  %s\" % (s[\"start\"], \" \".join(\"%s@%.2f\" % (w[\"word\"].strip(), w[\"start\"]) for w in s[\"words\"])))'",
                expect: "One line per sentence, each word tagged with the second it starts.",
            },
            Example {
                title: "A minute of speech, timed",
                learn: "Generates about 70 seconds of speech and transcribes it with the tiny model, reporting how many times faster than real time it ran. Whisper works in 30-second windows, so this is the first clip that needs several.",
                run: "python -c 'p = \"The quick brown fox jumps over the lazy dog. Unified memory lets the CPU and GPU share one pool, \"\np += \"so nothing is copied. Whisper listens in thirty second windows and writes what it hears. \"\nopen(\"long.txt\", \"w\").write(p * 6)' &&\nsay -o long.aiff -f long.txt &&\npython -c 'import mlx_whisper, time\nm = \"mlx-community/whisper-tiny\"; mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=m)\nt = time.perf_counter(); r = mlx_whisper.transcribe(\"long.aiff\", path_or_hf_repo=m, language=\"en\"); d = time.perf_counter() - t\nprint(\"%.0fs of audio in %.1fs: %.0fx real time\" % (r[\"segments\"][-1][\"end\"], d, r[\"segments\"][-1][\"end\"] / d))'",
                expect: "About 70 seconds of audio done in a few seconds, 10x real time or better.",
            },
            Example {
                title: "The big model",
                learn: "Downloads large-v3-turbo (about 1.5 GB, once), the model most transcription services run, and times it on the same minute. Slower than tiny, far more accurate on real recordings.",
                run: "python -c 'import mlx_whisper, time\nm = \"mlx-community/whisper-large-v3-turbo\"; mlx_whisper.transcribe(\"hello.aiff\", path_or_hf_repo=m)\nt = time.perf_counter(); r = mlx_whisper.transcribe(\"long.aiff\", path_or_hf_repo=m, language=\"en\"); d = time.perf_counter() - t\nprint(\"%.0fs of audio in %.1fs: %.0fx real time\" % (r[\"segments\"][-1][\"end\"], d, r[\"segments\"][-1][\"end\"] / d))'",
                expect: "The same line for turbo, still several times faster than real time.",
            },
            Example {
                title: "Another language, detected automatically",
                learn: "Speaks a French sentence with whichever French system voice is installed and lets Whisper work out the language.",
                run: "v=$(say -v '?' | grep -m1 ' fr_' | sed 's/ *fr_.*//') &&\nsay -v \"$v\" -o french.aiff 'Le chat dort sur le canapé pendant que la pluie tombe dehors.' &&\nmlx_whisper french.aiff --model mlx-community/whisper-large-v3-turbo --verbose False && cat french.txt",
                expect: "`Detected language: French`, then the sentence transcribed in French.",
            },
            Example {
                title: "Translate to English while transcribing",
                learn: "Downloads the medium model (about 1.4 GB, once) and asks it to translate. Turbo cannot: it was trained for transcription only. Medium and large-v3 can.",
                run: "mlx_whisper french.aiff --model mlx-community/whisper-medium-mlx --task translate --verbose False --output-name french-en && cat french-en.txt",
                expect: "`The cat sleeps on the couch while the rain falls outside.`",
            },
            Example {
                title: "Your own voice",
                learn: "Records five seconds from the microphone with ffmpeg (macOS asks for permission the first time), then transcribes with the big model. Say anything.",
                run: "echo 'recording 5 seconds, speak now' &&\nffmpeg -y -nostdin -loglevel error -f avfoundation -i ':0' -t 5 me.wav &&\nmlx_whisper me.wav --model mlx-community/whisper-large-v3-turbo --verbose False && cat me.txt",
                expect: "What you said, in writing.",
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
        requirements: whisper_requirements,
    },
    Recipe {
        name: "torch",
        summary: "PyTorch, the most widely used deep learning framework, with Apple GPU acceleration",
        what: "PyTorch is the framework most research code and most tutorials are written in. On a Mac it can run on the GPU through Apple's Metal Performance Shaders (MPS). Explore it here to compare with MLX or to run something written for PyTorch.",
        repo: "https://github.com/pytorch/pytorch",
        backend: Backend::Uv,
        packages: &["torch", "numpy"],
        python: "3.12",
        hello: Example {
            title: "PyTorch sees the Apple GPU",
            learn: "Imports torch and checks whether the MPS backend is available.",
            run: "python -c 'import torch; print(\"torch\", torch.__version__, \"mps:\", torch.backends.mps.is_available())'",
            expect: "A version and `mps: True`.",
        },
        tour: &[
            Example {
                title: "Tensors on the GPU",
                learn: "Same code as on CUDA, with `mps` as the device name.",
                run: "python -c 'import torch; a = torch.arange(1, 11, device=\"mps\"); print((a * a).tolist())'",
                expect: "The squares 1 to 100.",
            },
            Example {
                title: "GPU versus CPU matrix multiply",
                learn: "One large multiply on each device, timed after warmup. Compare with the MLX recipe's numbers.",
                run: "python -c 'import torch, time\nfor d in (\"mps\", \"cpu\"):\n    a = torch.randn(4096, 4096, device=d); a @ a\n    if d == \"mps\": torch.mps.synchronize()\n    t = time.perf_counter(); a @ a\n    if d == \"mps\": torch.mps.synchronize()\n    print(d, f\"{(time.perf_counter()-t)*1e3:.1f} ms\")'",
                expect: "Two timings, MPS well ahead.",
            },
            Example {
                title: "Train a tiny network",
                learn: "A two-layer network fitting a sine wave with the standard PyTorch training loop.",
                run: "python -c 'import torch, torch.nn as nn\nx = torch.linspace(-3, 3, 512).unsqueeze(1); y = torch.sin(x)\nm = nn.Sequential(nn.Linear(1, 64), nn.Tanh(), nn.Linear(64, 1)); opt = torch.optim.Adam(m.parameters(), 1e-2)\nfor i in range(500):\n    opt.zero_grad(); loss = ((m(x) - y) ** 2).mean(); loss.backward(); opt.step()\nprint(f\"final loss {loss.item():.4f}\")'",
                expect: "A loss well below 0.01.",
            },
        ],
        suggestions: &[
            "Time a small transformer forward pass on mps and cpu with warmup excluded, and note any op that falls back to CPU.",
        ],
        caveats: &[
            "MPS support has known gaps; fall back to CPU for unsupported ops and say which ops fell back.",
        ],
        requirements: torch_requirements,
    },
    Recipe {
        name: "bare",
        summary: "An empty Python environment to explore anything not covered by a recipe",
        what: "A clean Python with nothing installed. Use it when there is no recipe for the project you want to try: install it with `uv pip install`, poke at it, throw the sandbox away.",
        repo: "https://docs.astral.sh/uv/",
        backend: Backend::Uv,
        packages: &[],
        python: "3.12",
        hello: Example {
            title: "Which Python is this",
            learn: "Confirms the sandbox has its own interpreter, separate from the system one.",
            run: "python -c 'import sys, platform; print(sys.version.split()[0], platform.machine(), sys.prefix)'",
            expect: "A version, the architecture, and a prefix inside the sandbox directory.",
        },
        tour: &[
            Example {
                title: "Install something and see what came with it",
                learn: "Packages install into the sandbox only. Swap `rich` for whatever you want to explore.",
                run: "uv pip install rich && python -c 'from rich import print; print(\"[bold green]hello from the sandbox[/]\")'",
                expect: "A short install log, then green bold text.",
            },
            Example {
                title: "List what is installed",
                learn: "Shows the sandbox is self-contained; nothing here is on your machine.",
                run: "uv pip list",
                expect: "A short table of packages.",
            },
        ],
        suggestions: &[
            "Install the project the user names, run its own hello world, and report what it pulled in.",
        ],
        caveats: &[],
        requirements: bare_requirements,
    },
];

pub fn recipe(name: &str) -> Result<&'static Recipe, String> {
    RECIPES.iter().find(|r| r.name == name).ok_or_else(|| {
        let names: Vec<_> = RECIPES.iter().map(|r| r.name).collect();
        format!("unknown recipe {name:?}; known: {}", names.join(", "))
    })
}

pub fn wrap(text: &str, indent: usize, width: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width - indent {
            out.push_str(&pad);
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(&pad);
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn example(out: &mut String, label: &str, e: &Example) {
    out.push_str(&format!("    {label}{}\n", e.title));
    out.push_str(&wrap(e.learn, 8, 78));
    for (i, l) in e.run.lines().enumerate() {
        out.push_str(&format!("        {} {l}\n", if i == 0 { "$" } else { " " }));
    }
    out.push_str(&wrap(&format!("You should see: {}", e.expect), 8, 78));
    out.push('\n');
}

/// One-line-per-recipe table for `trybox recipes`.
pub fn table() -> String {
    let mut out = format!("{:<8} {:<6} {}\n", "RECIPE", "STEPS", "WHAT IT IS");
    for r in RECIPES {
        out.push_str(&format!(
            "{:<8} {:<6} {}\n",
            r.name,
            r.tour.len() + 1,
            r.summary
        ));
    }
    out
}

/// The REQUIREMENTS section: the matrix, annotated with this machine's verdict when given.
pub fn requirements_section(r: &Recipe, verdict: Option<&Verdict>) -> String {
    let req = (r.requirements)();
    let mut out = String::from("REQUIREMENTS\n");
    if let Some(v) = verdict {
        let line = match v.outcome {
            speccheck::Outcome::CanRun => "This machine can run it.",
            speccheck::Outcome::Pointless => {
                "This machine can run it, but it does not make sense here (see below)."
            }
            speccheck::Outcome::CannotRun => "This machine cannot run it (see below).",
        };
        out.push_str(&format!("    {line}\n\n"));
    }
    let width = req
        .rules
        .iter()
        .map(|x| speccheck::need(&x.check).len())
        .max()
        .unwrap_or(4)
        .max(4);
    for (i, x) in req.rules.iter().enumerate() {
        let (mark, actual) = match verdict.and_then(|v| v.findings.get(i)) {
            Some(f) => (if f.pass { "ok" } else { "--" }, format!(" [{}]", f.actual)),
            None => ("  ", String::new()),
        };
        let sev = match x.severity {
            Severity::Blocks => "required",
            Severity::Pointless => "for it to make sense",
            Severity::Recommended => "recommended",
        };
        out.push_str(&format!(
            "    {mark} {:<width$}  {sev}{actual}\n",
            speccheck::need(&x.check)
        ));
        out.push_str(&wrap(&x.why, 8 + width, 78 + width));
    }
    if !req.tiers.is_empty() {
        out.push_str("\n    What your machine unlocks:\n");
        for (i, t) in req.tiers.iter().enumerate() {
            let needs: Vec<String> = t.checks.iter().map(speccheck::need).collect();
            let mark = match verdict.and_then(|v| v.tiers.get(i)) {
                Some(tr) if tr.ok => "ok",
                Some(_) => "--",
                None => "  ",
            };
            out.push_str(&format!(
                "    {mark} {:<22} {} ({})\n",
                t.name,
                t.enables,
                needs.join(", ")
            ));
        }
    }
    out.push('\n');
    out
}

/// A man page for one recipe: what it is, hello world, the tour, where to go next.
/// Pass a verdict to annotate the requirements with this machine's actual values.
pub fn man(r: &Recipe, verdict: Option<&Verdict>) -> String {
    let upper = r.name.to_uppercase();
    let mut out = format!(
        "TRYBOX({upper})\n\nNAME\n    {} - {}\n\n",
        r.name, r.summary
    );
    out.push_str("WHAT IT IS\n");
    out.push_str(&wrap(r.what, 4, 78));
    out.push_str(&format!("    {}\n\n", r.repo));
    out.push_str(&requirements_section(r, verdict));
    out.push_str("HELLO WORLD\n");
    example(&mut out, "", &r.hello);
    out.push_str("TOUR\n");
    for (i, e) in r.tour.iter().enumerate() {
        example(&mut out, &format!("{}. ", i + 1), e);
    }
    if !r.suggestions.is_empty() {
        out.push_str("GO FURTHER\n    Things the resident agent can set up for you:\n");
        for s in r.suggestions {
            out.push_str(&wrap(&format!("- {s}"), 4, 78));
        }
        out.push('\n');
    }
    if !r.caveats.is_empty() {
        out.push_str("CAVEATS\n");
        for c in r.caveats {
            out.push_str(&wrap(&format!("- {c}"), 4, 78));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "TRY IT\n    trybox check {n}          can this machine run it, and does it make sense\n    trybox explore {n}        run the hello world, then pick tour steps\n    trybox agent {n}          open-ended: an agent inside the sandbox\n    trybox dispose {n} -y     delete everything it downloaded\n\nUNDER THE HOOD\n    backend {} · python {} · packages: {}\n",
        r.backend.as_str(),
        r.python,
        if r.packages.is_empty() { "none".to_string() } else { r.packages.join(" ") },
        n = r.name
    ));
    out
}

/// Detect this machine with the tools the recipe cares about, and check it.
pub fn check_here(r: &Recipe, disk_path: &std::path::Path) -> Verdict {
    let req = (r.requirements)();
    let spec = speccheck::spec::detect(disk_path, &req.probes());
    speccheck::check(&spec, &req)
}

/// The verdict for this machine with only the outcome-deciding rules probed. For menus that
/// show "can run here"; the recommended rows and tiers are not to be trusted from this one.
pub fn outcome_here(r: &Recipe, disk_path: &std::path::Path) -> Verdict {
    let req = (r.requirements)();
    let spec = speccheck::spec::detect(disk_path, &req.outcome_probes(Severity::Pointless));
    speccheck::check(&spec, &req)
}
