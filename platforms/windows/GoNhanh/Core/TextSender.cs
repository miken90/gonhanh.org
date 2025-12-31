using System.Runtime.InteropServices;

namespace GoNhanh.Core;

/// <summary>
/// Sends text to the active window using Unicode injection via SendInput API.
/// Supports multiple injection methods for different app compatibility.
/// Uses KEYEVENTF_UNICODE flag to inject characters directly as keyboard events.
/// This preserves clipboard content and maintains uppercase state correctly.
/// </summary>
public static class TextSender
{
    #region Win32 Constants

    private const uint INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
    private const uint KEYEVENTF_UNICODE = 0x0004;

    // Delay settings (ms) for slow mode - optimized for low latency
    private const int SlowModeKeyDelay = 1;     // Delay between chars (was 5)
    private const int SlowModePreDelay = 5;     // Delay before text (was 20)
    private const int SlowModePostDelay = 3;    // Delay after backspaces (was 15)
    private const int FastModeDelay = 2;        // Delay between backspace and text (was 10)

    #endregion

    #region Win32 Imports

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    #endregion

    #region Structures

    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT
    {
        public uint type;
        public INPUTUNION u;
    }

    [StructLayout(LayoutKind.Explicit, Size = 32)]
    private struct INPUTUNION
    {
        [FieldOffset(0)] public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    #endregion

    /// <summary>
    /// Send text replacement using auto-detected injection method.
    /// Detects foreground app and applies appropriate method.
    /// </summary>
    public static void SendText(string text, int backspaces)
    {
        var method = AppDetector.GetMethod();
        SendText(text, backspaces, method);
    }

    /// <summary>
    /// Send text replacement with specified injection method.
    /// </summary>
    public static void SendText(string text, int backspaces, InjectionMethod method)
    {
        if (string.IsNullOrEmpty(text) && backspaces == 0)
            return;

        var marker = KeyboardHook.GetInjectedKeyMarker();

        switch (method)
        {
            case InjectionMethod.Fast:
                SendFast(text, backspaces, marker);
                break;

            case InjectionMethod.Slow:
                SendSlow(text, backspaces, marker);
                break;
        }
    }

    /// <summary>
    /// Fast mode: batch backspaces and text in single SendInput calls.
    /// Best for standard apps (Notepad, Word, etc.)
    /// </summary>
    private static void SendFast(string text, int backspaces, IntPtr marker)
    {
        if (backspaces > 0)
        {
            SendBackspaces(backspaces, marker);
            Thread.Sleep(FastModeDelay);
        }

        if (!string.IsNullOrEmpty(text))
        {
            SendUnicodeTextBatch(text, marker);
        }
    }

    /// <summary>
    /// Slow mode: add delays between backspaces and text.
    /// Best for Electron apps, terminals, and browsers.
    /// </summary>
    private static void SendSlow(string text, int backspaces, IntPtr marker)
    {
        if (backspaces > 0)
        {
            SendBackspaces(backspaces, marker);
            Thread.Sleep(SlowModePostDelay);
        }

        if (!string.IsNullOrEmpty(text))
        {
            Thread.Sleep(SlowModePreDelay);
            SendUnicodeTextSlow(text, marker, SlowModeKeyDelay);
        }
    }

    /// <summary>
    /// Send multiple backspaces in a single batch
    /// </summary>
    private static void SendBackspaces(int count, IntPtr marker)
    {
        var inputs = new INPUT[count * 2];

        for (int i = 0; i < count; i++)
        {
            // Key down
            inputs[i * 2] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = KeyCodes.VK_BACK,
                        dwFlags = 0,
                        dwExtraInfo = marker
                    }
                }
            };

            // Key up
            inputs[i * 2 + 1] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = KeyCodes.VK_BACK,
                        dwFlags = KEYEVENTF_KEYUP,
                        dwExtraInfo = marker
                    }
                }
            };
        }

        SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
    }

    /// <summary>
    /// Send text using Unicode input - batched for fast mode.
    /// Injects all characters in a single SendInput call.
    /// </summary>
    private static void SendUnicodeTextBatch(string text, IntPtr marker)
    {
        var inputs = new INPUT[text.Length * 2];
        int idx = 0;

        foreach (char c in text)
        {
            // Key down
            inputs[idx++] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = 0,
                        wScan = c,
                        dwFlags = KEYEVENTF_UNICODE,
                        dwExtraInfo = marker
                    }
                }
            };

            // Key up
            inputs[idx++] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = 0,
                        wScan = c,
                        dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        dwExtraInfo = marker
                    }
                }
            };
        }

        SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
    }

    /// <summary>
    /// Send text using Unicode input - with delay between characters.
    /// Better compatibility for slow apps (Electron, browsers, terminals).
    /// </summary>
    private static void SendUnicodeTextSlow(string text, IntPtr marker, int delayMs)
    {
        foreach (char c in text)
        {
            var inputs = new INPUT[2];

            // Key down
            inputs[0] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = 0,
                        wScan = c,
                        dwFlags = KEYEVENTF_UNICODE,
                        dwExtraInfo = marker
                    }
                }
            };

            // Key up
            inputs[1] = new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = 0,
                        wScan = c,
                        dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        dwExtraInfo = marker
                    }
                }
            };

            SendInput(2, inputs, Marshal.SizeOf<INPUT>());

            if (delayMs > 0)
                Thread.Sleep(delayMs);
        }
    }

    /// <summary>
    /// Send a single key press (for passthrough when no transformation).
    /// Used by async worker when Rust core returns ImeAction.None.
    /// </summary>
    public static void SendKey(ushort vkCode, bool shift)
    {
        var marker = KeyboardHook.GetInjectedKeyMarker();
        var inputs = new List<INPUT>();

        // If shift needed and not already pressed, add shift down
        if (shift)
        {
            inputs.Add(new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = KeyCodes.VK_SHIFT,
                        dwFlags = 0,
                        dwExtraInfo = marker
                    }
                }
            });
        }

        // Key down
        inputs.Add(new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = vkCode,
                    dwFlags = 0,
                    dwExtraInfo = marker
                }
            }
        });

        // Key up
        inputs.Add(new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = vkCode,
                    dwFlags = KEYEVENTF_KEYUP,
                    dwExtraInfo = marker
                }
            }
        });

        // Release shift if we pressed it
        if (shift)
        {
            inputs.Add(new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new INPUTUNION
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = KeyCodes.VK_SHIFT,
                        dwFlags = KEYEVENTF_KEYUP,
                        dwExtraInfo = marker
                    }
                }
            });
        }

        SendInput((uint)inputs.Count, inputs.ToArray(), Marshal.SizeOf<INPUT>());
    }
}
