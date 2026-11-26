#!/usr/bin/env python3
"""What cases.txt cannot check: a real terminal.

Every key-driven prompt changes how the terminal behaves while it waits (no echo, no line
buffering). This starts each prompt in a pseudo-terminal, presses Ctrl-C, and checks that the
terminal is back to normal and the program has stopped.

    python3 conformance/terminal.py

It covers every shell found on this machine, and the Rust demo if it has been built
(cd rust && cargo build --features clap --example demo).
"""
import os
import pty
import re
import select
import shutil
import sys
import termios
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLI = os.path.join(ROOT, "sh", "cli.sh")
TODO = os.path.join(ROOT, "sh", "examples", "todo.sh")
RUST_DEMO = os.path.join(ROOT, "rust", "target", "debug", "examples", "demo")

# The program runs under a parent that survives Ctrl-C, as it would under an interactive shell.
# Started directly it would lead the session, and its death would hang up the terminal before
# anything could restore it.
PARENT = """
import signal, subprocess, sys, time
signal.signal(signal.SIGINT, signal.SIG_IGN)
child = subprocess.Popen(sys.argv[1:], preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))
child.wait()
sys.stderr.write("STATUS=%d\\n" % child.returncode)
sys.stderr.flush()
time.sleep(1.5)
"""


def interrupted(argv, keys=b""):
    """Start argv, type keys, press Ctrl-C. Returns (terminal is normal, how the program ended)."""
    env = {name: os.environ[name] for name in ("PATH", "HOME", "TMPDIR") if name in os.environ}
    env.update({"TERM": "xterm-256color", "LANG": "en_US.UTF-8", "CLI_AGENT": "0", "TODO_FILE": os.devnull})
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe(sys.executable, [sys.executable, "-c", PARENT] + argv, env)
    seen = b""

    def wait(seconds):
        nonlocal seen
        end = time.time() + seconds
        while time.time() < end:
            if select.select([fd], [], [], 0.05)[0]:
                try:
                    seen += os.read(fd, 65536)
                except OSError:
                    return

    wait(1.2)
    if keys:
        os.write(fd, keys)
        wait(0.4)
    os.write(fd, b"\x03")
    wait(0.8)
    flags = termios.tcgetattr(fd)[3]
    normal = all(flags & flag for flag in (termios.ICANON, termios.ECHO, termios.ISIG))
    status = re.search(rb"STATUS=(-?[0-9]+)", seen)
    try:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
    except OSError:
        pass
    if not status:
        return normal, "still running"
    code = int(status.group(1))
    return normal, "killed by signal %d" % -code if code < 0 else "exit %d" % code


def main():
    prompts = [  # what to run through cli.sh, and what to type before Ctrl-C
        (["select", "Pick", "a", "b", "c"], b"\x1b[B"),
        (["multiselect", "Pick", "a", "b", "c"], b" "),
        (["confirm", "Sure?"], b""),
        (["input", "-c", "main", "-c", "dev", "Branch"], b"m\t"),
        (["password", "Secret"], b"ab"),
        (["pause"], b""),
        (["browse", "Pick", ROOT], b"\x1b[B\x1b[C"),
        (["browse", "-l", "Pick", "a/b", "a/c", "d"], b"\x1b[C"),
    ]
    runs = []
    shells = [shell for shell in ("/bin/bash", shutil.which("bash"), shutil.which("zsh")) if shell]
    for shell in sorted(set(shells)):
        for call, keys in prompts:
            runs.append(("%s: %s" % (shell, call[0]), [shell, CLI] + call, keys))
        runs.append(("%s: interactive mode" % shell, [shell, TODO, "-i"], b"li"))
    if os.path.exists(RUST_DEMO):
        runs.append(("rust: menu", [RUST_DEMO], b"\x1b[B"))
        runs.append(("rust: wizard", [RUST_DEMO, "prompts"], b"An"))
        runs.append(("rust: browse", [RUST_DEMO, "tree"], b"\x1b[B\x1b[C"))
        runs.append(("rust: interactive mode", [RUST_DEMO, "interactive"], b"li"))
    else:
        print("(the Rust demo is not built; skipping it)")

    failed = 0
    for name, argv, keys in runs:
        normal, ended = interrupted(argv, keys)
        good = normal and ended != "still running"
        failed += not good
        if not good:
            print("FAIL %s: terminal %s, program %s" % (name, "normal" if normal else "left without echo", ended))
    print("%d prompts interrupted, %d left the terminal broken or kept running" % (len(runs), failed))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
