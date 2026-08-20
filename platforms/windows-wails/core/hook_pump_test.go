package core

import (
	"testing"
	"time"
)

// TestHookPump_StartStop exercises the dedicated-thread lifecycle end to
// end: installing WH_KEYBOARD_LL/WH_MOUSE_LL on a locked OS thread, running
// its message pump, and shutting it down cleanly via PostThreadMessageW.
// This is the actual threading-correctness concern from the Phase 1
// migration (deadlock-free start/stop, no hang waiting on the pump
// goroutine) - it doesn't require a live Wails app or simulated keystrokes.
func TestHookPump_StartStop(t *testing.T) {
	hook := NewKeyboardHook()
	mouseHook := NewMouseHook()
	pump := newHookPump(hook, mouseHook)

	if err := pump.start(); err != nil {
		t.Fatalf("pump.start() returned error: %v", err)
	}

	if hook.hookID == 0 {
		t.Error("keyboard hook was not installed (hookID == 0) after start()")
	}
	if mouseHook.hookID == 0 {
		t.Error("mouse hook was not installed (hookID == 0) after start()")
	}

	stopped := make(chan struct{})
	go func() {
		pump.stop()
		close(stopped)
	}()

	select {
	case <-stopped:
	case <-time.After(5 * time.Second):
		t.Fatal("pump.stop() did not return within 5s - pump thread likely deadlocked")
	}

	if hook.hookID != 0 {
		t.Error("keyboard hook still installed (hookID != 0) after stop()")
	}
	if mouseHook.hookID != 0 {
		t.Error("mouse hook still installed (hookID != 0) after stop()")
	}
}

// TestHookPump_StopWithoutStart verifies stop() is a safe no-op when
// start() was never called - it must not block or panic.
func TestHookPump_StopWithoutStart(t *testing.T) {
	pump := newHookPump(NewKeyboardHook(), NewMouseHook())

	done := make(chan struct{})
	go func() {
		pump.stop()
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("stop() without start() blocked instead of returning immediately")
	}
}

// TestHookPump_MultipleStartStopCycles verifies the dedicated thread can be
// torn down and rebuilt repeatedly without leaking threads or hooks -
// relevant since ImeLoop.Start()/Stop() can be called across settings
// toggles or (in principle) app restarts within the same process.
func TestHookPump_MultipleStartStopCycles(t *testing.T) {
	for i := 0; i < 3; i++ {
		hook := NewKeyboardHook()
		mouseHook := NewMouseHook()
		pump := newHookPump(hook, mouseHook)

		if err := pump.start(); err != nil {
			t.Fatalf("cycle %d: pump.start() returned error: %v", i, err)
		}

		stopped := make(chan struct{})
		go func() {
			pump.stop()
			close(stopped)
		}()

		select {
		case <-stopped:
		case <-time.After(5 * time.Second):
			t.Fatalf("cycle %d: pump.stop() did not return within 5s", i)
		}
	}
}
