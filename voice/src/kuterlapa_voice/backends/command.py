"""Any program: the text on its stdin, a 16-bit WAV file on its stdout.

This keeps a machine's own speech service out of this repository: a small
private script calls the service, and Kuterlapa only runs the script.
"""

import subprocess

from .wav import decode


class Backend:
    def __init__(self, command):
        if not command:
            raise SystemExit("voice backend 'command' needs a command in voice.toml")
        self.command = command
        self.ref = {"{ref_audio}": "", "{ref_text}": ""}

    def set_voice(self, ref_audio, ref_text):
        self.ref = {"{ref_audio}": ref_audio, "{ref_text}": ref_text}

    def synthesize(self, text):
        command = [self._fill(arg) for arg in self.command]
        done = subprocess.run(command, input=text.encode("utf-8"), capture_output=True)
        if done.returncode != 0:
            raise RuntimeError(f"{self.command[0]} failed: {done.stderr.decode(errors='replace').strip()}")
        return decode(done.stdout)

    def _fill(self, arg):
        for key, value in self.ref.items():
            arg = arg.replace(key, value)
        return arg


def load(settings):
    return Backend(settings.command)
