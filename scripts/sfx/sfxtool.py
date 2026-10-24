"""Tiny PCM WAV tool (standard library only).

  info <in.wav>                         duration, channels, rate, bits, peak, and RMS per 50 ms
  cut  <in.wav> <out.wav> <start> <end> [--half] [--fade MS]
       trims start..end seconds, mixes to mono 16-bit, optionally halves the sample rate
       (averaging pairs), optionally fades the last MS milliseconds out and the first 5 ms in
"""
import array, math, struct, sys, wave


def read(path):
    with wave.open(path, "rb") as w:
        ch, sw, rate, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    if sw == 2:
        data = array.array("h", raw)
        scale = 32768.0
    elif sw == 3:
        data = array.array("i", (int.from_bytes(raw[i:i + 3], "little", signed=True) for i in range(0, len(raw), 3)))
        scale = 8388608.0
    elif sw == 4:
        data = array.array("i", raw)
        scale = 2147483648.0
    elif sw == 1:
        data = array.array("b", (b - 128 for b in raw))
        scale = 128.0
    else:
        raise SystemExit(f"unsupported sample width {sw}")
    # Mono, as floats in -1..1.
    mono = [sum(data[i:i + ch]) / ch / scale for i in range(0, len(data), ch)]
    return mono, rate


def info(path):
    mono, rate = read(path)
    n = len(mono)
    peak = max(abs(x) for x in mono) if n else 0.0
    print(f"{path}: {n / rate:.2f}s rate={rate} peak={peak:.2f}")
    step = rate // 20
    line = []
    for i in range(0, n, step):
        chunk = mono[i:i + step]
        rms = math.sqrt(sum(x * x for x in chunk) / max(1, len(chunk)))
        line.append(f"{i / rate:4.2f}:{rms:.2f}")
    print("  " + " ".join(line))


def cut(src, dst, start, end, half=False, fade_ms=0):
    mono, rate = read(src)
    a, b = int(start * rate), min(len(mono), int(end * rate))
    seg = mono[a:b]
    if half:
        seg = [(seg[i] + seg[i + 1]) / 2 for i in range(0, len(seg) - 1, 2)]
        rate //= 2
    fade_in = min(len(seg), int(0.005 * rate))
    for i in range(fade_in):
        seg[i] *= i / fade_in
    fade = min(len(seg), int(fade_ms / 1000 * rate))
    for i in range(fade):
        seg[len(seg) - 1 - i] *= i / fade
    peak = max(abs(x) for x in seg) or 1.0
    # Normalize to -1 dBFS so every clip meets the mixer at the same level.
    gain = 0.89 / peak
    out = array.array("h", (max(-32768, min(32767, int(x * gain * 32767))) for x in seg))
    with wave.open(dst, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(out.tobytes())
    print(f"{dst}: {len(seg) / rate:.2f}s rate={rate} ({len(out) * 2 // 1024} KB)")


if __name__ == "__main__":
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "info":
        for p in args:
            info(p)
    elif cmd == "cut":
        half = "--half" in args
        fade = 0
        if "--fade" in args:
            fade = int(args[args.index("--fade") + 1])
        pos = [x for x in args if not x.startswith("--") and x != (str(fade) if fade else None)]
        cut(pos[0], pos[1], float(pos[2]), float(pos[3]), half, fade)
