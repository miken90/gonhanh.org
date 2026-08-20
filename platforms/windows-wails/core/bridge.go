package core

// Rust FFI bridge using syscall.LoadDLL (no CGo required)
// Port of RustBridge.cs from .NET implementation
//
// DLL: fkey_core.dll (renamed from gonhanh_core.dll)
// Must be in same directory as executable

import (
	"sync"
	"syscall"
	"unsafe"
)

// InputMethod type
type InputMethod uint8

const (
	Telex InputMethod = 0
	VNI   InputMethod = 1
)

// ImeAction type
type ImeAction uint8

const (
	ActionNone    ImeAction = 0
	ActionSend    ImeAction = 1
	ActionRestore ImeAction = 2
)

// ImeResult represents the result from Rust engine
type ImeResult struct {
	Action    ImeAction
	Backspace uint8
	Count     uint8
	Chars     []rune
}

// cResult mirrors the Rust FFI struct byte-for-byte:
//
//	#[repr(C)]
//	pub struct Result {
//	    pub chars: [u32; MAX],  // MAX = 256, see core/src/engine/buffer.rs
//	    pub action: u8,
//	    pub backspace: u8,
//	    pub count: u8,
//	    pub flags: u8,
//	}
//
// (core/src/engine/mod.rs, returned by ime_key_ext in core/src/lib.rs).
// [256]uint32 + 4 bytes of u8 fields = 1028 bytes total, 4-byte aligned,
// matching Go's layout for this field order exactly. flags exists in the
// Rust struct (key-consumed bit) but was never read by the prior manual
// byte-parsing either - preserved here as an unused trailing field so the
// layout stays byte-identical rather than silently changing behavior.
type cResult struct {
	chars     [256]uint32
	action    uint8
	backspace uint8
	count     uint8
	flags     uint8
}

// Compile-time layout assertion: cResult must be exactly 1028 bytes, the
// size the DLL actually allocates and frees via ime_free.
var _ [1028 - unsafe.Sizeof(cResult{})]byte
var _ [unsafe.Sizeof(cResult{}) - 1028]byte

// GetText returns the result text as a string
func (r ImeResult) GetText() string {
	return string(r.Chars)
}

// Bridge holds DLL handles and proc addresses
type Bridge struct {
	dll *syscall.DLL
	mu  sync.Mutex

	// Proc addresses
	pImeInit               *syscall.Proc
	pImeClear              *syscall.Proc
	pImeFree               *syscall.Proc
	pImeMethod             *syscall.Proc
	pImeEnabled            *syscall.Proc
	pImeModern             *syscall.Proc
	pImeKeyExt             *syscall.Proc
	pImeSkipWShortcut      *syscall.Proc
	pImeBracketShortcut    *syscall.Proc
	pImeEscRestore         *syscall.Proc
	pImeFreeTone           *syscall.Proc
	pImeEnglishAutoRestore *syscall.Proc
	pImeAutoCapitalize     *syscall.Proc
	pImeClearAll           *syscall.Proc
	pImeGetBuffer          *syscall.Proc
	pImeRestoreWord        *syscall.Proc
	pImeAddShortcut        *syscall.Proc
	pImeRemoveShortcut     *syscall.Proc
	pImeClearShortcuts     *syscall.Proc
}

// Global bridge instance
var (
	bridge     *Bridge
	bridgeOnce sync.Once
	bridgeErr  error
)

// DLLPath is set by main package before GetBridge() is called
// This allows embedded DLL extraction to work
var DLLPath string

// DLL names to try (fkey_core.dll first, fallback to gonhanh_core.dll for compatibility)
var dllNames = []string{"fkey_core.dll", "gonhanh_core.dll"}

// GetBridge returns the singleton bridge instance
func GetBridge() (*Bridge, error) {
	bridgeOnce.Do(func() {
		bridge, bridgeErr = newBridge()
	})
	return bridge, bridgeErr
}

func newBridge() (*Bridge, error) {
	var dll *syscall.DLL
	var err error

	// Try embedded DLL path first (set by main package)
	if DLLPath != "" {
		dll, err = syscall.LoadDLL(DLLPath)
		if err == nil {
			goto loaded
		}
	}

	// Fallback: try each DLL name in current directory
	for _, name := range dllNames {
		dll, err = syscall.LoadDLL(name)
		if err == nil {
			break
		}
	}
	if err != nil {
		return nil, err
	}

loaded:

	b := &Bridge{dll: dll}

	// Load all proc addresses
	b.pImeInit, _ = dll.FindProc("ime_init")
	b.pImeClear, _ = dll.FindProc("ime_clear")
	b.pImeFree, _ = dll.FindProc("ime_free")
	b.pImeMethod, _ = dll.FindProc("ime_method")
	b.pImeEnabled, _ = dll.FindProc("ime_enabled")
	b.pImeModern, _ = dll.FindProc("ime_modern")
	b.pImeKeyExt, _ = dll.FindProc("ime_key_ext")
	b.pImeSkipWShortcut, _ = dll.FindProc("ime_skip_w_shortcut")
	b.pImeBracketShortcut, _ = dll.FindProc("ime_bracket_shortcut")
	b.pImeEscRestore, _ = dll.FindProc("ime_esc_restore")
	b.pImeFreeTone, _ = dll.FindProc("ime_free_tone")
	b.pImeEnglishAutoRestore, _ = dll.FindProc("ime_english_auto_restore")
	b.pImeAutoCapitalize, _ = dll.FindProc("ime_auto_capitalize")
	b.pImeClearAll, _ = dll.FindProc("ime_clear_all")
	b.pImeGetBuffer, _ = dll.FindProc("ime_get_buffer")
	b.pImeRestoreWord, _ = dll.FindProc("ime_restore_word")
	b.pImeAddShortcut, _ = dll.FindProc("ime_add_shortcut")
	b.pImeRemoveShortcut, _ = dll.FindProc("ime_remove_shortcut")
	b.pImeClearShortcuts, _ = dll.FindProc("ime_clear_shortcuts")

	return b, nil
}

// Close releases DLL resources
func (b *Bridge) Close() error {
	if b.dll != nil {
		return b.dll.Release()
	}
	return nil
}

// boolToUintptr converts bool to uintptr (0 or 1)
func boolToUintptr(v bool) uintptr {
	if v {
		return 1
	}
	return 0
}

// Initialize the IME engine
func (b *Bridge) Initialize() {
	if b.pImeInit != nil {
		b.pImeInit.Call()
	}
}

// Clear the typing buffer
func (b *Bridge) Clear() {
	if b.pImeClear != nil {
		b.pImeClear.Call()
	}
}

// ClearAll clears buffer and resets all state
func (b *Bridge) ClearAll() {
	if b.pImeClearAll != nil {
		b.pImeClearAll.Call()
	}
}

// SetMethod sets input method (Telex=0, VNI=1)
func (b *Bridge) SetMethod(method InputMethod) {
	if b.pImeMethod != nil {
		b.pImeMethod.Call(uintptr(method))
	}
}

// SetEnabled enables or disables IME processing
func (b *Bridge) SetEnabled(enabled bool) {
	if b.pImeEnabled != nil {
		b.pImeEnabled.Call(boolToUintptr(enabled))
	}
}

// SetModernTone sets tone style (modern=true: hòa, old=false: hoà)
func (b *Bridge) SetModernTone(modern bool) {
	if b.pImeModern != nil {
		b.pImeModern.Call(boolToUintptr(modern))
	}
}

// SetSkipWShortcut sets whether to skip w→ư shortcut in Telex mode
func (b *Bridge) SetSkipWShortcut(skip bool) {
	if b.pImeSkipWShortcut != nil {
		b.pImeSkipWShortcut.Call(boolToUintptr(skip))
	}
}

// SetBracketShortcut sets whether bracket shortcuts are enabled: ] → ư, [ → ơ
func (b *Bridge) SetBracketShortcut(enabled bool) {
	if b.pImeBracketShortcut != nil {
		b.pImeBracketShortcut.Call(boolToUintptr(enabled))
	}
}

// SetEscRestore sets whether ESC key restores raw ASCII input
func (b *Bridge) SetEscRestore(enabled bool) {
	if b.pImeEscRestore != nil {
		b.pImeEscRestore.Call(boolToUintptr(enabled))
	}
}

// SetFreeTone sets whether to enable free tone placement
func (b *Bridge) SetFreeTone(enabled bool) {
	if b.pImeFreeTone != nil {
		b.pImeFreeTone.Call(boolToUintptr(enabled))
	}
}

// SetEnglishAutoRestore sets whether to auto-restore English words
func (b *Bridge) SetEnglishAutoRestore(enabled bool) {
	if b.pImeEnglishAutoRestore != nil {
		b.pImeEnglishAutoRestore.Call(boolToUintptr(enabled))
	}
}

// SetAutoCapitalize sets whether to auto-capitalize after sentence-ending punctuation
func (b *Bridge) SetAutoCapitalize(enabled bool) {
	if b.pImeAutoCapitalize != nil {
		b.pImeAutoCapitalize.Call(boolToUintptr(enabled))
	}
}

// ProcessKey processes a keystroke and returns the result
// keycode: macOS keycode (translated from Windows VK)
func (b *Bridge) ProcessKey(keycode uint16, capslock, ctrl, shift bool) ImeResult {
	if b.pImeKeyExt == nil {
		return ImeResult{Action: ActionNone}
	}

	ptr, _, _ := b.pImeKeyExt.Call(
		uintptr(keycode),
		boolToUintptr(capslock),
		boolToUintptr(ctrl),
		boolToUintptr(shift),
	)

	if ptr == 0 {
		return ImeResult{Action: ActionNone}
	}

	defer b.pImeFree.Call(ptr)

	// Cast once to the typed struct matching the Rust FFI layout (see
	// cResult's doc comment) instead of manually parsing a raw byte buffer.
	result := (*cResult)(unsafe.Pointer(ptr))

	chars := make([]rune, 0, result.count)
	for i := uint8(0); i < result.count; i++ {
		if charVal := result.chars[i]; charVal > 0 {
			chars = append(chars, rune(charVal))
		}
	}

	return ImeResult{
		Action:    ImeAction(result.action),
		Backspace: result.backspace,
		Count:     result.count,
		Chars:     chars,
	}
}

// AddShortcut adds a text shortcut
func (b *Bridge) AddShortcut(trigger, replacement string) {
	if b.pImeAddShortcut == nil {
		return
	}

	triggerBytes := append([]byte(trigger), 0)
	replacementBytes := append([]byte(replacement), 0)

	b.pImeAddShortcut.Call(
		uintptr(unsafe.Pointer(&triggerBytes[0])),
		uintptr(unsafe.Pointer(&replacementBytes[0])),
	)
}

// RemoveShortcut removes a shortcut
func (b *Bridge) RemoveShortcut(trigger string) {
	if b.pImeRemoveShortcut == nil {
		return
	}

	triggerBytes := append([]byte(trigger), 0)
	b.pImeRemoveShortcut.Call(uintptr(unsafe.Pointer(&triggerBytes[0])))
}

// ClearShortcuts removes all shortcuts
func (b *Bridge) ClearShortcuts() {
	if b.pImeClearShortcuts != nil {
		b.pImeClearShortcuts.Call()
	}
}

// RestoreWord restores a word to the buffer for continued editing
func (b *Bridge) RestoreWord(word string) {
	if b.pImeRestoreWord == nil {
		return
	}

	wordBytes := append([]byte(word), 0)
	b.pImeRestoreWord.Call(uintptr(unsafe.Pointer(&wordBytes[0])))
}

// ===== Keycode Translation (Windows VK -> macOS) =====

// macOS virtual keycodes (from core/src/data/keys.rs)
const (
	MAC_A         = 0x00
	MAC_S         = 0x01
	MAC_D         = 0x02
	MAC_F         = 0x03
	MAC_H         = 0x04
	MAC_G         = 0x05
	MAC_Z         = 0x06
	MAC_X         = 0x07
	MAC_C         = 0x08
	MAC_V         = 0x09
	MAC_B         = 0x0B
	MAC_Q         = 0x0C
	MAC_W         = 0x0D
	MAC_E         = 0x0E
	MAC_R         = 0x0F
	MAC_Y         = 0x10
	MAC_T         = 0x11
	MAC_O         = 0x1F
	MAC_U         = 0x20
	MAC_I         = 0x22
	MAC_P         = 0x23
	MAC_L         = 0x25
	MAC_J         = 0x26
	MAC_K         = 0x28
	MAC_N         = 0x2D
	MAC_M         = 0x2E
	MAC_N1        = 0x12
	MAC_N2        = 0x13
	MAC_N3        = 0x14
	MAC_N4        = 0x15
	MAC_N5        = 0x17
	MAC_N6        = 0x16
	MAC_N7        = 0x1A
	MAC_N8        = 0x1C
	MAC_N9        = 0x19
	MAC_N0        = 0x1D
	MAC_SPACE     = 0x31
	MAC_DELETE    = 0x33
	MAC_TAB       = 0x30
	MAC_RETURN    = 0x24
	MAC_ESC       = 0x35
	MAC_LBRACKET  = 0x21
	MAC_RBRACKET  = 0x1E
	MAC_DOT       = 0x2F
	MAC_COMMA     = 0x2B
	MAC_SLASH     = 0x2C
	MAC_SEMICOLON = 0x29
	MAC_QUOTE     = 0x27
	MAC_MINUS     = 0x1B
	MAC_EQUAL     = 0x18
)

// macKeycodeUnmapped is returned by TranslateToMacKeycode for any Windows VK
// with no macOS keycode equivalent.
const macKeycodeUnmapped uint16 = 0xFFFF

// vkToMacKeycode is a Windows-VK -> macOS-keycode lookup table, indexed
// directly by VK code (VK codes are single bytes, 0-255). Built once at
// package init instead of a per-call switch statement.
var vkToMacKeycode = buildVKToMacKeycodeTable()

func buildVKToMacKeycodeTable() [256]uint16 {
	var t [256]uint16
	for i := range t {
		t[i] = macKeycodeUnmapped
	}

	// Letters A-Z
	t[0x41] = MAC_A
	t[0x42] = MAC_B
	t[0x43] = MAC_C
	t[0x44] = MAC_D
	t[0x45] = MAC_E
	t[0x46] = MAC_F
	t[0x47] = MAC_G
	t[0x48] = MAC_H
	t[0x49] = MAC_I
	t[0x4A] = MAC_J
	t[0x4B] = MAC_K
	t[0x4C] = MAC_L
	t[0x4D] = MAC_M
	t[0x4E] = MAC_N
	t[0x4F] = MAC_O
	t[0x50] = MAC_P
	t[0x51] = MAC_Q
	t[0x52] = MAC_R
	t[0x53] = MAC_S
	t[0x54] = MAC_T
	t[0x55] = MAC_U
	t[0x56] = MAC_V
	t[0x57] = MAC_W
	t[0x58] = MAC_X
	t[0x59] = MAC_Y
	t[0x5A] = MAC_Z

	// Numbers 0-9
	t[0x30] = MAC_N0
	t[0x31] = MAC_N1
	t[0x32] = MAC_N2
	t[0x33] = MAC_N3
	t[0x34] = MAC_N4
	t[0x35] = MAC_N5
	t[0x36] = MAC_N6
	t[0x37] = MAC_N7
	t[0x38] = MAC_N8
	t[0x39] = MAC_N9

	// Special keys
	t[0x08] = MAC_DELETE   // VK_BACK
	t[0x09] = MAC_TAB      // VK_TAB
	t[0x0D] = MAC_RETURN   // VK_RETURN
	t[0x1B] = MAC_ESC      // VK_ESCAPE
	t[0x20] = MAC_SPACE    // VK_SPACE
	t[0xDB] = MAC_LBRACKET // VK_OEM_4 ([{)
	t[0xDD] = MAC_RBRACKET // VK_OEM_6 (]})

	// Punctuation
	t[0xBE] = MAC_DOT       // VK_OEM_PERIOD
	t[0xBC] = MAC_COMMA     // VK_OEM_COMMA
	t[0xBF] = MAC_SLASH     // VK_OEM_2 (/)
	t[0xBA] = MAC_SEMICOLON // VK_OEM_1 (;)
	t[0xDE] = MAC_QUOTE     // VK_OEM_7 (')
	t[0xBD] = MAC_MINUS     // VK_OEM_MINUS
	t[0xBB] = MAC_EQUAL     // VK_OEM_PLUS (=)

	return t
}

// TranslateToMacKeycode translates Windows Virtual Key code to macOS keycode.
// Returns 0xFFFF if key is not mapped.
func TranslateToMacKeycode(windowsVK uint16) uint16 {
	if windowsVK >= uint16(len(vkToMacKeycode)) {
		return macKeycodeUnmapped
	}
	return vkToMacKeycode[windowsVK]
}
