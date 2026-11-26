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
RUST_PASSWORD = os.path.join(ROOT, "rust", "target", "debug", "examples", "password")

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


def interrupted(argv, keys=b"", presses=1):
    """Start argv, type keys, press Ctrl-C (`presses` times). Returns (terminal is normal, how the
    program ended, the program was still running before the last press)."""
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
    survived = True
    for press in range(presses):
        survived = b"STATUS=" not in seen
        os.write(fd, b"\x03")
        wait(0.8 if presses == 1 else 1.4)  # bash 3.2 looks for Ctrl-C once a second
    flags = termios.tcgetattr(fd)[3]
    normal = all(flags & flag for flag in (termios.ICANON, termios.ECHO, termios.ISIG))
    status = re.search(rb"STATUS=(-?[0-9]+)", seen)
    try:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
    except OSError:
        pass
    if not status:
        return normal, "still running", survived
    code = int(status.group(1))
    return normal, "killed by signal %d" % -code if code < 0 else "exit %d" % code, survived


def typed_in_secret(command):
    """Run `command | cat` and type a password the moment the question shows, as a paste or a
    script would. Returns (the question was shown, times the password appeared)."""
    env = {name: os.environ[name] for name in ("PATH", "HOME", "TMPDIR") if name in os.environ}
    env.update({"TERM": "xterm-256color", "LANG": "en_US.UTF-8", "CLI_AGENT": "0"})
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe("/bin/sh", ["/bin/sh", "-c", command + " | cat"], env)
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

    end = time.time() + 5
    while b"Password" not in seen and time.time() < end:
        wait(0.005)
    asked = b"Password" in seen
    os.write(fd, b"hunter2\r")
    wait(1.0)
    try:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
    except OSError:
        pass
    return asked, seen.count(b"hunter2")


def screen(command):
    """Run a shell command in a pseudo-terminal and return what reached the terminal."""
    env = {name: os.environ[name] for name in ("PATH", "HOME", "TMPDIR") if name in os.environ}
    env.update({"TERM": "xterm-256color", "LANG": "en_US.UTF-8", "CLI_AGENT": "0"})
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe("/bin/sh", ["/bin/sh", "-c", command], env)
    seen, end = b"", time.time() + 10
    while time.time() < end:
        if select.select([fd], [], [], 0.05)[0]:
            try:
                data = os.read(fd, 65536)
            except OSError:
                break
            if not data:
                break
            seen += data
    try:
        os.waitpid(pid, 0)
    except OSError:
        pass
    return seen


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
        # At a command prompt Ctrl-C drops the line; a second one, on the empty line, leaves.
        runs.append(("%s: interactive mode" % shell, [shell, TODO, "-i"], b"li", 2))
    if os.path.exists(RUST_DEMO):
        runs.append(("rust: menu", [RUST_DEMO], b"\x1b[B"))
        runs.append(("rust: wizard", [RUST_DEMO, "prompts"], b"An"))
        runs.append(("rust: browse", [RUST_DEMO, "tree"], b"\x1b[B\x1b[C"))
        runs.append(("rust: interactive mode", [RUST_DEMO, "interactive"], b"li", 2))
    else:
        print("(the Rust demo is not built; skipping it)")

    failed = 0
    for name, argv, keys, *presses in runs:
        normal, ended, survived = interrupted(argv, keys, *presses)
        good = normal and ended != "still running" and survived
        failed += not good
        if not good:
            print("FAIL %s: terminal %s, program %s%s" % (name, "normal" if normal else "left without echo", ended,
                                                          "" if survived else ", and it ended at the first Ctrl-C"))
    print("%d prompts interrupted, %d left the terminal broken or kept running" % (len(runs), failed))

    # A password is hidden when the output is a pipe too: it must appear once, where `cat` prints it.
    secrets = [("%s: password" % shell, '%s "%s" password Password' % (shell, CLI)) for shell in sorted(set(shells))]
    if os.path.exists(RUST_PASSWORD):
        secrets.append(("rust: password", '"%s"' % RUST_PASSWORD))
    shown = 0
    for name, command in secrets:
        asked, times = typed_in_secret(command)
        if not asked or times != 1:
            shown += 1
            print("FAIL %s: question %s, password on the screen %d times" % (name, "shown" if asked else "missing", times))
    print("%d passwords typed with the output piped, %d shown or asked without a question" % (len(secrets), shown))

    # Each stream on its own: with the output piped, a status line at the terminal is still
    # rich (a green tick) and the content in the pipe is plain (an ASCII bullet).
    split = [("%s: streams" % shell, '%s "%s" ok Built | cat; %s "%s" list item | cat' % (shell, CLI, shell, CLI))
             for shell in sorted(set(shells))]
    if os.path.exists(RUST_DEMO):
        split.append(("rust: streams", 'DEMO_FAST=1 "%s" lists | cat' % RUST_DEMO))
    wrong = 0
    for name, command in split:
        seen = screen(command)
        tick, bullet = b"\x1b[32m\xe2\x9c\x93" in seen or name.startswith("rust"), b"  - " in seen
        dimmed = b"\x1b[2m" in seen
        if not (tick and bullet and (dimmed or not name.startswith("rust"))):
            wrong += 1
            print("FAIL %s: rich status %s, plain content %s" % (name, tick and dimmed, bullet))
    print("%d runs with the output piped, %d without rich status lines or with coloured content" % (len(split), wrong))
    sys.exit(1 if failed or shown or wrong else 0)


if __name__ == "__main__":
    main()
