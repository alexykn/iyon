# parrot.live — Streaming ANSI Art via `curl`

This directory contains **590 individual frames** captured from the live streaming endpoint at `parrot.live`.

## What is parrot.live?

`curl parrot.live` is a well-known internet easter egg. When you curl it, it streams an endlessly looping **dancing ASCII parrot** — frame by frame — using ANSI escape sequences to clear the screen and redraw the parrot in a different color each time.

The parrot cycles through colors (green → white → blue → magenta → cyan → yellow → red → …) while doing a little bounce/wave animation.

## How the frames were captured

The bash tool was used to download the stream with a size limit in place (using `--max-time` and `head -c`) to avoid an infinite loop — since `parrot.live` streams forever:

```bash
curl -s --max-time 5 parrot.live | head -c 50000 > raw_output.txt
```

Then the raw output was split into individual frames using the ANSI escape sequence that resets the display:

```
delimiter = \x1b[2J\x1b[3J\x1b[H   (clear screen + clear scrollback + cursor home)
```

## How the bash tool displays live streaming output

The bash tool can execute commands and capture their stdout/stderr in real time. This makes it possible to:

1. **Stream data from the internet** — `curl` connects to a remote server and pipes the data back.
2. **Process the stream** — split it into frames, save them, or replay them.
3. **Replay animation frame-by-frame** — by looping through saved frames with a small `sleep` between them, the tool renders each frame sequentially, creating the illusion of animation right in the terminal output.

For example, replaying the saved frames:

```bash
for i in $(seq 0 589); do
  f=$(printf "frame_%04d.txt" $i)
  cat "$f"
  sleep 0.04
done
```

Because the terminal emulator in the tool respects ANSI escape codes (clear screen, color changes, cursor positioning), each frame overwrites the previous one, producing a smooth animation.

## Files

| File | Description |
|------|-------------|
| `frame_0000.txt` – `frame_0589.txt` | Individual animation frames (each ~1108 bytes) |
| `README.md` | This explanation |