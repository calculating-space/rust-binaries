//! mlx: Apple's machine learning framework for Apple Silicon, with mlx-lm for running LLMs locally

use super::matrix::*;
use super::{Example, Recipe};
use crate::backend::Backend;
use speccheck::{Check, GpuKind, Requirements, Severity};

pub const RECIPE: Recipe = Recipe {
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
        models: &[],
    },
    tour: &[
        Example {
            title: "Arrays like NumPy, on the GPU",
            learn: "The API is NumPy-shaped. Work is lazy: nothing computes until you ask for a value.",
            run: "python -c 'import mlx.core as mx; a = mx.arange(1, 11); print(\"squares:\", (a * a).tolist()); print(\"mean:\", mx.mean(a).item())'",
            expect: "The squares 1 to 100 and a mean of 5.5.",
            models: &[],
        },
        Example {
            title: "Talk to a language model",
            learn: "Downloads a 3B model in 4-bit (about 1.8 GB, once) and generates text. The last lines report tokens per second; that number is the point of MLX.",
            run: "mlx_lm.generate --model mlx-community/Llama-3.2-3B-Instruct-4bit --prompt 'Explain unified memory in one paragraph' --max-tokens 200",
            expect: "A paragraph of text, then prompt and generation tokens/sec and peak memory.",
            models: &["mlx-community/Llama-3.2-3B-Instruct-4bit"],
        },
        Example {
            title: "Chat with it",
            learn: "An interactive chat loop on the same model. Type `q` to leave.",
            run: "mlx_lm.chat --model mlx-community/Llama-3.2-3B-Instruct-4bit",
            expect: "A prompt where you type and the model answers.",
            models: &["mlx-community/Llama-3.2-3B-Instruct-4bit"],
        },
        Example {
            title: "How fast is the GPU really",
            learn: "A single large matrix multiply, timed after a warmup. Compare with the CPU number below.",
            run: "python -c 'import mlx.core as mx, time\na = mx.random.normal((4096, 4096)); mx.eval(a @ a)\nt = time.perf_counter(); mx.eval(a @ a); d = time.perf_counter() - t\nprint(f\"gpu: {d*1e3:.1f} ms, {2*4096**3/d/1e12:.2f} TFLOPS\")'",
            expect: "A few tens of milliseconds and a few TFLOPS on an M1 Max.",
            models: &[],
        },
        Example {
            title: "Same multiply on the CPU",
            learn: "MLX can target the CPU too, with the same code and no copies thanks to unified memory.",
            run: "python -c 'import mlx.core as mx, time\nmx.set_default_device(mx.cpu)\na = mx.random.normal((4096, 4096)); mx.eval(a @ a)\nt = time.perf_counter(); mx.eval(a @ a); d = time.perf_counter() - t\nprint(f\"cpu: {d*1e3:.1f} ms, {2*4096**3/d/1e12:.2f} TFLOPS\")'",
            expect: "Noticeably slower than the GPU run.",
            models: &[],
        },
        Example {
            title: "A bigger model",
            learn: "An 8B model in 4-bit (about 4.5 GB, once). Watch how tokens/sec drops as parameters grow. With 64 GB you can go much larger later.",
            run: "mlx_lm.generate --model mlx-community/Meta-Llama-3.1-8B-Instruct-4bit --prompt 'Explain unified memory in one paragraph' --max-tokens 200",
            expect: "Better prose, lower tokens/sec than the 3B model.",
            models: &["mlx-community/Meta-Llama-3.1-8B-Instruct-4bit"],
        },
        Example {
            title: "Automatic differentiation",
            learn: "MLX computes gradients of plain Python functions, the building block for training.",
            run: "python -c 'import mlx.core as mx\nf = lambda x: (x ** 3).sum(); x = mx.array([1.0, 2.0, 3.0])\nprint(\"f(x) =\", f(x).item(), \" grad =\", mx.grad(f)(x).tolist())'",
            expect: "f(x) = 36 and a gradient of [3, 12, 27].",
            models: &[],
        },
        Example {
            title: "Train a tiny model from scratch",
            learn: "Fits a line to noisy data with gradient descent, entirely on the GPU. This is the same loop that trains large networks.",
            run: "python -c 'import mlx.core as mx\nx = mx.random.normal((256,)); y = 3 * x + 1 + 0.1 * mx.random.normal((256,))\nw = mx.zeros((2,)); loss = lambda w: mx.mean((w[0] * x + w[1] - y) ** 2); g = mx.grad(loss)\nfor i in range(200): w = w - 0.1 * g(w)\nprint(\"learned slope %.2f intercept %.2f (truth 3, 1)\" % (w[0].item(), w[1].item()))'",
            expect: "Slope near 3 and intercept near 1.",
            models: &[],
        },
        Example {
            title: "Convert and quantize a model yourself",
            learn: "Takes a model straight from Hugging Face (Qwen 0.5B, about 1 GB) and produces a 4-bit MLX version in the working dir. This is how the mlx-community models are made.",
            run: "mlx_lm.convert --hf-path Qwen/Qwen2.5-0.5B-Instruct -q --mlx-path ./qwen-0.5b-4bit\nmlx_lm.generate --model ./qwen-0.5b-4bit --prompt 'Say hello in three languages' --max-tokens 100",
            expect: "A conversion log, a new folder, and a reply from your own quantized model.",
            models: &["Qwen/Qwen2.5-0.5B-Instruct"],
        },
        Example {
            title: "Serve it as an OpenAI-compatible API",
            learn: "Starts a local server any OpenAI client can talk to. Runs until you press Ctrl-C; test it from another terminal with curl against http://localhost:8080/v1/chat/completions.",
            run: "mlx_lm.server --model mlx-community/Llama-3.2-3B-Instruct-4bit --port 8080",
            expect: "A server log line saying it is listening on port 8080.",
            models: &["mlx-community/Llama-3.2-3B-Instruct-4bit"],
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
    requirements,
};

fn requirements() -> Requirements {
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
