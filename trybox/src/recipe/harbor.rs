//! harbor: Terminal-Bench, the exam for agents that work in a terminal, and Harbor, its runner

use super::matrix::*;
use super::{Example, Recipe};
use crate::backend::Backend;
use speccheck::{Requirements, Severity};

pub const RECIPE: Recipe = Recipe {
    name: "harbor",
    summary: "Terminal-Bench, the benchmark for agents that work in a terminal, and Harbor, the runner that scores them",
    what: "Terminal-Bench hands an AI agent a Docker container, a plain-English job and a hidden test, and counts how often the job actually gets done. Harbor is the runner: it pulls each task's image, runs an agent or the task's own known-good solution inside it, and scores the result. From the Laude Institute and Stanford. This tour scores the known-good solutions, which costs nothing; running a real agent needs an API key.",
    repo: "https://www.tbench.ai",
    backend: Backend::Uv,
    packages: &["harbor"],
    python: "3.12",
    hello: Example {
        title: "Which Harbor is this",
        learn: "Confirms the runner is installed in the sandbox. Nothing is pulled yet.",
        run: "harbor --version",
        expect: "A version number such as 0.22.0.",
        models: &[],
    },
    tour: &[
        Example {
            title: "Score the answer key on one task",
            learn: "Every task ships a known-good solution, the oracle. Scoring it proves Docker and the task image work, and spends nothing. The first run pulls an 80 MB image; about two minutes.",
            run: "harbor run -d terminal-bench/terminal-bench-2 -a oracle -t terminal-bench/regex-log -y -q",
            expect: "A table with 1 trial, 0 exceptions and Mean 1.000, then the path of the job directory.",
            models: &[],
        },
        Example {
            title: "Read what the run left behind",
            learn: "A job is a directory: one folder per task, holding what the agent did and what the verifier's tests said. The reward is the score, 1 or 0.",
            run: "cat jobs/*/regex-log__*/verifier/reward.txt\ntail -4 jobs/*/regex-log__*/verifier/test-stdout.txt",
            expect: "A 1, then pytest's last lines ending in 1 passed.",
            models: &[],
        },
        Example {
            title: "Read a task the way the agent sees it",
            learn: "Downloads the task set, 89 small folders, and prints one task's instruction. The instruction is everything the agent is told; the tests stay hidden from it.",
            run: "harbor dataset download terminal-bench/terminal-bench-2 -o tasks >/dev/null\ncat tasks/terminal-bench-2/regex-log/instruction.md",
            expect: "A short brief asking for a regular expression, ending with where to save it.",
            models: &[],
        },
        Example {
            title: "Score the answer key on every task",
            learn: "All 89 tasks, four at a time. Pulls about 60 GB of images, the largest 13 GB, and took three and a quarter hours on an M1 Max under emulation. Images stay in Docker afterwards; see the caveats.",
            run: "harbor run -d terminal-bench/terminal-bench-2 -a oracle -n 4 -y -q",
            expect: "Mean around 0.7 under emulation (62 of 89 here). Some solutions are nondeterministic and more misbehave under qemu. The job directory names them.",
            models: &[],
        },
    ],
    suggestions: &[
        "With ANTHROPIC_API_KEY set, run one task with a real agent: `harbor run -d terminal-bench/terminal-bench-2 -a claude-code -m anthropic/claude-sonnet-5 -t terminal-bench/regex-log -y`, then read jobs/*/regex-log__*/agent/claude-code.txt, every command it typed.",
        "Run one task three times with `-k 3` and compare the rewards; the benchmark is noisy from trial to trial.",
        "Pick a task whose oracle failed here and read its verifier/test-stdout.txt to tell a machine problem from a broken solution.",
    ],
    caveats: &[
        "The task images are amd64. On an Apple Silicon Mac they run emulated; turn on Rosetta in Docker Desktop's settings (Use Rosetta for x86_64/amd64 emulation) or everything is several times slower and some solutions misbehave.",
        "Docker must be running, not just installed; the check here only sees that it is installed.",
        "`-t` takes the task's full name, terminal-bench/<task>, not <task> alone.",
        "Task images stay in Docker after the sandbox is disposed; `harbor cache clean` removes them.",
        "Running an agent costs API money; every step of this tour is free.",
        "Terminal-Bench 4.0 needs GPU sandboxes on Modal and is not covered here.",
    ],
    requirements,
};

fn requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "harbor".into(),
        rules: vec![
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(
                tool("docker"),
                Severity::Blocks,
                "every task runs in a Docker container",
            ),
            rule(disk(2), Severity::Blocks, "the runner plus one task image"),
        ],
        tiers: vec![tier(
            "whole tour",
            "scoring every task",
            vec![disk(65)],
        )],
    }
}
