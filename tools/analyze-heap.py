"""Compare the heap snapshots of a tools/wpr-heap.ps1 trace: what grew between the first and the last.

    python tools/analyze-heap.py C:\\traces\\twinfm-<tag>.etl --exe <the traced twinfm.exe> [--top N]

xperf (on PATH, or at the Windows Performance Toolkit's default install location) dumps the
snapshots with symbolised stacks (`-a heapsnapshot -data`) into <trace dir>\\export\\<name>-heap.txt,
reused while it is newer than the trace. The traced exe's directory goes on _NT_SYMBOL_PATH, because
once that variable is set xperf does not search the pdb path embedded in the exe.

Each snapshot lists every live allocation's stack with its count and bytes. The report groups them
two ways and ranks by the bytes grown from the first snapshot to the last:
  - by site: the deepest frame in twinfm.exe that is not the allocator or a std/core/alloc
    container (Slint, femtovg and the other crates count, being linked into twinfm.exe), else the
    deepest frame outside ntdll's heap;
  - by own code: the deepest `twinfm::` frame, where TwinFM's own code asked;
and prints the stacks that grew most, allocation end first. When the trace also recorded WPR's
VirtualAllocation profile, it adds xperf's VirtualAlloc summary for TwinFM between the two snapshots
(committed and still outstanding, by image and by stack): the memory outside the heap. The report
goes to the screen and to <trace dir>\\export\\<name>-heap-summary.txt.
"""
import os, re, shutil, subprocess, sys
from collections import defaultdict

WPT_DEFAULT = r"C:\Program Files (x86)\Windows Kits\10\Windows Performance Toolkit\xperf.exe"
SYMBOL_SERVER = "srv*C:\\Symbols*https://msdl.microsoft.com/download/symbols"
GENERIC = ("alloc::", "core::", "std::", "hashbrown::", "smallvec::", "<alloc::", "<core::", "<std::",
           "<hashbrown::", "<smallvec::", "__rust", "__rdl", "__rg", "rust_")
HEAP_FRAMES = ("RtlAllocateHeap", "RtlpAllocateNTHeapInternal", "RtlpAllocateHeapInternal",
               "RtlReAllocateHeap", "RtlpReAllocateHeapInternal", "HeapAlloc", "HeapReAlloc")

def xperf():
    found = shutil.which("xperf") or (WPT_DEFAULT if os.path.exists(WPT_DEFAULT) else None)
    if not found:
        sys.exit("xperf not found: install the Windows Performance Toolkit (Windows SDK) or put it on PATH")
    return found

def sym_env(exe):
    env = dict(os.environ)
    env.setdefault("_NT_SYMBOL_PATH", SYMBOL_SERVER)
    if exe:
        env["_NT_SYMBOL_PATH"] += ";" + os.path.dirname(os.path.abspath(exe))
    return env

def dump(etl, exe):
    name = os.path.splitext(os.path.basename(etl))[0]
    out_dir = os.path.join(os.path.dirname(os.path.abspath(etl)), "export")
    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, name + "-heap.txt")
    if os.path.exists(out) and os.path.getmtime(out) > os.path.getmtime(etl):
        return out, out_dir, name
    if not exe:
        print("  (no --exe: twinfm frames resolve only if the build's release directory is on _NT_SYMBOL_PATH)")
    print(f"dumping the snapshots of {etl} (about a minute per 100 MB of trace) ...")
    with open(out, "w", encoding="utf-8", errors="replace") as f:
        subprocess.run([xperf(), "-i", etl, "-quiet", "-symbols", "-a", "heapsnapshot", "-data"],
                       stdout=f, stderr=subprocess.DEVNULL, env=sym_env(exe))
    return out, out_dir, name

def instances(etl):
    """[(timestamp in microseconds, pid)] of the trace's heap snapshots, in order."""
    r = subprocess.run([xperf(), "-i", etl, "-quiet", "-a", "heapsnapshot", "-instance"],
                       capture_output=True, text=True, errors="replace")
    rows = re.findall(r"^\s*(\d+),\s*(\d+),\s*\d+,\s*\d+,", r.stdout, flags=re.M)
    return sorted((int(t), int(p)) for t, p in rows)

def virtual(etl, exe, pid, t1, t2, by, top):
    """xperf's VirtualAlloc summary for `pid` between the two snapshots, outstanding commit first."""
    r = subprocess.run([xperf(), "-i", etl, "-quiet", "-symbols", "-a", "virtualalloc", "-pid", str(pid),
                        "-range", str(t1), str(t2), by, "soc", "-top", str(top)],
                       capture_output=True, text=True, errors="replace", env=sym_env(exe))
    return [l.rstrip() for l in r.stdout.splitlines() if l.strip()]

def parse(path):
    """{instance: {stack (frames, root first): [count, bytes]}}"""
    snaps = defaultdict(dict)
    inst, frames, count = None, None, 0
    head = re.compile(r"^PID \[\d+\] Instance \[\s*(\d+)\]")
    ref = re.compile(r"^\s+StackReference : \S+, identical references = (\d+),")
    total = re.compile(r"^\s+Total Allocation of this stack: (\d+)")
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("StackID "):
                break  # the per-stack count tables at the end
            m = head.match(line)
            if m:
                inst = int(m.group(1))
                continue
            m = ref.match(line)
            if m:
                frames, count = [], int(m.group(1))
                continue
            if frames is not None and line.startswith("\t\t"):
                frames.append(line.strip())
                continue
            m = total.match(line)
            if m and frames is not None and inst is not None:
                key = tuple(frames)
                cell = snaps[inst].setdefault(key, [0, 0])
                cell[0] += count
                cell[1] += int(m.group(1))
                frames = None
    return snaps

def split(frame):
    module, _, func = frame.partition(" ")
    return module, func

def site(stack):
    for frame in reversed(stack):
        module, func = split(frame)
        if module == "twinfm.exe" and not func.startswith(GENERIC):
            return frame
    for frame in reversed(stack):
        module, func = split(frame)
        if not (module == "ntdll.dll" and func in HEAP_FRAMES):
            return frame
    return "(no frames)"

def own(stack):
    for frame in reversed(stack):
        module, func = split(frame)
        if module == "twinfm.exe" and func.startswith("twinfm::"):
            return frame
    return "(no twinfm:: frame)"

def mb(b):
    return f"{b / 1048576:9.2f}"

def grouped(snap, key):
    g = defaultdict(lambda: [0, 0])
    for stack, (c, b) in snap.items():
        cell = g[key(stack)]
        cell[0] += c
        cell[1] += b
    return g

def ranking(first, last, key, top):
    a, z = grouped(first, key), grouped(last, key)
    rows = []
    for k in set(a) | set(z):
        (c0, b0), (c1, b1) = a.get(k, (0, 0)), z.get(k, (0, 0))
        rows.append((b1 - b0, c1 - c0, b0, b1, k))
    rows.sort(key=lambda r: r[0], reverse=True)
    out = [f"{'grown MB':>9} {'grown n':>8} {'first MB':>9} {'last MB':>9}  frame"]
    for d, dc, b0, b1, k in rows[:top]:
        out.append(f"{mb(d)} {dc:8d} {mb(b0)} {mb(b1)}  {k}")
    return out

def main():
    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        sys.exit(__doc__)
    etl = args[0]
    exe = args[args.index("--exe") + 1] if "--exe" in args else None
    top = int(args[args.index("--top") + 1]) if "--top" in args else 25
    path, out_dir, name = dump(etl, exe)
    snaps = parse(path)
    if len(snaps) < 2:
        sys.exit(f"{path}: {len(snaps)} snapshot(s); two are needed")
    order = sorted(snaps)
    first, last = snaps[order[0]], snaps[order[-1]]
    lines = [f"heap snapshots of {etl}: {len(order)} ({', '.join(map(str, order))}); comparing {order[0]} and {order[-1]}"]
    for i in order:
        n = sum(c for c, _ in snaps[i].values())
        b = sum(b for _, b in snaps[i].values())
        lines.append(f"  snapshot {i}: {n} allocations, {b / 1048576:.2f} MB in {len(snaps[i])} stacks")
    lines += ["", "By site (the deepest frame in twinfm.exe outside the allocator and std's containers):"]
    lines += ranking(first, last, site, top)
    lines += ["", "By own code (the deepest twinfm:: frame):"]
    lines += ranking(first, last, own, top)
    lines += ["", "The stacks that grew most (allocation end first, 14 frames):"]
    rows = []
    for stack in set(first) | set(last):
        (c0, b0), (c1, b1) = first.get(stack, (0, 0)), last.get(stack, (0, 0))
        rows.append((b1 - b0, c1 - c0, stack))
    rows.sort(key=lambda r: r[0], reverse=True)
    for d, dc, stack in rows[:10]:
        lines.append(f"  +{d / 1048576:.2f} MB, {dc:+d} allocations:")
        shown = [f for f in reversed(stack) if not (split(f)[0] == "ntdll.dll" and split(f)[1] in HEAP_FRAMES)]
        lines += ["      " + f for f in shown[:14]]
    marks = instances(etl)
    if len(marks) >= 2:
        (t1, pid), (t2, _) = marks[0], marks[-1]
        lines += ["", f"Memory committed outside the heap (VirtualAlloc) by pid {pid} between the snapshots "
                      f"({t1 / 1e6:.0f} s to {t2 / 1e6:.0f} s), still committed at the end, by image:"]
        lines += virtual(etl, exe, pid, t1, t2, "-images", top) or ["  (no VirtualAlloc events: the trace has no VirtualAllocation profile)"]
        lines += ["", "The same, by the stacks that committed most:"]
        lines += virtual(etl, exe, pid, t1, t2, "-stacks", 10) or ["  (none)"]
    report = os.path.join(out_dir, name + "-heap-summary.txt")
    with open(report, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    sys.stdout.buffer.write(("\n".join(lines) + f"\n\nreport: {report}\n").encode("utf-8"))

if __name__ == "__main__":
    main()
