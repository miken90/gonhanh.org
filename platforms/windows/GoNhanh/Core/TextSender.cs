using System.Runtime.InteropServices;
using Clipboard = System.Windows.Clipboard;

namespace GoNhanh.Core;

/// <summary>
/// Sends text to the active window using clipboard-based injection
/// Uses Ctrl+V paste for better terminal compatibility (xterm.js, etc.)
/// </summary>
public static class TextSender
{
    #region Win32 Constants

    private const uint INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
    private const ushort VK_CONTROL = 0x11;
    private const ushort VK_V = 0x56;

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
    /// Send text replacement using clipboard-based injection
    /// This works better with terminal emulators like xterm.js
    /// </summary>
    public static void SendText(string text, int backspaces)
    {
        if (string.IsNullOrEmpty(text) && backspaces == 0)
        {
            return;
        }

        var marker = KeyboardHook.GetInjectedKeyMarker();

        // Send all backspaces in one batch for better performance
        if (backspaces > 0)
        {
            SendBackspaces(backspaces, marker);
            Thread.Sleep(2); // Minimal delay for terminal to process
        }

        // Use clipboard-based paste for text insertion
        if (!string.IsNullOrEmpty(text))
        {
            SendViaClipboard(text, marker);
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
    /// Send text via clipboard + Ctrl+V
    /// </summary>
    private static void SendViaClipboard(string text, IntPtr marker)
    {
        try
        {
            // Set our text to clipboard
            Clipboard.SetText(text);

            // Send Ctrl+V immediately (clipboard is synchronous)
            SendCtrlV(marker);
        }
        catch
        {
            // Fallback to direct Unicode if clipboard fails
            SendUnicodeText(text, marker);
        }
    }

    /// <summary>
    /// Send Ctrl+V keystroke
    /// </summary>
    private static void SendCtrlV(IntPtr marker)
    {
        var inputs = new INPUT[4];

        // Ctrl down
        inputs[0] = new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = VK_CONTROL,
                    dwFlags = 0,
                    dwExtraInfo = marker
                }
            }
        };

        // V down
        inputs[1] = new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = VK_V,
                    dwFlags = 0,
                    dwExtraInfo = marker
                }
            }
        };

        // V up
        inputs[2] = new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = VK_V,
                    dwFlags = KEYEVENTF_KEYUP,
                    dwExtraInfo = marker
                }
            }
        };

        // Ctrl up
        inputs[3] = new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new INPUTUNION
            {
                ki = new KEYBDINPUT
                {
                    wVk = VK_CONTROL,
                    dwFlags = KEYEVENTF_KEYUP,
                    dwExtraInfo = marker
                }
            }
        };

        SendInput(4, inputs, Marshal.SizeOf<INPUT>());
    }

    /// <summary>
    /// Fallback: Send text using Unicode input (for non-terminal apps)
    /// </summary>
    private static void SendUnicodeText(string text, IntPtr marker)
    {
        const uint KEYEVENTF_UNICODE = 0x0004;

        foreach (char c in text)
        {
            var inputs = new INPUT[2];

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
        }
    }
}
