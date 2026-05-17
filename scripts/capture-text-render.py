#!/usr/bin/env python3
"""Capture hitmap text renderer output and render it to a PNG."""

from __future__ import annotations

import argparse
import fcntl
import math
import os
import pty
import re
import select
import struct
import subprocess
import sys
import termios
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ModuleNotFoundError as exc:  # pragma: no cover - runtime dependency check
    raise SystemExit(
        "Pillow is required for text preview capture. Install it with: python3 -m pip install pillow"
    ) from exc

THEMES = {
    "light": {
        "background": (255, 255, 255, 255),
        "foreground": (36, 41, 47, 255),
    },
    "dark": {
        "background": (13, 17, 23, 255),
        "foreground": (201, 209, 217, 255),
    },
}

ANSI_SGR_RE = re.compile(r"\x1b\[([0-9;]*)m")
DEFAULT_FONT_CANDIDATES = [
    "/System/Library/Fonts/Supplemental/Menlo.ttc",
    "/System/Library/Fonts/Menlo.ttc",
    "/Library/Fonts/Menlo.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "C:/Windows/Fonts/consola.ttf",
    "C:/Windows/Fonts/cour.ttf",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Capture hitmap text output in a PTY and save a PNG preview."
    )
    parser.add_argument("--bin", required=True, help="Path to the hitmap binary")
    parser.add_argument("--repo", required=True, help="Repository path to render")
    parser.add_argument("--output", required=True, help="PNG file to write")
    parser.add_argument("--last", default="1y", help="Rolling range to render")
    parser.add_argument("--theme", default="light", choices=sorted(THEMES.keys()))
    parser.add_argument("--color-profile", default="github")
    parser.add_argument("--columns", type=int, default=120)
    parser.add_argument("--rows", type=int, default=20)
    parser.add_argument(
        "--font-size",
        type=int,
        default=int(os.environ.get("HITMAP_TEXT_FONT_SIZE", "18")),
    )
    parser.add_argument(
        "--padding-x",
        type=int,
        default=int(os.environ.get("HITMAP_TEXT_PADDING_X", "24")),
    )
    parser.add_argument(
        "--padding-y",
        type=int,
        default=int(os.environ.get("HITMAP_TEXT_PADDING_Y", "20")),
    )
    parser.add_argument(
        "--font",
        default=os.environ.get("HITMAP_TEXT_FONT"),
        help="Optional monospace font override",
    )
    return parser.parse_args()


def capture_ansi(args: argparse.Namespace) -> bytes:
    env = os.environ.copy()
    env["TERM"] = env.get("TERM", "xterm-256color") or "xterm-256color"
    env.pop("NO_COLOR", None)

    command = [
        args.bin,
        "render",
        "--renderer",
        "text",
        "--last",
        args.last,
        "--theme",
        args.theme,
        "--color-profile",
        args.color_profile,
        "--max-width-cells",
        str(args.columns),
        args.repo,
    ]

    master_fd, slave_fd = pty.openpty()
    fcntl.ioctl(
        slave_fd,
        termios.TIOCSWINSZ,
        struct.pack("HHHH", args.rows, args.columns, 0, 0),
    )

    proc = subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        stdout=slave_fd,
        stderr=subprocess.PIPE,
        env=env,
        close_fds=True,
    )
    os.close(slave_fd)

    chunks: list[bytes] = []
    while True:
        ready, _, _ = select.select([master_fd], [], [], 0.1)
        if master_fd in ready:
            try:
                chunk = os.read(master_fd, 4096)
            except OSError:
                chunk = b""
            if chunk:
                chunks.append(chunk)
            elif proc.poll() is not None:
                break
        elif proc.poll() is not None:
            break

    os.close(master_fd)
    stderr = proc.stderr.read().decode("utf-8", "replace") if proc.stderr else ""
    return_code = proc.wait()
    if return_code != 0:
        if stderr:
            sys.stderr.write(stderr)
        raise SystemExit(return_code)

    return b"".join(chunks)


def load_font(font_override: str | None, font_size: int) -> tuple[ImageFont.FreeTypeFont, str]:
    candidates = []
    if font_override:
        candidates.append(font_override)
    candidates.extend(DEFAULT_FONT_CANDIDATES)

    for candidate in candidates:
        if candidate and os.path.exists(candidate):
            try:
                return ImageFont.truetype(candidate, font_size), candidate
            except OSError:
                continue

    raise SystemExit(
        "No suitable monospace font found. Set HITMAP_TEXT_FONT=/path/to/font.ttf and re-run."
    )


def decode_lines(ansi_bytes: bytes, default_fg: tuple[int, int, int, int]) -> list[list[tuple[str, tuple[int, int, int, int]]]]:
    text = ansi_bytes.decode("utf-8", "replace").replace("\r\n", "\n").replace("\r", "")
    lines: list[list[tuple[str, tuple[int, int, int, int]]]] = [[]]
    current_fg = default_fg
    index = 0

    while index < len(text):
        match = ANSI_SGR_RE.match(text, index)
        if match:
            codes = [code for code in match.group(1).split(";") if code]
            if not codes:
                codes = ["0"]
            pos = 0
            while pos < len(codes):
                code = codes[pos]
                if code == "0":
                    current_fg = default_fg
                    pos += 1
                elif code == "39":
                    current_fg = default_fg
                    pos += 1
                elif code == "38" and pos + 4 < len(codes) and codes[pos + 1] == "2":
                    current_fg = (
                        int(codes[pos + 2]),
                        int(codes[pos + 3]),
                        int(codes[pos + 4]),
                        255,
                    )
                    pos += 5
                else:
                    pos += 1
            index = match.end()
            continue

        ch = text[index]
        index += 1
        if ch == "\n":
            lines.append([])
            continue

        lines[-1].append((ch, current_fg))

    while lines and not lines[-1]:
        lines.pop()

    for line in lines:
        while line and line[-1][0] == " ":
            line.pop()

    if not lines:
        raise SystemExit("Captured text preview was empty")

    return lines


def render_png(
    lines: list[list[tuple[str, tuple[int, int, int, int]]]],
    font: ImageFont.FreeTypeFont,
    background: tuple[int, int, int, int],
    output_path: Path,
    padding_x: int,
    padding_y: int,
) -> None:
    probe = Image.new("RGBA", (1, 1), background)
    draw = ImageDraw.Draw(probe)
    char_width = max(1, math.ceil(draw.textlength("M", font=font)))
    ascent, descent = font.getmetrics()
    line_height = max(1, ascent + descent + 3)
    bbox = draw.textbbox((0, 0), "Ag", font=font)
    top_adjust = -min(0, bbox[1])
    max_columns = max(len(line) for line in lines)

    image_width = padding_x * 2 + max_columns * char_width
    image_height = padding_y * 2 + top_adjust + len(lines) * line_height
    image = Image.new("RGBA", (image_width, image_height), background)
    draw = ImageDraw.Draw(image)

    for row, line in enumerate(lines):
        y = padding_y + row * line_height + top_adjust
        for column, (ch, color) in enumerate(line):
            x = padding_x + column * char_width
            draw.text((x, y), ch, font=font, fill=color)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(output_path)


def main() -> None:
    args = parse_args()
    theme = THEMES[args.theme]
    ansi_bytes = capture_ansi(args)
    lines = decode_lines(ansi_bytes, theme["foreground"])
    font, font_path = load_font(args.font, args.font_size)
    render_png(
        lines,
        font,
        theme["background"],
        Path(args.output),
        args.padding_x,
        args.padding_y,
    )
    print(f"Text preview font: {font_path}")


if __name__ == "__main__":
    main()
