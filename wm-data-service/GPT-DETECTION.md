**Algorithms implemented**

Single-event (stateless) heuristics in suspicious.rs:
- Process start command-line patterns (mainly on `EventData::Process` with `opcode == 1`)
  - PowerShell `-EncodedCommand` / `-enc`
  - PowerShell download/execute patterns (e.g., `Invoke-WebRequest`, `DownloadString`, `IEX`)
  - Hidden window flags (`-WindowStyle Hidden` / `-w hidden`)
  - `cmd.exe /c`, command chaining / redirection signals
  - `mshta.exe` with `http(s)://` in arguments
  - `certutil` and `bitsadmin` “download-ish” patterns
  - Small baseline tag for “LOLBIN/scripting process”
- Registry persistence signals (`EventData::Registry`)
  - Run/RunOnce key access
  - `SYSTEM\\CurrentControlSet\\Services\\` access (rough persistence-ish signal)
- File path signals (`EventData::FileCreate` / `EventData::FileInfo`)
  - AppData/Temp activity
  - Startup folder activity
  - Executable/script extensions in user-writable locations
- Network signals (`EventData::TcpIp` / `EventData::UdpIp`)
  - “Suspicious” destination ports (e.g., 4444/1337/31337/9001, etc.)
  - SMB (445) / RDP (3389) presence (low score on a single event)

Multi-event (stateful, per-host-IP) chain detectors (also in suspicious.rs):
- Process burst: ≥ 80 process-starts within 30s (per host).
- Port scan-ish: ≥ 40 distinct destination ports to the same destination IP within 60s (per host).
- Lateral movement-ish bursts:
  - SMB: ≥ 15 distinct destination IPs on port 445 within 60s (per host)
  - RDP: ≥ 10 distinct destination IPs on port 3389 within 60s (per host)
- Parent→child chain: Office/browser parent process starts, then spawns a LOLBIN/script child (per host; uses tracked process starts by PID).
- Drop→execute chain: write/rename an executable/script into a user-writable location, then execute that same path shortly after (per host).
- RunKey→LOLBIN chain: Run/RunOnce activity, then a LOLBIN/script process start within ~5 minutes (per host).
- Suspicious-spawn→network chain:
  - If a process already scored “suspicious enough” at start, then it makes a network connection shortly after start, boost score (per host, per PID).

Important isolation detail: state is stored under `hosts: HashMap<IpAddr, HostState>`, so events from different host addresses do not mix.

**How to trigger a positive detection (quick self-tests)**  
Do these in a test VM or sacrificial machine; some of these actions can trip endpoint security tools.

1) PowerShell encoded command (high-signal)
- Generate an encoded payload:
  - `powershell -NoProfile -Command "$e=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('Write-Output WMTEST')); Write-Output $e"`
- Then execute it:
  - `powershell -NoProfile -EncodedCommand <PASTE_BASE64_HERE>`

2) PowerShell “download-ish” pattern (should score)
- `powershell -NoProfile -Command "Invoke-WebRequest https://example.com -UseBasicParsing | Out-Null"`

3) mshta remote URL (should score)
- `mshta.exe https://example.com`

4) Process burst (chain detector)
- Spawn a bunch of short-lived processes:
  - `for /l %i in (1,1,90) do start "" /b cmd /c exit`

5) Port scan-ish (chain detector)
- In PowerShell, probe many ports on one IP quickly (even if closed):
  - `powershell -NoProfile -Command "1..50 | % { Test-NetConnection 127.0.0.1 -Port $_ -WarningAction SilentlyContinue | Out-Null }"`

6) Drop→execute (chain detector)
- Copy a known EXE into Temp and run it:
  - `powershell -NoProfile -Command "$p=\"$env:TEMP\\wmtest-notepad.exe\"; Copy-Item $env:WINDIR\\System32\\notepad.exe $p -Force; Start-Process $p"`

7) RunKey→LOLBIN chain (chain detector)
- Add a Run key, then start PowerShell:
  - `reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run" /v WMTest /t REG_SZ /d "%WINDIR%\\System32\\notepad.exe" /f`
  - `powershell -NoProfile -Command "Write-Output WMTEST"`
- Cleanup:
  - `reg delete "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run" /v WMTest /f`

**Where the outputs show up**
- In Elasticsearch documents written by the forwarder (bulk index `events.windows-monitor-ecs` in forwarder.rs).
- The forwarder enriches each ECS event with:
  - Root `tags`: includes `wm_suspicious_*` tags
  - `wm.suspicious.score`
  - `wm.suspicious.tags`
  - `wm.suspicious.reasons`
- In Kibana/Elasticsearch you can filter/query for detections with something like:
  - `wm.suspicious.score > 0`
  - or `tags: "wm_suspicious_*"`

If you tell me which host IP you expect to see (and whether `image_file_name` is full path or basename in your environment), I can recommend the most reliable test trigger for your setup.
