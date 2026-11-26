#!/usr/bin/env python3
"""Draw what the library prints at a terminal, as the SVG pictures used in the READMEs.

    python3 docs/render.py            every picture, into docs/img/
    python3 docs/render.py tree       only that one

Each picture runs a few lines of shell against sh/cli.sh in rich mode, feeds the output to a
small terminal (colours, carriage return, cursor up, erase), and draws the screen that results.
Nothing is drawn by hand, so a picture shows what the code really prints. A picture that moves
is a recording: the run as it happened, keys typed one at a time, each drawing shown for as
long as it was on the screen.

Lines, bars and dots are drawn as shapes and every other character sits at its own position,
so the picture lines up whatever monospace font the reader has.
"""
import codecs
import os
import re
import select
import subprocess
import sys
import threading
import time
from xml.sax.saxutils import escape

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
CLI = os.path.join(ROOT, "sh", "cli.sh")
OUT = os.path.join(HERE, "img")

CW, LH, SIZE, PAD = 8.4, 20, 14, 16  # cell width, line height, font size, padding
FONT = "ui-monospace,SFMono-Regular,'SF Mono',Menlo,Consolas,'Liberation Mono',monospace"
BACK, EDGE, FORE = "#0d1117", "#30363d", "#c9d1d9"
COLOURS = {31: "#ff7b72", 32: "#3fb950", 33: "#d29922", 34: "#58a6ff", 35: "#bc8cff", 36: "#39c5cf",
           37: FORE, 90: "#8b949e"}
TYPING, PAUSE, HOLD = 0.09, 0.5, 3.5  # seconds: between two keys, between two things typed, on the last frame

BACKUPS = '''"2024-02-01 | iPhone 12 | 38.7 GB" "2024-07-11 | iPad | 11.9 GB" "2025-01-15 | iPhone 12 | 40.2 GB" \\
  "2025-06-01 | iPhone 15 | continues iPhone 12 | iPhone 12" "2025-11-20 | iPad | 12.8 GB" \\
  "2026-09-14 | iPhone 15 | combined with iPad | iPad" "2026-11-20 | iPhone 15 | 44.9 GB"'''
FILES = '''"Photos/2024/rome.jpg | 2.1 MB" "Photos/2024/wedding.jpg | 3.4 MB" "Photos/2025/hiking.jpg | 1.8 MB" \\
  "Notes/todo.txt | 4 KB" "Notes/ideas.md | 12 KB" "Contacts.vcf | 310 KB"'''

# name: the columns of the terminal, the lines shown as typed, the code that runs (or a command
# of its own), and options:
#   keys      what is typed into the prompts. For a moving picture, a list: each item is typed
#             a key at a time (an escape sequence at once), with a pause before the next item.
#   animate   record the run as it happens, and play it back in a loop
#   cut       for a still picture: drop the output from the last match on (back=2: the one before)
SHOTS = {
    "tasks": dict(cols=62, animate=True, show=[
        'cli_tasks "Backup" "Scan | 1" "Photos | 6" "Messages | 2" "Verify | 1"',
        'cli_task Photos "$i" "$n"    # in your loop'], code="""
cli_tasks "Backup" "Scan | 1" "Photos | 6" "Messages | 2" "Verify | 1"
i=0; while [ $i -lt 4 ]; do i=$((i + 1)); cli_task Scan; sleep 0.12; done; cli_task_done Scan
i=0; while [ $i -lt 12 ]; do i=$((i + 1)); sleep 0.12; cli_task Photos $((i * 10)) 120; done
i=0; while [ $i -lt 6 ]; do i=$((i + 1)); sleep 0.12; cli_task Messages $((i * 50)) 300; done
i=0; while [ $i -lt 3 ]; do i=$((i + 1)); cli_task Verify; sleep 0.12; done; cli_task_done Verify
cli_tasks_done"""),
    "stack": dict(cols=78, animate=True, show=['cli_tasks "Sync" "iPhone | 1 | bytes rate eta" "Watch | 1 | percent" ...',
                                               'cli_task_note Watch "verifying"'], code="""
cli_tasks -t 60 "Sync" Passcode "iPhone | 1 | bytes rate eta" "iPad | 1 | bytes rate eta" "Watch | 1 | percent" Mac
cli_task_skip Passcode
i=0
while [ $i -lt 16 ]; do
  i=$((i + 1)); sleep 0.16
  cli_task -t 60 iPhone $((2147483648 + i * 134217728)) 4294967296
  cli_task -t 25 iPad $((i * 201326592)) 6442450944
  cli_task Watch $((i * 5)) 100
  case $i in 3) cli_task_note Watch "verifying";; 6) cli_task_fail Mac "not reachable";; 10) cli_task_note Watch "installing";; esac
done"""),
    "messages": dict(cols=62, animate=True, show=['cli_steps 2', 'cli_step "Build"', 'cli_info "Using the cache"',
                                                  'cli_ok "42 files compiled"', 'cli_warn "2 tests skipped"',
                                                  'cli_error "Connection refused"', 'cli_hint "Try again with --verbose"'],
                     code="""
cli_steps 2
cli_step "Build"; sleep 0.5
cli_info "Using the cache"; sleep 0.5
cli_ok "42 files compiled"; sleep 0.6
cli_step "Deploy"; sleep 0.5
cli_warn "2 tests skipped"; sleep 0.6
cli_error "Connection refused"; sleep 0.5
cli_hint "Try again with --verbose\""""),
    "progress": dict(cols=62, animate=True, show=['cli_progress "$i" 40 "Downloading"     # in your loop',
                                                  'cli_spin "Compiling" make', 'cli_spin "Linking" make link'], code="""
i=0; while [ $i -lt 20 ]; do i=$((i + 1)); sleep 0.07; cli_progress $((i * 2)) 40 "Downloading"; done
cli_spin "Compiling" sleep 1.5
cli_spin "Linking" sh -c "sleep 0.7; echo undefined symbol: main; exit 1" || true
i=0; while [ $i -lt 14 ]; do i=$((i + 1)); sleep 0.09; cli_progress $i 40 "Uploading"; done"""),
    "table": dict(cols=62, show=['cli_table "Item | Qty | Price" "Apple | 3 | 1.20" ...',
                                 'cli_table -k Id "Device | State | Id" ROW...'], code="""
cli_table "Item | Qty | Price" "Apple | 3 | 1.20" "Melon | 12 | 10.00" "Fig | 100 | 0.50"
cli_nl
cli_table -k Id "Device | State | Id" \\
  "📱 Kitchen tablet | $(cli_style green online) | 00008030-000123456789ABCD" \\
  "A phone with a rather long name | $(cli_style red failed) | 00008150-001A2B3C4D5E6F78" \\
  "日本語の名前 | $(cli_style dim never) | 00008101-000A1B2C3D4E5F60\""""),
    "layout": dict(cols=62, code="""
cli_header "Backup"
cli_list "Photos" "Messages" "Settings"
cli_kv "Last run" "2026-11-20 09:14" "Kept in" "/Volumes/Backup"
cli_box -t "Next steps" "Verify the backup" "Unplug the device\""""),
    "timeline": dict(cols=72, show=["cli_timeline ROW...        # WHEN | TRACK | LABEL | FROM", "cli_timeline -v ROW..."],
                     code="cli_timeline %s\ncli_nl\ncli_timeline -v %s" % (BACKUPS, BACKUPS)),
    "tree": dict(cols=50, show=["cli_tree ROW...            # PATH | NOTE"], code="cli_tree %s" % FILES),
    "browse": dict(cols=72, animate=True, cwd=os.path.join(ROOT, "conformance"),
                   keys=["\x1b[B", "\x1b[C", "\x1b[B", "\x1b[C", "\x1b[B", "\x1b[D", "\x1b[D", "\x1b[B", "\n"],
                   show=['file=$(cli_browse "Open which file" files)'], code='cli_browse "Open which file" files'),
    "prompts": dict(cols=62, animate=True, keys=["\x1b[B", "\n", "y", " ", "\x1b[B", "\x1b[B", " ", "\n"],
                    show=['env=$(cli_select "Deploy to" staging production)', 'cli_confirm "Run the migrations?" y',
                          'who=$(cli_multiselect "Notify" email slack pager sms)'], code="""
cli_select "Deploy to" staging production >/dev/null
cli_confirm "Run the migrations?" y
cli_multiselect "Notify" email slack pager sms >/dev/null"""),
    "interactive": dict(cols=72, animate=True, show=["todo -i"],
                        keys=["add Buy milk", "\n", "li", "\t", "\n", "\t", "\t", "\t", "\x15", "exit", "\n"],
                        code='TODO_FILE=$(mktemp) "%s" -i' % os.path.join(ROOT, "sh", "examples", "todo.sh")),
    # The line editor is in the Rust crate: this one runs its demo (cargo build --features clap --example demo).
    "editor": dict(cols=78, animate=True, show=["demo editor"],
                   keys=["op", "\t", "\t", "\n", "open No", "\t", "I", "\t", "\n", "ex", "\t", "\n"],
                   command=[os.path.join(ROOT, "rust", "target", "debug", "examples", "demo"), "editor"]),
    "help": dict(cols=72, show=["todo --help"], code='TODO_FILE=/dev/null "%s" --help' % os.path.join(ROOT, "sh", "examples", "todo.sh")),
}


WIDE = ((0x1100, 0x115F), (0x2E80, 0x303E), (0x3041, 0x33FF), (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xA000, 0xA4CF),
        (0xAC00, 0xD7A3), (0xF900, 0xFAFF), (0xFE30, 0xFE4F), (0xFF00, 0xFF60), (0xFFE0, 0xFFE6), (0x1F300, 0x1F64F),
        (0x1F680, 0x1F6FF), (0x1F900, 0x1F9FF), (0x20000, 0x3FFFD))


def wide(ch):
    return any(low <= ord(ch) <= high for low, high in WIDE)


class Screen:
    """As much of a terminal as the library uses."""

    def __init__(self):
        self.rows, self.r, self.c = [[]], 0, 0
        self.fg, self.bold, self.dim = None, False, False

    def put(self, ch):
        row = self.rows[self.r]
        while len(row) <= self.c + 1:
            row.append((" ", None, False, False))
        row[self.c] = (ch, self.fg, self.bold, self.dim)
        self.c += 2 if wide(ch) else 1  # CJK and emoji take two cells, as on a terminal

    def feed(self, text):
        i = 0
        while i < len(text):
            ch = text[i]
            csi = re.match(r"\x1b\[([0-9;]*)([A-Za-z])", text[i:]) if ch == "\x1b" else None
            if csi:
                args, kind = csi.group(1), csi.group(2)
                n = int(args) if args.isdigit() else 1
                if kind == "m":
                    for code in [int(a) for a in args.split(";") if a] or [0]:
                        if code == 0:
                            self.fg, self.bold, self.dim = None, False, False
                        elif code == 1:
                            self.bold = True
                        elif code == 2:
                            self.dim = True
                        elif code in COLOURS:
                            self.fg = COLOURS[code]
                elif kind == "A":
                    self.r = max(0, self.r - n)
                elif kind == "D":
                    self.c = max(0, self.c - n)
                elif kind == "K":
                    del self.rows[self.r][self.c:]
                elif kind == "J":
                    del self.rows[self.r][self.c:]
                    del self.rows[self.r + 1:]
                i += csi.end()
                continue
            if ch == "\n":
                self.r, self.c = self.r + 1, 0
                while len(self.rows) <= self.r:
                    self.rows.append([])
            elif ch == "\r":
                self.c = 0
            elif ch >= " ":
                self.put(ch)
            i += 1

    def snapshot(self):
        rows = [list(row) for row in self.rows]
        while rows and not "".join(cell[0] for cell in rows[-1]).strip():
            rows.pop()
        return rows


# Lines: which sides of the cell the character reaches (left, right, up, down), and a round corner.
LINES = {"─": "lr", "│": "ud", "├": "udr", "┤": "udl", "┬": "lrd", "┴": "lru", "┼": "lrud",
         "╭": "rd", "╮": "ld", "╰": "ur", "╯": "ul"}
ROUND = "╭╮╰╯"


def shape(ch, x, y, colour, opacity):
    """A character that is drawn, not written; None for the others."""
    cx, cy, look = x + CW / 2, y + LH / 2, 'opacity="%s"' % opacity if opacity < 1 else ""
    if ch in "█░":
        return ""  # drawn by bar(), a whole run at once
    if ch in "●◉○":
        ring = '<circle cx="%.1f" cy="%.1f" r="4.2" fill="none" stroke="%s" stroke-width="1.4" %s/>' % (cx, cy, colour, look)
        dot = '<circle cx="%.1f" cy="%.1f" r="%s" fill="%s" %s/>' % (cx, cy, 2 if ch == "◉" else 3.6, colour, look)
        return {"●": dot, "◉": ring + dot, "○": ring}[ch]
    if ch not in LINES:
        return None
    ends = {"l": (x - 0.2, cy), "r": (x + CW + 0.2, cy), "u": (cx, y - 0.2), "d": (cx, y + LH + 0.2)}
    sides = LINES[ch]
    if ch in ROUND:  # one stroke, from one side through a quarter circle to the other
        (ax, ay), (bx, by), k = ends[sides[0]], ends[sides[1]], 3.6
        path = "M%.1f %.1f L%.1f %.1f Q%.1f %.1f %.1f %.1f L%.1f %.1f" % (
            ax, ay, cx + (ax - cx) * k / (CW / 2) if ax != cx else cx, cy + (ay - cy) * k / (LH / 2) if ay != cy else cy,
            cx, cy, cx + (bx - cx) * k / (CW / 2) if bx != cx else cx, cy + (by - cy) * k / (LH / 2) if by != cy else cy,
            bx, by)
    else:
        path = " ".join("M%.1f %.1f L%.1f %.1f" % (cx, cy, *ends[side]) for side in sides)
    return '<path d="%s" fill="none" stroke="%s" stroke-width="1.3" stroke-linecap="square" %s/>' % (path, colour, look)


def bars(row, y):
    """The cells of a bar: neighbours of one kind and colour are one rectangle, without seams."""
    parts, c = [], 0
    while c < len(row):
        ch, fg, _, dim = row[c]
        end = c
        while ch in "█░" and end < len(row) and row[end][0] == ch and row[end][1] == fg and row[end][3] == dim:
            end += 1
        if end > c:
            fade = (0.55 if dim else 1) * (1 if ch == "█" else 0.3)
            parts.append('<rect x="%.1f" y="%.1f" width="%.1f" height="%.1f" fill="%s" opacity="%.2f"/>' % (
                PAD + c * CW, y + 4, (end - c) * CW, LH - 8, fg or FORE, fade))
        c = max(end, c + 1)
    return parts


def draw(rows):
    """The SVG elements of one screen."""
    parts = []
    for r, row in enumerate(rows):
        y, run = PAD + r * LH, None  # run: [style, xs, characters]
        parts.extend(bars(row, y))
        for c, (ch, fg, bold, dim) in enumerate(row + [(" ", None, False, False)]):
            x, colour, opacity = PAD + c * CW, fg or FORE, 0.55 if dim else 1
            drawn = shape(ch, x, y, colour, opacity)
            style = (colour, bold, opacity)
            if run and (ch == " " or drawn is not None or run[0] != style):
                colour_, bold_, opacity_ = run[0]
                parts.append('<text y="%.1f" x="%s" fill="%s"%s%s>%s</text>' % (
                    y + LH * 0.72, " ".join("%.1f" % at for at in run[1]), colour_,
                    ' font-weight="700"' if bold_ else "", ' opacity="%s"' % opacity_ if opacity_ < 1 else "",
                    escape("".join(run[2]))))
                run = None
            if drawn is not None:
                parts.append(drawn)
            elif ch != " ":
                run = run or [style, [], []]
                run[1].append(x)
                run[2].append(ch)
    return "".join(parts)


def svg(frames, cols, times=None):
    """One picture. Several frames play in a loop, each for its time, the last one held."""
    height = PAD * 2 + max(len(rows) for rows in frames) * LH
    width = PAD * 2 + cols * CW
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %.0f %.0f" width="%.0f" height="%.0f" '
           'font-family="%s" font-size="%s">' % (width, height, width, height, FONT, SIZE)]
    if len(frames) > 1:
        total, at, rules = sum(times), 0.0, []
        for i, time_on_show in enumerate(times):
            # Each frame is on show for its own slice of the loop; step-end switches without blending.
            start, at = 100 * at / total, at + time_on_show
            end = 100 if i == len(frames) - 1 else 100 * at / total
            stops = {0.0: "hidden", 100.0: "hidden"}
            stops[round(start, 3)] = "visible"
            stops[round(end, 3)] = "visible" if end == 100 else "hidden"
            keys = "".join("%s%%{visibility:%s}" % (stop, stops[stop]) for stop in sorted(stops))
            rules.append("@keyframes f%d{%s}.f%d{animation:f%d %.2fs step-end infinite}" % (i, keys, i, i, total))
        out.append("<style>.f{visibility:hidden}%s</style>" % "".join(rules))
    out.append('<rect x="0.5" y="0.5" width="%.0f" height="%.0f" rx="8" fill="%s" stroke="%s"/>' % (width - 1, height - 1, BACK, EDGE))
    for i, rows in enumerate(frames):
        out.append('<g class="f f%d">%s</g>' % (i, draw(rows)) if len(frames) > 1 else draw(rows))
    out.append("</svg>\n")
    return "".join(out)


def record(command, env, cwd, keys):
    """Run a command and return what it printed as it printed it: (seconds since the start, text)
    pairs. The keys are typed meanwhile, the way a person would: a key at a time, with pauses."""
    process = subprocess.Popen(command, env=env, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT)

    def type_keys():
        try:
            for item in keys:
                time.sleep(PAUSE)
                for key in [item] if item.startswith("\x1b") else item:
                    process.stdin.write(key.encode())
                    process.stdin.flush()
                    time.sleep(TYPING)
            time.sleep(PAUSE)
            process.stdin.close()
        except OSError:
            pass  # the program left before the last key

    threading.Thread(target=type_keys, daemon=True).start()
    start, events, text = time.time(), [], codecs.getincrementaldecoder("utf-8")("replace")
    stream = process.stdout.fileno()
    while True:
        if not select.select([stream], [], [], 0.05)[0]:
            continue
        at, data = time.time() - start, os.read(stream, 65536)
        if not data:
            break
        while select.select([stream], [], [], 0.02)[0]:  # the rest of one drawing arrives at once
            more = os.read(stream, 65536)
            if not more:
                break
            data += more
        events.append((at, text.decode(data)))
    process.wait()
    return events


def render(name, shot):
    env = {key: os.environ[key] for key in ("PATH", "HOME", "TMPDIR") if key in os.environ}
    env.update(CLI_MODE="rich", COLUMNS=str(shot["cols"]), LANG="en_US.UTF-8", TERM="xterm", CLI_AGENT="0")
    if "keys" in shot:
        env["CLI_INTERACTIVE"] = "1"
    command = shot.get("command") or ["bash", "-c", '. "%s"\n%s' % (CLI, shot["code"])]
    if not os.path.exists(command[0]) and os.sep in command[0]:
        print("%-12s skipped: %s is not built" % (name, os.path.relpath(command[0], ROOT)))
        return
    screen = Screen()
    shown = shot.get("show") or shot["code"].strip().split("\n")
    screen.feed("".join("\x1b[90m$\x1b[0m %s\n" % line for line in shown) + "\n")
    frames, times = [], []
    if shot.get("animate"):
        events = record(command, env, shot.get("cwd", ROOT), shot.get("keys", []))
        for i, (at, text) in enumerate(events):
            screen.feed(text)
            # On show until the next drawing: not shorter than the eye needs, not longer than a pause.
            pause = events[i + 1][0] - at if i + 1 < len(events) else HOLD
            pause = HOLD if i + 1 == len(events) else min(max(pause, 0.06), 1.2)
            rows = screen.snapshot()
            if frames and rows == frames[-1]:
                times[-1] += pause
            else:
                frames.append(rows)
                times.append(pause)
    else:
        done = subprocess.run(command, env=env, cwd=shot.get("cwd", ROOT), input="".join(shot.get("keys", "")).encode(),
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        raw = done.stdout.decode()
        if "cut" in shot:
            raw = raw[:[match.start() for match in re.finditer(shot["cut"], raw)][-shot.get("back", 1)]]
        screen.feed(raw)
        frames.append(screen.snapshot())
    used = max(len(row) for rows in frames for row in rows)
    path = os.path.join(OUT, name + ".svg")
    with open(path, "w", encoding="utf-8") as file:
        file.write(svg(frames, max(used, 40), times))
    seconds = " %4.1fs" % sum(times) if times else ""
    print("%-12s %2d frame%s %6.1f KB%s" % (name, len(frames), " " if len(frames) == 1 else "s", os.path.getsize(path) / 1024, seconds))


def main():
    os.makedirs(OUT, exist_ok=True)
    names = sys.argv[1:] or list(SHOTS)
    for name in names:
        if name not in SHOTS:
            sys.exit("no such picture: %s (there are: %s)" % (name, ", ".join(SHOTS)))
        render(name, SHOTS[name])


if __name__ == "__main__":
    main()
