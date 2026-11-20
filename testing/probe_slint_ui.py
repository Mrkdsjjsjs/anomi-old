#!/usr/bin/env python3
"""Exercise Yapfast on the private compositor and capture states for visual review."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
INPUT = runpy.run_path(str(ROOT / 'scripts/framework-input'))
INPUT['require_private_display']()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--renderer', choices=['gpu', 'software'], default='gpu')
parser.add_argument('--size', choices=['1120x760', '800x560'], default='1120x760')
parser.add_argument('--exe', type=Path, default=ROOT / 'target/fast/yapfast')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
width, height = map(int, args.size.split('x'))
environment = dict(os.environ, YAPFAST_UI_TEST_SIZE=args.size)
session_directory = args.output / 'unused-preview-session'
command = [str(args.exe.resolve()), '--preview', '--renderer', args.renderer,
           '--data-dir', str(session_directory)]

def capture(name):
    time.sleep(.25)
    with tempfile.TemporaryDirectory(prefix='yapfast-slint-shot-') as folder:
        subprocess.run(['weston-screenshooter'],
                       env=dict(environment, XDG_PICTURES_DIR=folder),
                       check=True, timeout=10, capture_output=True)
        pictures = list(Path(folder).glob('*.png'))
        if len(pictures) != 1:
            raise RuntimeError('Private compositor did not produce a screenshot')
        path = args.output / f'{name}.png'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y',
                        '-i', str(pictures[0]), '-vf', 'vflip', str(path)],
                       check=True, timeout=10)
        return path


def locate_rail(picture):
    # Locate the rail in our private screenshot, rather than assume where
    # Weston placed the window. Exact neutral pixels form its long left edge.
    pixels = subprocess.check_output(['ffmpeg', '-hide_banner', '-loglevel', 'error',
                                     '-i', str(picture), '-f', 'rawvideo',
                                     '-pix_fmt', 'rgb24', '-'], timeout=10)
    output_width, output_height = 1600, 1000
    if len(pixels) != output_width * output_height * 3:
        raise RuntimeError('Unexpected private output dimensions')
    target = bytes([14, 14, 14])
    columns = [sum(pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target
                   for y in range(output_height)) for x in range(output_width)]
    best = max(columns)
    if best < height - 80:
        raise RuntimeError('Yapfast rail was not found; inspect startup.png and app.log')
    x = next(x for x, count in enumerate(columns) if count >= best - 2)
    ys = [y for y in range(output_height)
          if pixels[(y * output_width + x) * 3:(y * output_width + x) * 3 + 3] == target]
    return x, min(ys)


with (args.output / 'app.log').open('wb') as log:
    process = subprocess.Popen(command, env=environment, stdout=log, stderr=log,
                               start_new_session=True)
    try:
        time.sleep(2)
        if process.poll() is not None:
            raise RuntimeError('Yapfast exited before the first capture; inspect app.log')
        origin_x, origin_y = locate_rail(capture('startup'))

        def click(x, y):
            INPUT['click'](origin_x + x, origin_y + y)
            time.sleep(.15)

        def type_text(text):
            keys = dict(zip('abcdefghijklmnopqrstuvwxyz',
                            [30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50,
                             49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44]))
            keys.update(dict(zip('1234567890', range(2, 12))))
            keys.update({' ': 57, '.': 52, '/': 53, '-': 12})
            for character in text:
                if character == ':':
                    INPUT['send']('key 42 1')
                    INPUT['key'](39)
                    INPUT['send']('key 42 0')
                else:
                    INPUT['key'](keys[character])

        click(390, height - 65)
        type_text('slint private test')
        capture('typed')
        INPUT['key'](28)
        capture('sent')
        click(150, 197)
        type_text('remember this draft')
        capture('channel-draft')
        click(32, 36)
        capture('direct-messages')
        click(32, 101)
        capture('restored-server-draft')
        click(150, 303)
        capture('voice-empty')
        click(395, 208 if width == 1120 else 226)
        capture('voice-alone')
        click(150, 159)
        capture('voice-while-chatting')
        click(134, height - 137)
        capture('deafened')
        click(174, height - 137)
        capture('audio-devices')
        INPUT['key'](1)  # Escape dismisses the device popover.
        click(150, height - 29)
        capture('sign-in')
        (args.output / 'actions.json').write_text(json.dumps({
            'renderer': args.renderer, 'size': args.size,
            'origin': [origin_x, origin_y],
            'binary_sha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
            'isolation': 'Private Weston, virtual input and private D-Bus only. No microphone or speakers opened.',
            'validation': 'Coordinate actions and screenshots require visual inspection; captures alone do not assert success.',
        }, indent=2) + '\n')
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
