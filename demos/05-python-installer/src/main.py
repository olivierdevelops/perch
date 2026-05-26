#!/usr/bin/env python3
"""
stt_bin — a tiny "fake speech-to-text" demo Python project.

This stand-in shows the pattern: a Python program with its own CLI.
In a real project this would import torch / openai-whisper / whatever.
"""

import argparse
import sys


def transcribe(audio_path: str) -> str:
    # Pretend we ran a 3GB speech model.
    return f"[FAKE TRANSCRIPT of {audio_path}]"


def main(argv=None):
    p = argparse.ArgumentParser(prog="stt", description="Fake speech-to-text demo")
    p.add_argument("audio", nargs="?", default="example.wav",
                   help="Path to an audio file (default: example.wav)")
    p.add_argument("-l", "--lang", default="en",
                   help="Language code (default: en)")
    args = p.parse_args(argv)

    print(f"Transcribing {args.audio} (lang={args.lang})…", file=sys.stderr)
    print(transcribe(args.audio))


if __name__ == "__main__":
    main()
