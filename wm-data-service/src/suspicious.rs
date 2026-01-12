use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use wm_common::schema::event::{CapturedEventRecord, EventData};

#[derive(Debug, Default, Clone)]
pub struct SuspiciousAnalysis {
    pub score: u32,
    pub tags: Vec<String>,
    pub reasons: Vec<String>,
}

impl SuspiciousAnalysis {
    fn add(&mut self, score: u32, tag: impl Into<String>, reason: impl Into<String>) {
        self.score = self.score.saturating_add(score);
        self.tags.push(tag.into());
        self.reasons.push(reason.into());
    }

    fn merge(&mut self, other: SuspiciousAnalysis) {
        self.score = self.score.saturating_add(other.score);
        self.tags.extend(other.tags);
        self.reasons.extend(other.reasons);
    }

    fn dedupe(&mut self) {
        let mut seen = HashSet::new();
        self.tags.retain(|t| seen.insert(t.clone()));

        let mut seen = HashSet::new();
        self.reasons.retain(|r| seen.insert(r.clone()));
    }
}

#[derive(Debug, Default)]
pub struct SuspicionDetector {
    hosts: HashMap<IpAddr, HostState>,
}

#[derive(Debug)]
struct HostState {
    last_seen: Instant,
    process_starts: VecDeque<Instant>,
    conn_events: VecDeque<ConnEvent>,

    processes: HashMap<u32, ProcessState>,
    file_drops: VecDeque<FileDrop>,
    run_key_events: VecDeque<RunKeyEvent>,

    lateral_445: VecDeque<LateralConn>,
    lateral_3389: VecDeque<LateralConn>,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            last_seen: Instant::now(),
            process_starts: VecDeque::new(),
            conn_events: VecDeque::new(),
            processes: HashMap::new(),
            file_drops: VecDeque::new(),
            run_key_events: VecDeque::new(),
            lateral_445: VecDeque::new(),
            lateral_3389: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ConnEvent {
    ts: Instant,
    dst_ip: IpAddr,
    dst_port: u16,
}

#[derive(Debug, Clone)]
struct ProcessState {
    started: Instant,
    image_lower: String,
    command_lower: String,
    base_score: u32,
}

#[derive(Debug, Clone)]
struct FileDrop {
    ts: Instant,
    path_lower: String,
}

#[derive(Debug, Clone)]
struct RunKeyEvent {
    ts: Instant,
}

#[derive(Debug, Clone, Copy)]
struct LateralConn {
    ts: Instant,
    dst_ip: IpAddr,
}

impl SuspicionDetector {
    pub fn observe(&mut self, host_ip: IpAddr, record: &CapturedEventRecord) -> SuspiciousAnalysis {
        let now = Instant::now();
        let host = self.hosts.entry(host_ip).or_default();
        host.last_seen = now;

        let mut analysis = analyze_record(record);

        // Per-host pruning to prevent PID reuse mixing across long periods.
        prune_older(&mut host.process_starts, now, Duration::from_secs(30));
        prune_older_conn(&mut host.conn_events, now, Duration::from_secs(60));
        prune_processes(&mut host.processes, now, Duration::from_secs(10 * 60));
        prune_older_filedrops(&mut host.file_drops, now, Duration::from_secs(5 * 60));
        prune_older_runkeys(&mut host.run_key_events, now, Duration::from_secs(10 * 60));
        prune_older_lateral(&mut host.lateral_445, now, Duration::from_secs(60));
        prune_older_lateral(&mut host.lateral_3389, now, Duration::from_secs(60));

        match &record.event.data {
            EventData::Process { .. } if record.event.opcode == 1 => {
                host.process_starts.push_back(now);

                // Very simple burst detector; useful for malware that spawns tons of short-lived processes.
                let process_burst_threshold = 80usize;
                if host.process_starts.len() >= process_burst_threshold {
                    analysis.add(
                        25,
                        "wm_suspicious_process_burst",
                        format!(
                            "process-start burst: {} in last 30s",
                            host.process_starts.len()
                        ),
                    );
                }

                // Chain detectors that require process start context.
                if let EventData::Process {
                    process_id,
                    parent_id,
                    image_file_name,
                    command_line,
                    ..
                } = &record.event.data
                {
                    let image_lower = image_file_name.to_ascii_lowercase();
                    let command_lower = command_line.to_ascii_lowercase();

                    // Save a per-PID state snapshot so future network/file/registry events can correlate.
                    host.processes.insert(
                        *process_id,
                        ProcessState {
                            started: now,
                            image_lower: image_lower.clone(),
                            command_lower: command_lower.clone(),
                            base_score: analysis.score,
                        },
                    );

                    // Office/browser -> LOLBin chain (parent/child relationship).
                    if let Some(parent) = host.processes.get(parent_id) {
                        if is_office_or_browser_parent(&parent.image_lower)
                            && is_lolbin_or_script_child(&image_lower, &command_lower)
                        {
                            analysis.add(
                                55,
                                "wm_suspicious_parent_child_lolbin",
                                format!(
                                    "{parent} spawned LOLBIN/script child {child}",
                                    parent = parent.image_lower,
                                    child = image_lower
                                ),
                            );
                        }
                    }

                    // Recently dropped executable/script -> executed chain.
                    if let Some(drop) = host
                        .file_drops
                        .iter()
                        .rev()
                        .find(|d| now.duration_since(d.ts) <= Duration::from_secs(2 * 60))
                    {
                        if image_lower == drop.path_lower {
                            analysis.add(
                                70,
                                "wm_suspicious_drop_and_execute",
                                "recently dropped executable/script was executed",
                            );
                        }
                    }

                    // Run key write -> suspicious process start chain.
                    if is_lolbin_or_script_child(&image_lower, &command_lower) {
                        if let Some(_) = host
                            .run_key_events
                            .iter()
                            .rev()
                            .find(|e| now.duration_since(e.ts) <= Duration::from_secs(5 * 60))
                        {
                            analysis.add(
                                35,
                                "wm_suspicious_runkey_then_lolbin",
                                "Run/RunOnce activity followed by LOLBIN/script start",
                            );
                        }
                    }
                }
            }
            EventData::TcpIp { daddr, dport, .. } | EventData::UdpIp { daddr, dport, .. } => {
                // Track connection destinations to detect basic port scanning.
                host.conn_events.push_back(ConnEvent {
                    ts: now,
                    dst_ip: *daddr,
                    dst_port: *dport,
                });

                let mut unique_ports = HashSet::new();
                for evt in host.conn_events.iter().filter(|e| e.dst_ip == *daddr) {
                    unique_ports.insert(evt.dst_port);
                }

                // If we see a lot of distinct destination ports on the same dst_ip quickly, flag it.
                let port_scan_threshold = 40usize;
                if unique_ports.len() >= port_scan_threshold {
                    analysis.add(
                        40,
                        "wm_suspicious_port_scan",
                        format!(
                            "possible port scan: {} distinct ports to {} in last 60s",
                            unique_ports.len(),
                            daddr
                        ),
                    );
                }

                // Lateral movement-ish: many targets on SMB/RDP within a short window.
                match *dport {
                    445 => {
                        host.lateral_445.push_back(LateralConn {
                            ts: now,
                            dst_ip: *daddr,
                        });
                        let unique_targets = unique_lateral_targets(&host.lateral_445);
                        if unique_targets >= 15 {
                            analysis.add(
                                45,
                                "wm_suspicious_lateral_smb",
                                format!("SMB to many targets: {unique_targets} in last 60s"),
                            );
                        }
                    }
                    3389 => {
                        host.lateral_3389.push_back(LateralConn {
                            ts: now,
                            dst_ip: *daddr,
                        });
                        let unique_targets = unique_lateral_targets(&host.lateral_3389);
                        if unique_targets >= 10 {
                            analysis.add(
                                40,
                                "wm_suspicious_lateral_rdp",
                                format!("RDP to many targets: {unique_targets} in last 60s"),
                            );
                        }
                    }
                    _ => {}
                }

                // Suspicious process -> outbound network chain (per-host, per-PID; avoids cross-host mixing).
                let pid = match &record.event.data {
                    EventData::TcpIp { pid, .. } | EventData::UdpIp { pid, .. } => *pid,
                    _ => record.event.process_id,
                };

                if let Some(proc_state) = host.processes.get(&pid) {
                    // If the process was already suspicious and it connects shortly after start, boost.
                    let suspicious_base_threshold = 40u32;
                    if proc_state.base_score >= suspicious_base_threshold
                        && now.duration_since(proc_state.started) <= Duration::from_secs(60)
                    {
                        analysis.add(
                            30,
                            "wm_suspicious_spawn_then_network",
                            format!(
                                "suspicious process connected soon after start: pid={pid} image={image}",
                                image = proc_state.image_lower
                            ),
                        );
                    }

                    // Shell/LOLBIN that talks on the network is often high-signal.
                    if is_lolbin_or_script_child(&proc_state.image_lower, &proc_state.command_lower)
                        && now.duration_since(proc_state.started) <= Duration::from_secs(2 * 60)
                    {
                        analysis.add(
                            20,
                            "wm_suspicious_lolbin_network",
                            format!("LOLBIN/script made network connection: pid={pid}"),
                        );
                    }
                }
            }
            EventData::FileReadWrite { file_path, .. } if record.event.opcode == 68 => {
                // Chain: suspicious write location -> later execution.
                let path_lower = file_path.to_ascii_lowercase();
                if looks_executable_path(&path_lower) && is_user_writable_path(&path_lower) {
                    host.file_drops.push_back(FileDrop {
                        ts: now,
                        path_lower,
                    });
                }
            }
            EventData::FileInfo { file_path, .. } if record.event.opcode == 71 => {
                // Rename into an executable in a user-writable folder can indicate staging.
                let path_lower = file_path.to_ascii_lowercase();
                if looks_executable_path(&path_lower) && is_user_writable_path(&path_lower) {
                    analysis.add(
                        25,
                        "wm_suspicious_rename_to_executable",
                        "file renamed into executable/script in user-writable directory",
                    );
                    host.file_drops.push_back(FileDrop {
                        ts: now,
                        path_lower,
                    });
                }
            }
            EventData::Registry { key_name, .. } => {
                // Capture Run/RunOnce modifications to correlate with later behavior.
                let key_lower = key_name.to_ascii_lowercase();
                if is_run_key(&key_lower)
                    && (record.event.opcode == 14 || record.event.opcode == 20)
                {
                    host.run_key_events.push_back(RunKeyEvent { ts: now });
                }
            }
            _ => {}
        }

        analysis.dedupe();
        analysis
    }
}

fn prune_older(queue: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while let Some(front) = queue.front().copied() {
        if now.duration_since(front) > window {
            queue.pop_front();
        } else {
            break;
        }
    }
}

fn prune_older_conn(queue: &mut VecDeque<ConnEvent>, now: Instant, window: Duration) {
    while let Some(front) = queue.front().copied() {
        if now.duration_since(front.ts) > window {
            queue.pop_front();
        } else {
            break;
        }
    }
}

fn prune_processes(map: &mut HashMap<u32, ProcessState>, now: Instant, window: Duration) {
    map.retain(|_, v| now.duration_since(v.started) <= window);
}

fn prune_older_filedrops(queue: &mut VecDeque<FileDrop>, now: Instant, window: Duration) {
    while let Some(front) = queue.front() {
        if now.duration_since(front.ts) > window {
            queue.pop_front();
        } else {
            break;
        }
    }
}

fn prune_older_runkeys(queue: &mut VecDeque<RunKeyEvent>, now: Instant, window: Duration) {
    while let Some(front) = queue.front() {
        if now.duration_since(front.ts) > window {
            queue.pop_front();
        } else {
            break;
        }
    }
}

fn prune_older_lateral(queue: &mut VecDeque<LateralConn>, now: Instant, window: Duration) {
    while let Some(front) = queue.front().copied() {
        if now.duration_since(front.ts) > window {
            queue.pop_front();
        } else {
            break;
        }
    }
}

fn unique_lateral_targets(queue: &VecDeque<LateralConn>) -> usize {
    let mut uniq = HashSet::new();
    for evt in queue.iter() {
        uniq.insert(evt.dst_ip);
    }
    uniq.len()
}

fn is_office_or_browser_parent(image_lower: &str) -> bool {
    // Parent process names are approximate; we only have image filename/path.
    image_lower.ends_with("winword.exe")
        || image_lower.ends_with("excel.exe")
        || image_lower.ends_with("powerpnt.exe")
        || image_lower.ends_with("outlook.exe")
        || image_lower.ends_with("onenote.exe")
        || image_lower.ends_with("msedge.exe")
        || image_lower.ends_with("chrome.exe")
        || image_lower.ends_with("firefox.exe")
        || image_lower.ends_with("iexplore.exe")
}

fn is_lolbin_or_script_child(image_lower: &str, command_lower: &str) -> bool {
    image_lower.ends_with("powershell.exe")
        || image_lower.ends_with("pwsh.exe")
        || image_lower.ends_with("cmd.exe")
        || image_lower.ends_with("mshta.exe")
        || image_lower.ends_with("rundll32.exe")
        || image_lower.ends_with("regsvr32.exe")
        || image_lower.ends_with("wscript.exe")
        || image_lower.ends_with("cscript.exe")
        || image_lower.ends_with("certutil.exe")
        || image_lower.ends_with("bitsadmin.exe")
        || command_lower.contains("powershell")
        || command_lower.contains("mshta")
        || command_lower.contains("rundll32")
        || command_lower.contains("regsvr32")
}

fn is_run_key(key_lower: &str) -> bool {
    key_lower.contains("\\software\\microsoft\\windows\\currentversion\\run")
        || key_lower.contains("\\software\\microsoft\\windows\\currentversion\\runonce")
}

fn looks_executable_path(path_lower: &str) -> bool {
    path_lower.ends_with(".exe")
        || path_lower.ends_with(".dll")
        || path_lower.ends_with(".sys")
        || path_lower.ends_with(".ps1")
        || path_lower.ends_with(".bat")
        || path_lower.ends_with(".vbs")
        || path_lower.ends_with(".js")
}

fn is_user_writable_path(path_lower: &str) -> bool {
    (path_lower.contains("\\users\\")
        && (path_lower.contains("\\appdata\\") || path_lower.contains("\\temp\\")))
        || path_lower.contains("\\programdata\\")
}

fn analyze_record(record: &CapturedEventRecord) -> SuspiciousAnalysis {
    let mut analysis = SuspiciousAnalysis::default();

    match &record.event.data {
        EventData::Process {
            image_file_name,
            command_line,
            ..
        } if record.event.opcode == 1 => {
            analysis.merge(analyze_process_start(image_file_name, command_line));
        }
        EventData::Registry { key_name, .. } => {
            analysis.merge(analyze_registry(key_name));
        }
        EventData::FileCreate { open_path, .. }
        | EventData::FileInfo {
            file_path: open_path,
            ..
        } => {
            analysis.merge(analyze_file_path(open_path));
        }
        EventData::TcpIp { dport, daddr, .. } | EventData::UdpIp { dport, daddr, .. } => {
            analysis.merge(analyze_network(*dport, daddr));
        }
        _ => {}
    }

    analysis
}

fn analyze_process_start(image_file_name: &str, command_line: &str) -> SuspiciousAnalysis {
    let mut analysis = SuspiciousAnalysis::default();

    let img = image_file_name.to_ascii_lowercase();
    let cmd = command_line.to_ascii_lowercase();

    // LOLBins / scripting engines (not always bad, but good signals to score with args).
    let is_powershell = img.ends_with("powershell.exe") || cmd.contains("powershell");
    let is_cmd = img.ends_with("cmd.exe") || cmd.starts_with("cmd.exe") || cmd.contains(" cmd ");
    let is_mshta = img.ends_with("mshta.exe") || cmd.contains("mshta");
    let is_rundll32 = img.ends_with("rundll32.exe") || cmd.contains("rundll32");
    let is_regsvr32 = img.ends_with("regsvr32.exe") || cmd.contains("regsvr32");
    let is_wscript = img.ends_with("wscript.exe")
        || img.ends_with("cscript.exe")
        || cmd.contains("wscript")
        || cmd.contains("cscript");
    let is_certutil = img.ends_with("certutil.exe") || cmd.contains("certutil");
    let is_bitsadmin = img.ends_with("bitsadmin.exe") || cmd.contains("bitsadmin");

    // PowerShell encoded / hidden / download patterns.
    if is_powershell {
        if cmd.contains(" -enc ") || cmd.contains(" -encodedcommand ") {
            analysis.add(
                60,
                "wm_suspicious_powershell_encoded",
                "PowerShell encoded command",
            );
        }
        if cmd.contains("invoke-webrequest")
            || cmd.contains("iwr ")
            || cmd.contains("downloadstring")
            || cmd.contains("new-object net.webclient")
            || cmd.contains("iex(")
            || cmd.contains("invoke-expression")
        {
            analysis.add(
                45,
                "wm_suspicious_powershell_download",
                "PowerShell download/execute pattern",
            );
        }
        if cmd.contains("-windowstyle hidden") || cmd.contains("-w hidden") {
            analysis.add(15, "wm_suspicious_hidden_window", "Hidden window");
        }
    }

    // cmd.exe execution chaining.
    if is_cmd {
        if cmd.contains(" /c ") {
            analysis.add(10, "wm_suspicious_cmd_shell", "cmd.exe /c usage");
        }
        if cmd.contains("&&") || cmd.contains("| ") || cmd.contains(" >") {
            analysis.add(
                10,
                "wm_suspicious_cmd_chaining",
                "command chaining / redirection",
            );
        }
    }

    // mshta executing remote content.
    if is_mshta && (cmd.contains("http://") || cmd.contains("https://")) {
        analysis.add(70, "wm_suspicious_mshta_remote", "mshta remote URL");
    }

    // rundll32 executing via URL / javascript.
    if is_rundll32
        && (cmd.contains("http://") || cmd.contains("https://") || cmd.contains("javascript:"))
    {
        analysis.add(
            70,
            "wm_suspicious_rundll32_remote",
            "rundll32 remote/script execution",
        );
    }

    // regsvr32 /i:http(s)
    if is_regsvr32 && (cmd.contains("/i:http") || cmd.contains("/i:https")) {
        analysis.add(
            70,
            "wm_suspicious_regsvr32_remote",
            "regsvr32 /i:http(s) pattern",
        );
    }

    // wscript/cscript running from temp/user dirs.
    if is_wscript && (cmd.contains("\\appdata\\") || cmd.contains("\\temp\\")) {
        analysis.add(
            35,
            "wm_suspicious_script_from_temp",
            "script interpreter running from AppData/Temp",
        );
    }

    // certutil download-ish patterns.
    if is_certutil
        && (cmd.contains("-urlcache")
            || cmd.contains("-split")
            || cmd.contains("http://")
            || cmd.contains("https://"))
    {
        analysis.add(
            50,
            "wm_suspicious_certutil_download",
            "certutil download pattern",
        );
    }

    // bitsadmin download patterns.
    if is_bitsadmin
        && (cmd.contains("/transfer") || cmd.contains("http://") || cmd.contains("https://"))
    {
        analysis.add(
            40,
            "wm_suspicious_bitsadmin_download",
            "bitsadmin download pattern",
        );
    }

    // Add a small baseline score for LOLBins to make filtering easier.
    if is_powershell
        || is_mshta
        || is_rundll32
        || is_regsvr32
        || is_wscript
        || is_certutil
        || is_bitsadmin
    {
        analysis.add(
            5,
            "wm_suspicious_lolbin",
            format!("LOLBIN/scripting process: {image_file_name}"),
        );
    }

    analysis
}

fn analyze_registry(key_name: &str) -> SuspiciousAnalysis {
    let mut analysis = SuspiciousAnalysis::default();
    let key = key_name.to_ascii_lowercase();

    // Persistence-related keys.
    if key.contains("\\software\\microsoft\\windows\\currentversion\\run")
        || key.contains("\\software\\microsoft\\windows\\currentversion\\runonce")
    {
        analysis.add(
            55,
            "wm_suspicious_persistence_runkey",
            "registry write to Run/RunOnce key",
        );
    }

    // Service installation/modification (very rough; depends on collector producing key names).
    if key.contains("\\system\\currentcontrolset\\services\\") {
        analysis.add(
            35,
            "wm_suspicious_service_registry",
            "registry access under Services (possible service persistence)",
        );
    }

    analysis
}

fn analyze_file_path(path: &str) -> SuspiciousAnalysis {
    let mut analysis = SuspiciousAnalysis::default();
    let p = path.to_ascii_lowercase();

    // Suspicious write locations.
    if p.contains("\\users\\")
        && (p.contains("\\appdata\\local\\temp\\") || p.contains("\\appdata\\roaming\\"))
    {
        analysis.add(
            15,
            "wm_suspicious_user_write_location",
            "file activity in AppData/Temp",
        );
    }

    // Startup folder persistence.
    if p.contains("\\start menu\\programs\\startup\\") {
        analysis.add(
            60,
            "wm_suspicious_startup_folder",
            "file activity in Startup folder",
        );
    }

    // Executable-like extensions in user-writable areas.
    let looks_executable = p.ends_with(".exe")
        || p.ends_with(".dll")
        || p.ends_with(".sys")
        || p.ends_with(".ps1")
        || p.ends_with(".bat")
        || p.ends_with(".vbs")
        || p.ends_with(".js");

    if looks_executable && (p.contains("\\appdata\\") || p.contains("\\temp\\")) {
        analysis.add(
            40,
            "wm_suspicious_dropper_path",
            "executable/script created in user-writable directory",
        );
    }

    analysis
}

fn analyze_network(dport: u16, daddr: &IpAddr) -> SuspiciousAnalysis {
    let mut analysis = SuspiciousAnalysis::default();

    // Very common C2 / reverse shell / pentest ports.
    const SUSPICIOUS_PORTS: &[u16] = &[4444, 4443, 1337, 31337, 6666, 6667, 9001];
    if SUSPICIOUS_PORTS.contains(&dport) {
        analysis.add(
            35,
            "wm_suspicious_destination_port",
            format!("connection to suspicious port {dport} ({daddr})"),
        );
    }

    // Outbound SMB/RDP to non-local can be interesting; this is intentionally conservative.
    if dport == 445 {
        analysis.add(
            10,
            "wm_suspicious_smb",
            format!("SMB traffic observed to {daddr}"),
        );
    }
    if dport == 3389 {
        analysis.add(
            10,
            "wm_suspicious_rdp",
            format!("RDP traffic observed to {daddr}"),
        );
    }

    analysis
}

/// Convert an ECS struct to JSON and enrich it with a `wm.suspicious` section and additional tags.
///
/// This avoids coupling `wm-data-service` to the generated ECS Rust types.
pub fn enrich_ecs_json<T: Serialize>(ecs: &T, analysis: &SuspiciousAnalysis) -> Value {
    let mut value = serde_json::to_value(ecs).unwrap_or_else(|_| json!({}));

    if analysis.score == 0 && analysis.tags.is_empty() && analysis.reasons.is_empty() {
        return value;
    }

    // 1) Add/merge tags at the root ECS `tags` field.
    if !analysis.tags.is_empty() {
        match value.get_mut("tags") {
            Some(Value::Array(arr)) => {
                let mut seen = HashSet::new();
                for v in arr.iter() {
                    if let Some(s) = v.as_str() {
                        seen.insert(s.to_string());
                    }
                }
                for tag in &analysis.tags {
                    if seen.insert(tag.clone()) {
                        arr.push(Value::String(tag.clone()));
                    }
                }
            }
            _ => {
                value["tags"] =
                    Value::Array(analysis.tags.iter().cloned().map(Value::String).collect());
            }
        }
    }

    // 2) Add a non-ECS namespace for detector outputs.
    if !matches!(value, Value::Object(_)) {
        value = json!({});
    }

    value["wm"]["suspicious"] = json!({
        "score": analysis.score,
        "tags": analysis.tags,
        "reasons": analysis.reasons,
    });

    value
}
