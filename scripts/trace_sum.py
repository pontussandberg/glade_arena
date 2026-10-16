"""Sums a Chrome trace (`trace-*.json`, from a client built with `--features profile`) per span:
milliseconds per frame and calls per frame, over a window of the run, most costly first.

    python scripts/trace_sum.py trace-123.json 20 40            # seconds 20 to 40 of the run
    python scripts/trace_sum.py trace-123.json 20 40 prepare    # only spans whose name has "prepare"

The traces are huge (gigabytes), so it streams them line by line with a regex over the begin/end
events instead of parsing the JSON, and stops reading past the window's end. Frames are counted
by the main schedule's span ("name=Main"). See docs/PERF.md.
"""
import re, sys, collections
path, start_s, end_s = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
pat = re.compile(r'"name":"((?:[^"\\]|\\.)*)","ph":"([BE])","pid":\d+,"tid":(\d+),"ts":([\d.]+)')
stacks = collections.defaultdict(list)
total = collections.Counter()
count = collections.Counter()
lo, hi = start_s * 1e6, end_s * 1e6
with open(path, 'r', encoding='utf-8', errors='replace') as f:
    for line in f:
        m = pat.search(line)
        if not m:
            continue
        name, ph, tid, ts = m.group(1), m.group(2), m.group(3), float(m.group(4))
        if ts > hi:
            break
        st = stacks[tid]
        if ph == 'B':
            st.append((name, ts))
        elif st:
            n, t0 = st.pop()
            if t0 >= lo:
                total[n] += ts - t0
                count[n] += 1
main = [k for k in total if 'name=Main' in k]
nframes = max([count[k] for k in main] or [1])
print('frames in window', nframes, 'main spans', main[:3])
for name, us in [kv for kv in total.most_common() if (len(sys.argv) < 5 or sys.argv[4] in kv[0])][:70]:
    print(f'{us / nframes / 1000:8.3f} ms/frame  {count[name] / nframes:6.1f}x  {name[:160]}')
