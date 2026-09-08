//! torch: PyTorch, the most widely used deep learning framework, with Apple GPU acceleration

use super::matrix::*;
use super::{Example, Recipe};
use crate::backend::Backend;
use speccheck::{Check, GpuKind, Requirements, Severity};

pub const RECIPE: Recipe = Recipe {
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
        models: &[],
    },
    tour: &[
        Example {
            title: "Tensors on the GPU",
            learn: "Same code as on CUDA, with `mps` as the device name.",
            run: "python -c 'import torch; a = torch.arange(1, 11, device=\"mps\"); print((a * a).tolist())'",
            expect: "The squares 1 to 100.",
            models: &[],
        },
        Example {
            title: "GPU versus CPU matrix multiply",
            learn: "One large multiply on each device, timed after warmup. Compare with the MLX recipe's numbers.",
            run: "python -c 'import torch, time\nfor d in (\"mps\", \"cpu\"):\n    a = torch.randn(4096, 4096, device=d); a @ a\n    if d == \"mps\": torch.mps.synchronize()\n    t = time.perf_counter(); a @ a\n    if d == \"mps\": torch.mps.synchronize()\n    print(d, f\"{(time.perf_counter()-t)*1e3:.1f} ms\")'",
            expect: "Two timings, MPS well ahead.",
            models: &[],
        },
        Example {
            title: "Train a tiny network",
            learn: "A two-layer network fitting a sine wave with the standard PyTorch training loop.",
            run: "python -c 'import torch, torch.nn as nn\nx = torch.linspace(-3, 3, 512).unsqueeze(1); y = torch.sin(x)\nm = nn.Sequential(nn.Linear(1, 64), nn.Tanh(), nn.Linear(64, 1)); opt = torch.optim.Adam(m.parameters(), 1e-2)\nfor i in range(500):\n    opt.zero_grad(); loss = ((m(x) - y) ** 2).mean(); loss.backward(); opt.step()\nprint(f\"final loss {loss.item():.4f}\")'",
            expect: "A loss well below 0.01.",
            models: &[],
        },
    ],
    suggestions: &[
        "Time a small transformer forward pass on mps and cpu with warmup excluded, and note any op that falls back to CPU.",
    ],
    caveats: &[
        "MPS support has known gaps; fall back to CPU for unsupported ops and say which ops fell back.",
    ],
    requirements,
};

fn requirements() -> Requirements {
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
