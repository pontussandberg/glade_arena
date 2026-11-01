#!/bin/bash
# audio.sh CLASS: as CLASS in practice with a dummy up-right, tap the audio graph with an
# AnalyserNode, attack the dummy for a few seconds and print the loud spans (ms from the attack
# order, peak) so each sound's layers can be seen firing.
N=/c/Users/PONTUS~1/AppData/Local/Temp/arena-sfx
cls=$1; py=670
case $cls in
  revenant) x=100;; javelinist) x=307;; frost_mage) x=489;; warden) x=671;;
  pyre_witch) x=852; py=710;; shade) x=1035;; breaker) x=1217;; stormweaver) x=1399;;
  *) echo "unknown class $cls"; exit 1;;
esac
TAP='(() => { const orig = AudioNode.prototype.connect; window.__peaks = []; AudioNode.prototype.connect = function(dest, ...a) { if (dest instanceof AudioDestinationNode && !window.__an) { const an = dest.context.createAnalyser(); an.fftSize = 1024; window.__an = an; orig.call(an, dest); const buf = new Float32Array(1024); setInterval(() => { an.getFloatTimeDomainData(buf); let p = 0; for (const x of buf) p = Math.max(p, Math.abs(x)); window.__peaks.push([performance.now(), p]); }, 10); return orig.call(this, an, ...a); } return orig.call(this, dest, ...a); }; })();'
DUMP='(() => { const t0 = window.__t0 ?? 0; const spans = []; let cur = null; for (const [t, p] of window.__peaks) { if (t < t0) continue; if (p > 0.015) { if (cur && t - cur.end < 40) { cur.end = t; cur.max = Math.max(cur.max, p); } else { cur = { start: t, end: t, max: p }; spans.push(cur); } } } return spans.map(s => `${Math.round(s.start - t0)}-${Math.round(s.end - t0)}ms ${s.max.toFixed(2)}`).join(" | ") + ` (tapped=${!!window.__an}, samples=${window.__peaks.length})`; })()'
cd "$N" || exit 1
GREP_LOGS="didn't load|panick|rror" node shoot.mjs "$N/audio_$cls.png" "preload:$TAP" nav:http://localhost:8080 wait:14 click:$x,675 wait:1 click:1535,$py wait:5 \
  scroll:790,360,-500 wait:0.4 scroll:790,360,-500 wait:0.8 click:1475,333 wait:0.3 click:1000,330 wait:1.0 \
  rclick:800,330 wait:0.3 press:s wait:1.5 "eval:window.__t0=performance.now()" click:1030,310 wait:3.5 "eval:$DUMP" 2>&1 | grep -vE "^saved" | cut -c 1-900
