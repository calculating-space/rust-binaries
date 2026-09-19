"""Harbor agent: Claude Code with sieve's PostToolUse hook installed.

Harbor (the Terminal-Bench runner) installs Claude Code into each task
container and runs it with a per-run settings file. This agent is Harbor's
own ``claude-code`` agent plus three things:

1. the Linux sieve binary is uploaded to ``/usr/local/bin/sieve``;
2. the settings file carries sieve's hook, the same JSON that
   ``sieve install --dry-run`` prints, so every Bash result goes through it;
3. ``SIEVE_HOME`` points inside the agent's log directory, so the ledger and
   spill files are copied out of the container with the transcript.

Everything else (model, timeouts, transcript export) is inherited unchanged,
which is the point: the only difference between this agent and ``claude-code``
is the hook. ``--ak hook=false`` runs the same agent, same install path,
without the hook: the control arm of an A/B run.

Claude Code is installed with ``npm install -g`` rather than the native
installer, because the native installer's binary segfaults under qemu when
an amd64 task image runs on an Apple Silicon host. ``--ak npm_install=false``
restores Harbor's default installer.

Build the binary first, in ``sieve/``::

    cross build --release --target x86_64-unknown-linux-musl    # terminal-bench-2 images are amd64
    cross build --release --target aarch64-unknown-linux-musl   # for arm64 images

Run from the repository root::

    PYTHONPATH=. harbor run -d terminal-bench/terminal-bench-2 \\
        -a sieve.harbor.agent:SieveClaudeCode -m anthropic/<model> \\
        -t terminal-bench/regex-log

``--install-only`` exercises the install without an API key. Agent kwargs
(``--ak key=value``): ``hook`` (default true), ``npm_install`` (default
true), ``sieve_binary`` (path override).
"""

from __future__ import annotations

import json
import shlex
from pathlib import Path, PurePosixPath
from typing import Any

from harbor.agents.installed.claude_code import ClaudeCode
from harbor.environments.base import BaseEnvironment
from harbor.models.trial.paths import EnvironmentPaths

REMOTE_BIN = PurePosixPath("/usr/local/bin/sieve")
PACKAGE_DIR = Path(__file__).resolve().parents[1]

# `uname -m` inside the container -> Rust target whose build we upload.
TARGETS = {
    "aarch64": "aarch64-unknown-linux-musl",
    "arm64": "aarch64-unknown-linux-musl",
    "x86_64": "x86_64-unknown-linux-musl",
    "amd64": "x86_64-unknown-linux-musl",
}


def hook_settings(command_prefix: str = f"{REMOTE_BIN} hook") -> dict[str, Any]:
    """The hooks block sieve registers, as `sieve install` writes it."""
    return {
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "command",
                            "command": f"{command_prefix} PostToolUse",
                            "timeout": 5,
                        }
                    ],
                }
            ]
        }
    }


def _merge_hooks(base: dict[str, Any], hooks: dict[str, Any]) -> dict[str, Any]:
    """`base` plus sieve's hook entries, without touching other hooks."""
    merged = json.loads(json.dumps(base))
    target = merged.setdefault("hooks", {})
    for event, entries in hooks["hooks"].items():
        existing = target.setdefault(event, [])
        for entry in entries:
            if entry not in existing:
                existing.append(entry)
    return merged


def _flag(value: Any, default: bool) -> bool:
    """Agent kwargs arrive as strings from `--ak key=value`."""
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


class SieveClaudeCode(ClaudeCode):
    """Harbor's claude-code agent with sieve's Bash hook in the container."""

    def __init__(
        self,
        logs_dir: Path,
        *args: Any,
        hook: Any = None,
        npm_install: Any = None,
        sieve_binary: str | Path | None = None,
        config: Path | str | dict[str, Any] | None = None,
        extra_env: dict[str, str] | None = None,
        environment_logs_dir: PurePosixPath | None = None,
        **kwargs: Any,
    ):
        self.hook = _flag(hook, True)
        self.npm_install = _flag(npm_install, True)
        self._sieve_binary = Path(sieve_binary).expanduser() if sieve_binary else None

        remote_logs = environment_logs_dir or EnvironmentPaths.agent_dir
        self.sieve_home = PurePosixPath(remote_logs) / "sieve"

        env = dict(extra_env or {})
        if self.hook:
            env.setdefault("SIEVE_HOME", self.sieve_home.as_posix())

        if config is None:
            base: dict[str, Any] = {}
        elif isinstance(config, dict):
            base = config
        else:
            base = json.loads(Path(config).expanduser().read_text())
        settings = _merge_hooks(base, hook_settings()) if self.hook else base

        super().__init__(
            logs_dir,
            *args,
            config=settings or None,
            extra_env=env,
            environment_logs_dir=environment_logs_dir,
            **kwargs,
        )

    @staticmethod
    def name() -> str:
        return "sieve-claude-code"

    def _binary_for(self, arch: str) -> Path:
        if self._sieve_binary is not None:
            if not self._sieve_binary.is_file():
                raise RuntimeError(f"sieve_binary not found: {self._sieve_binary}")
            return self._sieve_binary
        target = TARGETS.get(arch)
        if target is None:
            raise RuntimeError(f"no sieve build target known for container arch {arch!r}")
        path = PACKAGE_DIR / "target" / target / "release" / "sieve"
        if not path.is_file():
            raise RuntimeError(
                f"no Linux sieve binary at {path}; build it in {PACKAGE_DIR} with "
                f"`cross build --release --target {target}`"
            )
        return path

    async def _install_claude_via_npm(self, environment: BaseEnvironment) -> None:
        """`npm install -g` works under qemu where the native installer does not."""
        await self.ensure_system_dependencies(
            environment, ("curl", "bash", "nodejs", "npm", "procps")
        )
        spec = "@anthropic-ai/claude-code" + (f"@{self._version}" if self._version else "")
        await self.exec_as_root(
            environment,
            command=f"npm install -g {shlex.quote(spec)} && claude --version",
        )

    async def install(self, environment: BaseEnvironment) -> None:
        if self.npm_install:
            await self._install_claude_via_npm(environment)
        # Skips itself when `claude` already satisfies the requested version.
        await super().install(environment)

        if not self.hook:
            self.logger.info("sieve hook disabled (hook=false); control arm")
            return

        result = await environment.exec(command="uname -m")
        arch = (result.stdout or "").strip()
        binary = self._binary_for(arch)

        remote = REMOTE_BIN.as_posix()
        await environment.upload_file(binary, remote)
        await self.exec_as_root(
            environment,
            command=f"chmod 755 {shlex.quote(remote)} && {shlex.quote(remote)} --version",
        )
        await self.exec_as_agent(
            environment,
            command=f"mkdir -p {shlex.quote(self.sieve_home.as_posix())}",
        )
        self.logger.info(
            "sieve %s installed at %s; SIEVE_HOME=%s",
            binary.name,
            remote,
            self.sieve_home,
        )
