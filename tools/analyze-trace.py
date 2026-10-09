#!/usr/bin/env python3
"""Main-thread analysis of a gate-run WPR trace, no elevation needed.

    python tools/analyze-trace.py C:\\traces\\rustfm-<name>.etl [--symbols]

Steps (all via xperf from the Windows Performance Toolkit, which must be on PATH):
  1. list the process's threads; the first thread is the main thread, and the right-pane
     enumeration worker (a 20 to 60 ms thread starting 2 s or more after the process) marks
     the split: the F3 mark is ~12 ms before it;
  2. count main-thread sampled-profile hits over the 10 s after the split (1 ms sampling,
     so hits ~= milliseconds busy) and over the whole run;
  3. with --symbols, write a symbolized butterfly report for that window and print the
     accessibility / render / ListView shares.
Outputs land next to the trace under <trace dir>\\export\\<name>\\.
"""
import html
import os
import re
import subprocess
import sys

def run(args):
    r = subprocess.run(args, capture_output=True, text=True, errors="replace")
    return r.stdout + r.stderr

def tables(path):
    s = open(path, encoding="utf-8", errors="replace").read()
    out = []
    for tb in re.findall(r"<table.*?</table>", s, flags=re.S):
        rows = []
        for tr in re.findall(r"<tr.*?</tr>", tb, flags=re.S):
            cells = [html.unescape(re.sub(r"<[^>]+>", "", c)).strip() for c in re.findall(r"<t[dh][^>]*>(.*?)</t[dh]>", tr, flags=re.S)]
            if cells:
                rows.append(cells)
        out.append(rows)
    return out

def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    etl = os.path.abspath(sys.argv[1])
    symbols = "--symbols" in sys.argv
    name = os.path.splitext(os.path.basename(etl))[0]
    outdir = os.path.join(os.path.dirname(etl), "export", name)
    os.makedirs(outdir, exist_ok=True)
    base = ["xperf", "-tle", "-tti", "-i", etl]

    threads_txt = os.path.join(outdir, "threads.txt")
    run(base + ["-o", threads_txt, "-a", "process", "-thread"])
    procs, threads = [], []
    for line in open(threads_txt, encoding="utf-8", errors="replace"):
        if "rustfm-proto.exe" not in line:
            continue
        f = [x.strip() for x in line.split(",")]
        start, end, kind = int(f[0]), int(f[1]), f[2]
        if kind == "Process":
            procs.append((start, end, f[4]))
        elif kind == "Thread":
            threads.append((start, end, int(f[5])))
    if not threads:
        print("no rustfm-proto.exe threads found in", etl)
        return 1
    threads.sort()
    pstart = procs[0][0] if procs else threads[0][0]
    main_tid = threads[0][2]
    workers = [(s, e, t) for s, e, t in threads if 15_000 <= e - s <= 80_000 and s - pstart >= 2_000_000]
    if not workers:
        print("could not find the right-pane enumeration worker; threads:", threads[:12])
        return 1
    enum_start = workers[0][0]
    split = enum_start - 12_000
    lo, hi = split, split + 10_000_000
    print(f"{name}: pid start {pstart/1e6:.3f} s, main tid {main_tid}, split ~{split/1e6:.3f} s, window {lo/1e6:.3f}-{hi/1e6:.3f} s")

    def count(label, extra):
        out = os.path.join(outdir, f"bf-{label}-nosym.html")
        run(base + ["-o", out, "-a", "stack", "-butterfly", "1000000", "-tid", str(main_tid)] + extra + ["-event", "Prof"])
        t = tables(out)
        return int(t[0][1][2]) if t and len(t[0]) > 1 else 0
    win = count("10s", ["-range", str(lo), str(hi)])
    whole = count("all", [])
    print(f"  main thread: {win} sampled ms in the 10 s after the split = {win/100:.1f}% busy; {whole} ms over the whole run")

    if symbols:
        out = os.path.join(outdir, "bf-main-10s.html")
        env = dict(os.environ)
        env.setdefault("_NT_SYMBOL_PATH", "srv*C:\\Symbols*https://msdl.microsoft.com/download/symbols")
        subprocess.run(base + ["-symbols", "-o", out, "-a", "stack", "-butterfly", "20", "-tid", str(main_tid), "-range", str(lo), str(hi), "-event", "Prof"], capture_output=True, env=env)
        t = tables(out)
        funcs = t[3][1:] if len(t) > 3 else []
        keys = {
            "accessibility (with_access_kit_adapter)": "with_access_kit_adapter_from_weak_window_adapter",
            "  full rebuild (build_new_tree)": "build_new_tree",
            "  dirty-node rebuild": "rebuild_tree_of_dirty_nodes",
            "  UIA event raising (QueuedEvents::raise)": "QueuedEvents::raise",
            "render pass (FemtoVGRenderer::render)": "FemtoVGRenderer::render",
            "  text (draw_text)": "draw_text",
            "ListView rows (ensure_updated_listview)": "ensure_updated_listview",
            "timers (activate_expired)": "activate_expired",
            "heap (RtlpLowFragHeapAllocFromContext)": "RtlpLowFragHeapAllocFromContext",
        }
        print(f"  symbolized report: {out}")
        for label, k in keys.items():
            hit = next((f for f in funcs if k in f[0]), None)
            if hit:
                print(f"  {hit[1]:>6} {hit[2]:>7}  {label}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
