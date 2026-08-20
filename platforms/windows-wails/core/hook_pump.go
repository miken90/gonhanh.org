package core

// Dedicated OS-thread message pump for the low-level keyboard/mouse hooks.
//
// WH_KEYBOARD_LL / WH_MOUSE_LL are only delivered to the thread that
// installed them, and only while that thread is pumping its message queue -
// a thread that never calls GetMessage stalls system-wide keyboard input,
// and Windows silently removes hooks that don't respond within
// LowLevelHooksTimeout. The previous design relied on the Wails main
// thread's own message loop (started inside application.App.Run()) to pump
// for it, so no keystrokes worked until Wails had finished initializing.
// This file gives the hooks their own OS thread with a minimal message
// pump, started independently of Wails, so typing works while Wails is
// still starting up.

import (
	"log"
	"runtime"
	"sync"
	"unsafe"
)

const wmQuit = 0x0012

// winMSG mirrors the Win32 MSG structure (winuser.h). Field order and
// implicit padding must match exactly for GetMessageW/DispatchMessageW.
type winMSG struct {
	Hwnd    uintptr
	Message uint32
	WParam  uintptr
	LParam  uintptr
	Time    uint32
	Pt      struct{ X, Y int32 }
}

var (
	procGetMessageW        = user32.NewProc("GetMessageW")
	procTranslateMessage   = user32.NewProc("TranslateMessage")
	procDispatchMessageW   = user32.NewProc("DispatchMessageW")
	procPostThreadMessageW = user32.NewProc("PostThreadMessageW")
	procGetCurrentThreadID = kernel32.NewProc("GetCurrentThreadId")
)

// hookPump owns the dedicated, locked OS thread that installs and services
// the keyboard and mouse hooks.
type hookPump struct {
	hook      *KeyboardHook
	mouseHook *MouseHook

	mu       sync.Mutex
	threadID uint32
	done     chan struct{} // closed once the pump goroutine has unhooked and returned
}

func newHookPump(hook *KeyboardHook, mouseHook *MouseHook) *hookPump {
	return &hookPump{hook: hook, mouseHook: mouseHook}
}

// start installs the hooks on a freshly locked OS thread and runs that
// thread's message pump in the background until stop() is called. Blocks
// until the hooks are installed (or installation fails).
func (p *hookPump) start() error {
	startErr := make(chan error, 1)
	p.done = make(chan struct{})

	go p.run(startErr)

	return <-startErr
}

// run is the body of the dedicated pump goroutine. It must never return
// without closing p.done, or stop() would block forever.
func (p *hookPump) run(startErr chan<- error) {
	runtime.LockOSThread()
	// Intentionally never call UnlockOSThread: this goroutine (and its OS
	// thread) live exactly as long as the hooks do. Per runtime.LockOSThread
	// docs, a goroutine that exits without unlocking terminates its OS
	// thread instead of returning it to the pool, which is the outcome we
	// want here - SetWindowsHookExW/UnhookWindowsHookEx have thread
	// affinity, so this thread must not be reused for anything else.
	defer close(p.done)

	// SetWindowsHookExW must be called on the thread that will pump for it.
	if err := p.hook.Start(); err != nil {
		startErr <- err
		return
	}
	if err := p.mouseHook.Start(); err != nil {
		// Non-fatal, matches prior behavior: typing still works via the
		// keyboard hook alone, just without the click-clears-buffer net.
		log.Printf("[HookPump] failed to start mouse hook: %v", err)
	}

	tid, _, _ := procGetCurrentThreadID.Call()
	p.mu.Lock()
	p.threadID = uint32(tid)
	p.mu.Unlock()

	startErr <- nil

	// Message pump: keeps this thread's queue serviced so the LL hooks keep
	// receiving callbacks. GetMessageW blocks until a message arrives; it
	// returns 0 on WM_QUIT (posted by stop()) and -1 on error.
	var msg winMSG
	for {
		ret, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
		if int32(ret) <= 0 {
			break
		}
		procTranslateMessage.Call(uintptr(unsafe.Pointer(&msg)))
		procDispatchMessageW.Call(uintptr(unsafe.Pointer(&msg)))
	}

	// Unhook on this same thread, before it terminates. UnhookWindowsHookEx
	// has no documented thread-affinity requirement (unlike
	// SetWindowsHookExW), but doing it here keeps the hooks' full lifecycle
	// on the one thread that owns them and guarantees it happens before the
	// thread that serviced them is gone.
	p.hook.Stop()
	p.mouseHook.Stop()
}

// stop signals the pump thread to exit (via a posted WM_QUIT) and blocks
// until it has unhooked and the thread has terminated. No-op if start()
// was never called or already failed.
func (p *hookPump) stop() {
	p.mu.Lock()
	threadID := p.threadID
	done := p.done
	p.mu.Unlock()

	if threadID == 0 || done == nil {
		return
	}

	procPostThreadMessageW.Call(uintptr(threadID), wmQuit, 0, 0)
	<-done
}
