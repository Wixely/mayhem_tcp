using System.Runtime.InteropServices;
using System.Text;

namespace MayhemTcp;

internal static class Native
{
    [DllImport("mayhem_android", EntryPoint = "mt_create")]
    internal static extern nint Create(int fd, ushort port, byte lan, byte analog, byte digital, uint queueBlocks);
    [DllImport("mayhem_android", EntryPoint = "mt_run")]
    internal static extern int Run(nint handle);
    [DllImport("mayhem_android", EntryPoint = "mt_stop")]
    internal static extern void Stop(nint handle);
    [DllImport("mayhem_android", EntryPoint = "mt_free")]
    internal static extern void Free(nint handle);
    [DllImport("mayhem_android", EntryPoint = "mt_log")]
    private static extern nuint ReadLog([Out] byte[] buffer, nuint capacity);

    internal static void DrainLogs()
    {
        var buffer = new byte[8192];
        for (var i = 0; i < 128; i++)
        {
            var length = ReadLog(buffer, (nuint)buffer.Length);
            if (length == 0) break;
            ServerState.Log(Encoding.UTF8.GetString(buffer, 0, (int)length));
        }
    }
}

internal static class ServerState
{
    private static readonly object Gate = new();
    private static readonly Queue<string> Lines = new();
    internal static volatile bool Active;
    internal static volatile string Status = "Stopped";
    internal static void Log(string message)
    {
        if (Active)
        {
            if (message.StartsWith("mayhem_tcp listening")) Status = message;
            else if (message.StartsWith("Configured frequency=")) Status = "Streaming to rtl_tcp client";
            else if (message.StartsWith("Session closed") || message.StartsWith("Session ended:")) Status = "Listening — ready for another client";
        }
        lock (Gate)
        {
            if (Lines.Count >= 160) Lines.Dequeue();
            Lines.Enqueue($"{DateTime.Now:HH:mm:ss} {message}");
        }
        global::Android.Util.Log.Info("mayhem_tcp", message);
    }
    internal static string LogText() { lock (Gate) return string.Join('\n', Lines); }
}
