package core

// Digit-path debug logging (Phase 0 instrumentation), gated by FKEY_DEBUG=1.
//
// Two pieces:
//   - A small ring buffer (last 16) of keys the hook saw but IsRelevantKey
//     never routes to processKey (arrows, Home/End/PgUp/PgDn, Delete,
//     F-keys, numpad digits, Win) — these are the keys hypothesis H0 says
//     leave the IME's internal buffer stale relative to what's on screen.
//   - A digit-path log line, written whenever a top-row digit (VK 0x30-0x39)
//     is processed, dumping that ring buffer alongside it so a repro can be
//     matched against the untracked keys that preceded it.
//
// Every call site gates on FKeyDebugEnabled first, so this costs a single
// bool check when FKEY_DEBUG is not set. Logging itself never blocks the
// hook: it goes through a buffered, non-blocking channel, same pattern as
// keyTimingCh in keyboard_hook.go.

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

// FKeyDebugEnabled is resolved once at process start from FKEY_DEBUG=1.
var FKeyDebugEnabled = os.Getenv("FKEY_DEBUG") == "1"

const ignoredKeyRingSize = 16
const debugLogCapBytes = 64 * 1024

var (
	ignoredKeyRingMu  sync.Mutex
	ignoredKeyRing    [ignoredKeyRingSize]string
	ignoredKeyRingPos int
	ignoredKeyRingLen int
)

// isUntrackedRingKey reports whether vk is one of the key classes
// IsRelevantKey never routes to processKey: arrows, Home/End/PgUp/PgDn,
// Delete, F-keys, numpad digits, and the Win keys.
func isUntrackedRingKey(vk uint16) bool {
	switch {
	case vk >= 0x25 && vk <= 0x28: // Left/Up/Right/Down
		return true
	case vk == 0x21 || vk == 0x22 || vk == 0x23 || vk == 0x24: // PgUp/PgDn/End/Home
		return true
	case vk == 0x2E: // Delete
		return true
	case vk >= 0x70 && vk <= 0x7B: // F1-F12
		return true
	case vk >= 0x60 && vk <= 0x69: // Numpad 0-9
		return true
	case vk == 0x5B || vk == 0x5C: // LWin/RWin
		return true
	}
	return false
}

// recordIgnoredKey pushes vk into the ignored-key ring buffer when it is one
// of the untracked classes above. No-op unless FKEY_DEBUG=1.
func recordIgnoredKey(vk uint16) {
	if !FKeyDebugEnabled || !isUntrackedRingKey(vk) {
		return
	}
	ignoredKeyRingMu.Lock()
	ignoredKeyRing[ignoredKeyRingPos] = fmt.Sprintf("0x%02X", vk)
	ignoredKeyRingPos = (ignoredKeyRingPos + 1) % ignoredKeyRingSize
	if ignoredKeyRingLen < ignoredKeyRingSize {
		ignoredKeyRingLen++
	}
	ignoredKeyRingMu.Unlock()
}

// snapshotIgnoredKeyRing returns the ring buffer contents, oldest first.
func snapshotIgnoredKeyRing() string {
	ignoredKeyRingMu.Lock()
	defer ignoredKeyRingMu.Unlock()

	if ignoredKeyRingLen == 0 {
		return "(empty)"
	}
	out := make([]string, 0, ignoredKeyRingLen)
	start := (ignoredKeyRingPos - ignoredKeyRingLen + ignoredKeyRingSize) % ignoredKeyRingSize
	for i := 0; i < ignoredKeyRingLen; i++ {
		out = append(out, ignoredKeyRing[(start+i)%ignoredKeyRingSize])
	}
	return strings.Join(out, ",")
}

var (
	debugLogCh         = make(chan string, 64)
	debugLogLoggerOnce sync.Once
)

// startDebugLogger lazily starts the background writer for debug.log.
func startDebugLogger() {
	debugLogLoggerOnce.Do(func() {
		go func() {
			dir, err := fkeyLogDir()
			if err != nil {
				// Drain the channel so producers never block, even though
				// nothing gets written.
				for range debugLogCh {
				}
				return
			}
			path := filepath.Join(dir, "debug.log")
			for line := range debugLogCh {
				appendLogLineCapped(path, line, debugLogCapBytes)
			}
		}()
	})
}

// logDigitDebug enqueues a digit-path debug line for the background writer.
// Non-blocking: if the channel is full, the line is dropped rather than
// stalling the hook. No-op unless FKEY_DEBUG=1.
func logDigitDebug(line string) {
	if !FKeyDebugEnabled {
		return
	}
	startDebugLogger()
	select {
	case debugLogCh <- line:
	default:
		log.Printf("[Debug] debug.log channel full, dropped: %s", line)
	}
}

// injectionMethodName returns a human-readable name for an InjectionMethod,
// for the digit debug log line.
func injectionMethodName(m InjectionMethod) string {
	switch m {
	case MethodFast:
		return "fast"
	case MethodSlow:
		return "slow"
	case MethodAtomic:
		return "atomic"
	case MethodPaste:
		return "paste"
	case MethodPassthrough:
		return "passthrough"
	case MethodPasteShiftV:
		return "paste-shift-v"
	default:
		return fmt.Sprintf("unknown(%d)", int(m))
	}
}
