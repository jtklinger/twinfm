# Cleanup list

Things left on the desktop by TwinFM gate runs and by the 2026-10-08 Files-app profiling session
that should not be forgotten. Sizes measured 2026-10-15. Tick items off (or delete the line) when
done; add new ones at the bottom with a date.

## Stale reference builds in this checkout (done 2026-10-15)

`CLAUDE.md` § Measuring: the reference binary is the last merged milestone's gate build and "the
previous one is deleted once the new one has served as a reference". Four previous ones are still here:

| Directory | Size | Status |
|---|---|---|
| `target-p3m4` | 1.65 GB | deleted 2026-10-15 |
| `target-p3m5` | 1.65 GB | deleted 2026-10-15 |
| `target-p3m5a` | 1.65 GB | deleted 2026-10-15 |
| `target-p3m5b` | 1.64 GB | deleted 2026-10-15 |
| `target-p4m1` | 1.65 GB | still here 2026-10-16: superseded as the reference by `target-p4m3` on 2026-10-15, so it is due for deletion on Jeremy's word |
| `target-p4m2` | 1.65 GB | deleted 2026-10-15 |
| `target-p4m3` | 1.5 GB | deleted 2026-10-16 (served as the reference for P5-M1 and P5-M2, ab20) |
| `target-p5m2` | 1.5 GB | **keep** - current reference (6b1d0f1), for P5-M3's triplet (ab21) |
| `target` | 3.50 GB | ordinary dev builds; `cargo clean` any time |

The four superseded ones were deleted on 2026-10-15 (6.6 GB freed). For the next round, from the checkout root:

```powershell
Remove-Item -Recurse -Force .\target-p3m4, .\target-p3m5, .\target-p3m5a, .\target-p3m5b
```

## Gate traces in `C:\traces` (169 GB)

`tools/wpr-triplet.ps1` leaves every run's `.etl` plus a `.NGENPDB` / `.EmbeddedPdbs` folder next
to it. As of 2026-10-15: 117 `twinfm-*.etl` files (156.5 GB, 2026-10-10 to 2026-10-15) and 189
side folders (12.7 GB). The gate records live in `docs/phase-*-plan.md`, so a trace is only worth
keeping while its milestone's gate is still open or a re-analysis is planned.

Suggested rule: keep the current milestone's six triplet traces and the current reference's; delete
the rest after each gate closes. For everything older than the current milestone tag (adjust the
tag), list first, then pipe the same listing to the delete cmdlet:

```powershell
Get-ChildItem C:\traces -Filter 'twinfm-*' | Where-Object { $_.Name -notmatch 'p4m2' }
```

Also in `C:\traces`: `soak.err` (459 KB) and `soak.log` from a soak run, `p3m4-drop.stderr.txt`.
Small; delete with the traces.

## Files-app profiling leftovers (2026-10-08)

Write-up: `C:\traces\files-perf-summary.md` (keep; it is the evidence behind the TwinFM decision -
consider copying it into `docs/` if the folder is ever cleared).

| Item | Size | Action |
|---|---|---|
| `C:\traces\files-dualpane.etl` + `files-dualpane.etl.NGENPDB` | 6.3 GB | delete once the summary is enough |
| `C:\traces\export\` (butterfly HTML, CSVs) | 0.28 GB | delete with the trace, or keep `bf-ui-prof-*.txt` (5 MB) |
| `C:\dumps\Files.exe.239824.dmp` | 1.9 GB | delete once an upstream report is filed or abandoned (`crash-analyze.txt` has the stacks) |
| `C:\traces\*.wpaProfile` (9 files), `repro*-timeline.txt`, `record-trace.log`, `setup-elevated.log`, `crash-analyze.txt` | < 100 KB | tidy; `crash-analyze.txt` is worth keeping |
| `HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\Files.exe` | - | remove when Files is retired; until then it writes up to five 2 GB dumps into `C:\dumps` |
| `C:\Symbols` (symbol store + `symcache`) | 12.6 GB | **keep** while gate runs use `-symbols`; prune `C:\Symbols\symcache` if space matters |
| Windows Performance Toolkit (ADK), Store WinDbg | - | **keep** while gating |
| `C:\traces\bigdir` (40,000 files), `C:\traces\bigdir.zip`, `grid.toml`, `grid-thumbs.toml`, `thumbs.toml` | 4 MB | **keep** - fixtures named in `CLAUDE.md` |

The dump key is removed with the registry cmdlet on
`HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\Files.exe` (elevated, `-Recurse`).

## Files app itself

- Files leaves a window-less `Files.exe` running after its last window closes; one such process had
  been alive for a week with 81 CPU-minutes on 2026-10-08. Check `Get-Process Files` now and then.
- When TwinFM has replaced it for daily use: uninstall the sideloaded package
  (`Get-AppxPackage Files | Remove-AppxPackage`) and remove the dump key above.
