using System.Collections.Concurrent;
using System.Diagnostics;

namespace GoNhanh.Core;

/// <summary>
/// Readonly struct for keyboard events - lightweight, stack-allocated.
/// Contains all data needed to process a key press.
/// </summary>
public readonly struct KeyEvent
{
    public ushort VirtualKeyCode { get; init; }
    public bool Shift { get; init; }
    public bool CapsLock { get; init; }
    public long Timestamp { get; init; }

    public KeyEvent(ushort vkCode, bool shift, bool capsLock)
    {
        VirtualKeyCode = vkCode;
        Shift = shift;
        CapsLock = capsLock;
        Timestamp = Stopwatch.GetTimestamp();
    }
}

/// <summary>
/// Thread-safe queue for keyboard events.
/// Producer: Hook callback (enqueues in <1ms)
/// Consumer: Worker thread (dequeues and processes)
/// Uses ConcurrentQueue (lock-free) + AutoResetEvent (efficient signaling).
/// </summary>
public sealed class KeyEventQueue : IDisposable
{
    private readonly ConcurrentQueue<KeyEvent> _queue = new();
    private readonly AutoResetEvent _signal = new(false);
    private int _disposed;  // 0 = active, 1 = disposed (Interlocked for thread-safe check)

    /// <summary>
    /// Enqueue a key event. Called from hook callback thread.
    /// Returns immediately (<1μs) - non-blocking.
    /// </summary>
    public void Enqueue(KeyEvent evt)
    {
        if (Volatile.Read(ref _disposed) == 1) return;
        _queue.Enqueue(evt);
        _signal.Set();
    }

    /// <summary>
    /// Try to dequeue a key event. Called from worker thread.
    /// Blocks until item available or timeout expires.
    /// </summary>
    /// <param name="evt">The dequeued event (if successful)</param>
    /// <param name="timeoutMs">Max wait time in milliseconds</param>
    /// <returns>True if event dequeued, false on timeout or disposed</returns>
    public bool TryDequeue(out KeyEvent evt, int timeoutMs = 100)
    {
        evt = default;
        if (Volatile.Read(ref _disposed) == 1) return false;

        // Wait for signal or timeout
        try
        {
            if (!_signal.WaitOne(timeoutMs))
                return false;
        }
        catch (ObjectDisposedException)
        {
            return false;  // Signal disposed during wait
        }

        // Check disposed again after waking up
        if (Volatile.Read(ref _disposed) == 1) return false;

        return _queue.TryDequeue(out evt);
    }

    /// <summary>
    /// Check if queue is empty. For debugging/profiling only.
    /// </summary>
    public bool IsEmpty => _queue.IsEmpty;

    /// <summary>
    /// Get approximate queue count. For debugging/profiling only.
    /// </summary>
    public int Count => _queue.Count;

    public void Dispose()
    {
        // Atomic exchange - ensures single disposal
        if (Interlocked.Exchange(ref _disposed, 1) == 1) return;

        _signal.Set();  // Wake up waiting thread

        // Small delay to allow waiting threads to exit gracefully
        Thread.Sleep(5);

        _signal.Dispose();
    }
}
