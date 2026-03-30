#!/usr/bin/env python3
"""Transcribe audio via AssemblyAI with speaker diarization.

Outputs markdown in the same format as Minutes app so it flows through
the Obsidian vault pipeline (01-Inbox → process-meeting → structured notes).

Usage:
    python3 scripts/transcribe-assemblyai.py <audio_file> [--title "Meeting Name"]

Requires ASSEMBLYAI_API_KEY in .env or environment.
"""

import argparse
import os
import re
import shutil
import sys
import time
from datetime import datetime
from pathlib import Path

# Load .env from repo root
env_path = Path(__file__).resolve().parent.parent / ".env"
if env_path.exists():
    for line in env_path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            os.environ.setdefault(key.strip(), value.strip())

import assemblyai as aai

API_KEY = os.environ.get("ASSEMBLYAI_API_KEY")
if not API_KEY:
    print("Error: ASSEMBLYAI_API_KEY not set. Add it to .env or export it.", file=sys.stderr)
    sys.exit(1)

aai.settings.api_key = API_KEY

# Config — matches Minutes app paths
MEETINGS_DIR = Path.home() / "meetings"
VAULT_PATH = Path.home() / "Documents" / "Obsidian" / "a-life"
VAULT_INBOX = VAULT_PATH / "01-Inbox"
IDENTITY_NAME = "James Gaynor"


def transcribe(audio_path: str, title: str | None = None) -> Path:
    """Upload, transcribe, and save in Minutes-compatible format."""
    audio = Path(audio_path)
    if not audio.exists():
        print(f"Error: file not found: {audio}", file=sys.stderr)
        sys.exit(1)

    print(f"Uploading {audio.name} ({audio.stat().st_size / 1024 / 1024:.1f} MB)...")

    config = aai.TranscriptionConfig(
        speech_models=[
            "universal-3-pro",
            "universal-2",
        ],
        language_code="en_us",
        speaker_labels=True,
        punctuate=True,
        format_text=True,
        disfluencies=False,
    )

    transcriber = aai.Transcriber(config=config)

    start = time.time()
    print("Transcribing (this may take a few minutes)...")
    transcript = transcriber.transcribe(str(audio))
    elapsed = time.time() - start

    if transcript.status == aai.TranscriptStatus.error:
        print(f"Error: transcription failed: {transcript.error}", file=sys.stderr)
        sys.exit(1)

    duration_secs = transcript.audio_duration or 0
    num_speakers = count_speakers(transcript.utterances)
    print(f"Done in {elapsed:.0f}s — {format_duration(duration_secs)}, {num_speakers} speakers, {transcript.confidence:.1%} confidence")

    # Generate title from first utterance if not provided
    if not title:
        title = generate_title(transcript)

    # Build Minutes-compatible markdown
    now = datetime.now().astimezone()
    duration_str = format_duration(duration_secs)

    # Build speaker map from utterances
    speakers = {}
    if transcript.utterances:
        for utt in transcript.utterances:
            if utt.speaker not in speakers:
                speakers[utt.speaker] = f"Speaker {utt.speaker}"

    # YAML frontmatter
    lines = []
    lines.append("---")
    lines.append(f"title: {title}")
    lines.append("type: meeting")
    lines.append(f"date: {now.isoformat()}")
    lines.append(f"duration: {duration_str}")
    lines.append("status: transcript-only")
    lines.append(f"recorded_by: {IDENTITY_NAME}")
    lines.append("transcription_engine: assemblyai-universal-3-pro")
    if speakers:
        lines.append("speaker_map:")
        for label, name in speakers.items():
            lines.append(f"- speaker_label: {label}")
            lines.append(f"  name: {name}")
            lines.append("  confidence: high")
            lines.append("  source: assemblyai")
    lines.append("---")
    lines.append("")
    lines.append("## Transcript")
    lines.append("")

    # Transcript body — [Speaker X:XX] format matching Minutes
    if transcript.utterances:
        for utt in transcript.utterances:
            ts = format_timestamp(utt.start)
            speaker = f"Speaker {utt.speaker}"
            lines.append(f"[{speaker} {ts}] {utt.text}")
    else:
        lines.append(transcript.text or "(empty transcript)")

    lines.append("")
    md = "\n".join(lines)

    # Save to ~/meetings/ with Minutes naming convention
    slug = slugify(title)
    date_prefix = now.strftime("%Y-%m-%d")
    recorder_suffix = slugify(IDENTITY_NAME.split()[0].lower())
    filename = f"{date_prefix}-{slug}-{recorder_suffix}.md"

    MEETINGS_DIR.mkdir(parents=True, exist_ok=True)
    output_path = MEETINGS_DIR / filename

    # Handle collision
    counter = 2
    while output_path.exists():
        output_path = MEETINGS_DIR / f"{date_prefix}-{slug}-{recorder_suffix}-{counter}.md"
        counter += 1

    output_path.write_text(md)
    # Match Minutes file permissions
    output_path.chmod(0o600)
    print(f"Saved: {output_path}")

    # Vault sync (copy to Obsidian inbox)
    if VAULT_INBOX.exists():
        vault_dest = VAULT_INBOX / output_path.name
        shutil.copy2(output_path, vault_dest)
        print(f"Vault: {vault_dest}")
    else:
        print(f"Vault inbox not found at {VAULT_INBOX}, skipping sync")

    return output_path


def generate_title(transcript) -> str:
    """Generate title from first utterance text."""
    if transcript.utterances and transcript.utterances[0].text:
        first = transcript.utterances[0].text
        # Take first ~8 words, title case
        words = first.split()[:8]
        candidate = " ".join(words)
        # Remove trailing punctuation
        candidate = candidate.rstrip(".,!?;:")
        return candidate.title() if candidate else "Untitled Meeting"
    return "Untitled Meeting"


def slugify(text: str, max_len: int = 60) -> str:
    """Convert text to URL-friendly slug."""
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    text = text.strip("-")
    return text[:max_len]


def format_duration(seconds: int) -> str:
    if not seconds:
        return "0s"
    m, s = divmod(seconds, 60)
    h, m = divmod(m, 60)
    if h:
        return f"{h}h{m}m"
    if m:
        return f"{m}m{s}s"
    return f"{s}s"


def format_timestamp(ms: int | None) -> str:
    if not ms:
        return "0:00"
    total_secs = ms // 1000
    m, s = divmod(total_secs, 60)
    h, m_rem = divmod(m, 60)
    if h:
        return f"{h}:{m_rem:02d}:{s:02d}"
    return f"{m}:{s:02d}"


def count_speakers(utterances) -> int:
    if not utterances:
        return 0
    return len(set(u.speaker for u in utterances))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Transcribe audio via AssemblyAI")
    parser.add_argument("audio", help="Path to audio file")
    parser.add_argument("--title", "-t", help="Meeting title (default: auto-generated from transcript)")
    args = parser.parse_args()

    result_path = transcribe(args.audio, args.title)
    print(f"\nTranscript ready at: {result_path}")
