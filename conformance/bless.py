#!/usr/bin/env python3
"""Rewrite the expected output in cases.txt from what the shell implementation prints.

    python3 conformance/bless.py          rewrite cases.txt
    python3 conformance/bless.py --check  only say which cases would change

To add a case, write its "[mode] name", "%", "$" and "<" lines into cases.txt and run this.
Then read the diff: this tool records what the reference implementation does, right or wrong.
"""
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CASES = os.path.join(HERE, "cases.txt")
LIB = os.path.join(HERE, "..", "sh", "cli.sh")
TIMED = {"time", "progress", "progress_done", "spin", "tasks", "task", "task_done", "task_fail", "tasks_done", "task_skip", "task_note"}  # their output contains a duration


def decode(text):
    for escape, char in (("\\e", "\x1b"), ("\\t", "\t"), ("\\r", "\r"), ("\\s", " "), ("\\b", "\x7f")):
        text = text.replace(escape, char)
    return text


def encode(stream):
    lines = stream.split("\n")
    if lines[-1] == "":
        lines.pop()
    else:
        lines[-1] += "\\c"
    encoded = []
    for line in lines:
        line = line.replace("\x1b", "\\e").replace("\t", "\\t").replace("\r", "\\r")
        if line.endswith(" "):
            line = line[:-1] + "\\s"
        elif line.endswith(" \\c"):
            line = line[:-3] + "\\s\\c"
        encoded.append(line)
    return encoded


def run(case):
    env = {name: os.environ[name] for name in ("PATH", "HOME", "TMPDIR") if name in os.environ}
    env.update({"CLI_MODE": case["mode"], "COLUMNS": "80", "LANG": "en_US.UTF-8", "TERM": "xterm"})
    env.update(case["env"])
    script = '. "%s"\n' % LIB + "".join("cli_%s\n" % call.replace("\\e", "\x1b") for call in case["calls"])
    stdin = "".join(decode(line) + "\n" for line in case["stdin"]).encode()
    done = subprocess.run(["/bin/bash", "-c", script], input=stdin, env=env, capture_output=True, cwd=HERE)
    out, err = done.stdout.decode(), done.stderr.decode()
    if any(call.split()[0] in TIMED for call in case["calls"]):
        out, err = (re.sub(r"[0-9]+(\.[0-9])?m?s", "<T>", text) for text in (out, err))
    return encode(out), encode(err), done.returncode


def main():
    head, cases, case = [], [], None
    for line in open(CASES, encoding="utf-8").read().split("\n"):
        if line.startswith("["):
            mode, name = line[1:].split("] ", 1)
            case = {"mode": mode, "name": name, "env": {}, "calls": [], "stdin": [], "old": []}
            cases.append(case)
        elif case is None:
            if line.startswith("#"):
                head.append(line)
        elif line.startswith("% "):
            name, value = line[2:].split("=", 1)
            case["env"][name] = value
        elif line.startswith("$ "):
            case["calls"].append(line[2:])
        elif line.startswith("<"):
            case["stdin"].append(line[2:] if line.startswith("< ") else "")
        elif line[:1] in ("|", "!", "="):
            case["old"].append(line)

    text, changed = "\n".join(head) + "\n", []
    for case in cases:
        out, err, status = run(case)
        new = ["|" + line for line in out] + ["!" + line for line in err] + (["= %d" % status] if status else [])
        if new != case["old"]:
            changed.append("[%s] %s" % (case["mode"], case["name"]))
        text += "\n[%s] %s\n" % (case["mode"], case["name"])
        text += "".join("%% %s=%s\n" % pair for pair in case["env"].items())
        text += "".join("$ %s\n" % call for call in case["calls"])
        text += "".join("< %s\n" % line if line else "<\n" for line in case["stdin"])
        text += "".join(line + "\n" for line in new)
    for name in changed:
        print("changed:", name)
    print("%d cases, %d changed" % (len(cases), len(changed)))
    if "--check" not in sys.argv:
        open(CASES, "w", encoding="utf-8").write(text)


if __name__ == "__main__":
    main()
