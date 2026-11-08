"""The macOS `say` command. Instant, but robotic."""

import os
import subprocess
import tempfile

from .wav import decode


class Backend:
    def __init__(self, voice):
        self.voice = voice

    def synthesize(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "say.wav")
            args = ["say", "--file-format=WAVE", "--data-format=LEI16@22050", "-o", path]
            if self.voice:
                args += ["-v", self.voice]
            subprocess.run(args + [text], check=True)
            with open(path, "rb") as f:
                return decode(f.read())


def load(settings):
    return Backend(settings.say_voice)
