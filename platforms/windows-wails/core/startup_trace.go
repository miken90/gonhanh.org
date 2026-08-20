package core

// Lightweight startup stage tracer (Phase 0 instrumentation).
//
// Records a process-entry wall-clock timestamp and per-stage elapsed deltas
// so a boot can be compared against external signals (Winlogon 7001,
// TaskScheduler Operational 129/201, explorer.exe StartTime) to tell apart
// "process created late" (external cause: Defender scan, logon throttling,
// scheduler dispatch) from "process created on time but slow internally".
//
// One line per boot is appended to %LOCALAPPDATA%\FKey\startup.log. The file
// is capped at ~64KB, dropping the oldest lines, so it never grows unbounded
// across many boots. Any failure to write is swallowed — this instrumentation
// must never be the reason startup crashes or stalls.

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const startupLogCapBytes = 64 * 1024

var (
	startupTraceMu    sync.Mutex
	startupTraceStart time.Time
	startupTraceLast  time.Time
	startupTraceParts []string
)

// StartupTraceBegin records the process-entry wall-clock timestamp. Call this
// as the very first statement in main() so entry time can be diffed against
// logon/explorer.exe timestamps.
func StartupTraceBegin() {
	startupTraceMu.Lock()
	defer startupTraceMu.Unlock()

	now := time.Now()
	startupTraceStart = now
	startupTraceLast = now
	startupTraceParts = []string{fmt.Sprintf("entry=%s", now.Format("2006-01-02T15:04:05.000"))}
}

// StartupTraceStage records the elapsed time since the previous stage
// boundary (and since process entry) under the given stage name. Call this at
// each existing startup boundary in main.go without reordering startup.
func StartupTraceStage(name string) {
	startupTraceMu.Lock()
	defer startupTraceMu.Unlock()

	if startupTraceStart.IsZero() {
		return
	}
	now := time.Now()
	delta := now.Sub(startupTraceLast)
	total := now.Sub(startupTraceStart)
	startupTraceLast = now
	startupTraceParts = append(startupTraceParts,
		fmt.Sprintf("%s=+%dms(total=%dms)", name, delta.Milliseconds(), total.Milliseconds()))
}

// StartupTraceFinish appends the collected boot trace as a single line to
// %LOCALAPPDATA%\FKey\startup.log. Never crashes startup: any I/O failure
// (missing LOCALAPPDATA, permission error, disk full) is silently ignored.
func StartupTraceFinish() {
	startupTraceMu.Lock()
	line := strings.Join(startupTraceParts, " ")
	startupTraceMu.Unlock()

	if line == "" {
		return
	}

	defer func() { recover() }()

	dir, err := fkeyLogDir()
	if err != nil {
		return
	}
	appendLogLineCapped(filepath.Join(dir, "startup.log"), line, startupLogCapBytes)
}

// fkeyLogDir returns (and creates) %LOCALAPPDATA%\FKey, matching the
// convention already used for DLL extraction (see dll_embed.go).
func fkeyLogDir() (string, error) {
	localAppData := os.Getenv("LOCALAPPDATA")
	if localAppData == "" {
		return "", fmt.Errorf("LOCALAPPDATA not set")
	}
	dir := filepath.Join(localAppData, "FKey")
	if err := os.MkdirAll(dir, 0755); err != nil {
		return "", err
	}
	return dir, nil
}

// appendLogLineCapped appends line (plus newline) to path, truncating the
// file to keep only the most recent maxBytes when it would otherwise grow
// past that cap. Shared by the startup tracer and the digit-path debug log.
func appendLogLineCapped(path, line string, maxBytes int64) {
	existing, _ := os.ReadFile(path)

	buf := make([]byte, 0, len(existing)+len(line)+1)
	buf = append(buf, existing...)
	buf = append(buf, []byte(line)...)
	buf = append(buf, '\n')

	if int64(len(buf)) > maxBytes {
		// Keep the tail: drop oldest lines until under the cap, then drop a
		// partial leading line so the file starts cleanly.
		buf = buf[int64(len(buf))-maxBytes:]
		if idx := bytes.IndexByte(buf, '\n'); idx >= 0 {
			buf = buf[idx+1:]
		}
	}

	_ = os.WriteFile(path, buf, 0644)
}
