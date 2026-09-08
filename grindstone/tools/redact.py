#!/usr/bin/env python3
"""Redact a Claude Code transcript JSONL file into a shareable test fixture.

Removes: cwd, all text/thinking/tool_result bodies, file paths, commands,
attachment payloads, toolUseResult blobs. Preserves: the envelope (uuids,
timestamps, promptIds, requestIds, usage, models, flags) plus the derivation-
relevant text skeletons — interrupt markers, <command-name> tags, and denial
phrases — since the sessionizer keys on them. Paths and commands map to
deterministic hashes so equality (retry linkage, retouch detection) survives.

Usage: redact.py <in.jsonl> <out.jsonl>
"""
import hashlib
import json
import re
import sys

DENIAL_PHRASES = [
    "has been denied",
    "was denied",
    "denied by the Claude Code auto mode classifier",
    "The user doesn't want to proceed",
    "User rejected",
    "user declined",
]
INTERRUPT = "[Request interrupted by user"

KEEP_STR_KEYS = {
    "type", "subtype", "uuid", "parentUuid", "sessionId", "session_id",
    "timestamp", "promptId", "requestId", "version", "gitBranch",
    "entrypoint", "userType", "model", "stop_reason", "id", "name",
    "tool_use_id", "operation", "messageId", "snapshotMessageId", "level",
    "role", "effort", "service_tier", "speed", "inference_geo", "backupTime",
    "toolDenialKind", "promptSource", "origin", "leafUuid",
    "sourceToolAssistantUUID", "sourceToolUseID", "interruptedMessageId",
    "caller", "permissionMode", "mode",
}
PATH_KEYS = {"file_path", "notebook_path", "path", "trackingPath", "realParentDir", "backupFileName"}
DROP_KEYS = {"toolUseResult", "classifierMetaLines", "quotaLimits"}


def sha8(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()[:8]


def redact_path(p: str) -> str:
    m = re.search(r"(\.[A-Za-z0-9]{1,8})$", p)
    ext = m.group(1) if m else ""
    return f"/redacted/file_{sha8(p)}{ext}"


def redact_text(s: str) -> str:
    if not s:
        return s
    if s.startswith(INTERRUPT):
        end = s.find("]")
        return s[: end + 1] if end != -1 else INTERRUPT + "]"
    if "<command-name>" in s:
        m = re.search(r"<command-name>(.*?)</command-name>", s, re.S)
        cmd = m.group(1).strip() if m else ""
        return f"<command-name>{cmd}</command-name> <redacted {len(s)} chars>"
    for p in DENIAL_PHRASES:
        if p in s:
            return f"<redacted> {p} <redacted {len(s)} chars>"
    return f"<redacted {len(s)} chars>"


def redact_tool_input(obj):
    if isinstance(obj, dict):
        out = {}
        for k, v in obj.items():
            if k in PATH_KEYS and isinstance(v, str):
                out[k] = redact_path(v)
            elif k == "command" and isinstance(v, str):
                out[k] = f"cmd-{sha8(v.strip())}"
            elif isinstance(v, str):
                out[k] = redact_text(v)
            else:
                out[k] = redact_tool_input(v)
        return out
    if isinstance(obj, list):
        return [redact_tool_input(x) for x in obj]
    return obj


def redact_content_blocks(blocks):
    out = []
    for b in blocks:
        if not isinstance(b, dict):
            out.append({"type": "text", "text": "<redacted>"})
            continue
        t = b.get("type")
        nb = dict(b)
        if t == "text":
            nb["text"] = redact_text(b.get("text") or "")
        elif t == "thinking":
            nb["thinking"] = redact_text(b.get("thinking") or "")
            nb.pop("signature", None)
        elif t == "tool_use":
            nb["input"] = redact_tool_input(b.get("input") or {})
        elif t == "tool_result":
            c = b.get("content")
            if isinstance(c, str):
                nb["content"] = redact_text(c)
            elif isinstance(c, list):
                nb["content"] = redact_content_blocks(c)
        elif t == "image":
            nb = {"type": "text", "text": "<redacted image>"}
        out.append(nb)
    return out


def redact_entry(o):
    o = {k: v for k, v in o.items() if k not in DROP_KEYS}
    t = o.get("type")
    if "cwd" in o:
        o["cwd"] = "/redacted"
    if "message" in o and isinstance(o["message"], dict):
        m = o["message"]
        c = m.get("content")
        if isinstance(c, str):
            m["content"] = redact_text(c)
        elif isinstance(c, list):
            m["content"] = redact_content_blocks(c)
    if t == "attachment":
        o["attachment"] = {"type": (o.get("attachment") or {}).get("type")}
    elif t == "file-history-delta":
        if "trackingPath" in o:
            o["trackingPath"] = redact_path(o["trackingPath"])
        if isinstance(o.get("backup"), dict):
            o["backup"] = redact_tool_input(o["backup"])
    elif t == "file-history-snapshot":
        snap = o.get("snapshot") or {}
        tfb = snap.get("trackedFileBackups") or {}
        snap["trackedFileBackups"] = {
            redact_path(k): redact_tool_input(v) for k, v in tfb.items()
        }
        o["snapshot"] = snap
    elif t == "queue-operation" and "content" in o:
        o["content"] = redact_text(o["content"]) if isinstance(o["content"], str) else "<redacted>"
    elif t == "system" and isinstance(o.get("content"), str):
        o["content"] = redact_text(o["content"])
    # known content-bearing fields on otherwise-unmodeled types
    for k in ("aiTitle", "lastPrompt", "atis", "frameUrl", "title", "prUrl"):
        if k in o:
            o[k] = "<redacted>"
    for k in PATH_KEYS:  # e.g. top-level `path` on frame-link
        if k in o and isinstance(o[k], str):
            o[k] = redact_path(o[k])
    if "error" in o and isinstance(o["error"], str):
        o["error"] = redact_text(o["error"])
    return o


def main():
    src, dst = sys.argv[1], sys.argv[2]
    n = 0
    with open(src) as f, open(dst, "w") as g:
        for line in f:
            line = line.strip()
            if not line:
                continue
            o = json.loads(line)
            g.write(json.dumps(redact_entry(o), separators=(",", ":")) + "\n")
            n += 1
    print(f"{dst}: {n} lines")


if __name__ == "__main__":
    main()
