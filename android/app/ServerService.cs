using Android.App;
using Android.Content;
using Android.Content.PM;
using Android.Hardware.Usb;
using Android.OS;

namespace MayhemTcp;

[Service(Exported = false, ForegroundServiceType = ForegroundService.TypeConnectedDevice)]
public sealed class ServerService : Service
{
    internal const string StopAction = "com.wixely.mayhemtcp.STOP";
    private const string ChannelId = "mayhem_tcp_server";
    private readonly object gate = new();
    private nint handle;
    private PowerManager.WakeLock? wakeLock;
    private DetachReceiver? detached;
    private string? deviceName;

    public override IBinder? OnBind(Intent? intent) => null;

    public override void OnCreate()
    {
        base.OnCreate();
        var notifications = (NotificationManager)GetSystemService(NotificationService)!;
        notifications.CreateNotificationChannel(new NotificationChannel(ChannelId, "Radio server", NotificationImportance.Low));
        detached = new DetachReceiver(this);
        var filter = new IntentFilter(UsbManager.ActionUsbDeviceDetached);
        if (OperatingSystem.IsAndroidVersionAtLeast(33)) RegisterReceiver(detached, filter, ReceiverFlags.NotExported);
        else RegisterReceiver(detached, filter);
    }

    public override StartCommandResult OnStartCommand(Intent? intent, StartCommandFlags flags, int startId)
    {
        if (intent?.Action == StopAction)
        {
            RequestStop();
            if (!ServerState.Active) StopSelf(startId);
            return StartCommandResult.NotSticky;
        }
        if (ServerState.Active) return StartCommandResult.NotSticky;
        UsbDeviceConnection? connection = null;
        try
        {
            var port = intent?.GetIntExtra("port", 12346) ?? 12346;
            var lan = intent?.GetBooleanExtra("lan", true) ?? true;
            deviceName = intent?.GetStringExtra("device");
            var manager = (UsbManager)GetSystemService(UsbService)!;
            if (deviceName == null || !manager.DeviceList!.TryGetValue(deviceName, out var device) || !manager.HasPermission(device))
                throw new InvalidOperationException("HackRF disconnected or USB permission is missing. Open the app and start again.");
            var open = PendingIntent.GetActivity(this, 0, new Intent(this, typeof(MainActivity)), PendingIntentFlags.Immutable | PendingIntentFlags.UpdateCurrent);
            var stop = PendingIntent.GetService(this, 1, new Intent(this, typeof(ServerService)).SetAction(StopAction), PendingIntentFlags.Immutable | PendingIntentFlags.UpdateCurrent);
            var notification = new Notification.Builder(this, ChannelId)
                .SetContentTitle("mayhem_tcp running")
                .SetContentText($"{(lan ? "All interfaces" : "Localhost")} · TCP {port} · tap to view")
                .SetSmallIcon(global::Android.Resource.Drawable.IcMenuShare)
                .SetContentIntent(open).SetOngoing(true)
                .AddAction(new Notification.Action.Builder(null, "Stop", stop).Build())
                .Build();
            if (OperatingSystem.IsAndroidVersionAtLeast(29)) StartForeground(12346, notification, ForegroundService.TypeConnectedDevice);
            else StartForeground(12346, notification);
            connection = manager.OpenDevice(device) ?? throw new InvalidOperationException("Could not open HackRF USB connection");
            lock (gate)
            {
                handle = Native.Create(connection.FileDescriptor, checked((ushort)port), (byte)(lan ? 1 : 0),
                    (byte)((intent?.GetBooleanExtra("analog", true) ?? true) ? 1 : 0),
                    (byte)((intent?.GetBooleanExtra("digital", true) ?? true) ? 1 : 0),
                    checked((uint)(intent?.GetIntExtra("queueBlocks", 32) ?? 32)));
                if (handle == 0) throw new InvalidOperationException("Could not create native server; see log");
            }
            var power = (PowerManager)GetSystemService(PowerService)!;
            wakeLock = power.NewWakeLock(WakeLockFlags.Partial, "mayhem_tcp:usb_rx");
            wakeLock!.SetReferenceCounted(false);
            wakeLock.Acquire();
            ServerState.Active = true;
            ServerState.Status = $"Starting on {(lan ? "0.0.0.0" : "127.0.0.1")}:{port}";
            ServerState.Log(ServerState.Status);
            var ownedConnection = connection;
            connection = null; // Worker owns connection lifetime through native shutdown.
            _ = Task.Run(() => RunServer(ownedConnection, startId));
        }
        catch (Exception error)
        {
            lock (gate) { if (handle != 0) { Native.Free(handle); handle = 0; } }
            connection?.Close();
            if (wakeLock?.IsHeld == true) wakeLock.Release();
            ServerState.Status = "Start failed";
            ServerState.Log(error.Message);
            StopForeground(StopForegroundFlags.Remove);
            StopSelf(startId);
        }
        return StartCommandResult.NotSticky;
    }

    private void RunServer(UsbDeviceConnection connection, int startId)
    {
        var failed = false;
        try { failed = Native.Run(handle) != 0; }
        catch (Exception error) { failed = true; ServerState.Log(error.Message); }
        finally
        {
            lock (gate) { Native.Free(handle); handle = 0; }
            connection.Close();
            if (wakeLock?.IsHeld == true) wakeLock.Release();
            new Handler(Looper.MainLooper!).Post(() =>
            {
                ServerState.Status = failed ? "Server stopped with an error — see log" : "Stopped";
                StopForeground(StopForegroundFlags.Remove);
                ServerState.Active = false;
                StopSelf(startId);
            });
        }
    }

    private void RequestStop()
    {
        lock (gate) { if (handle != 0) Native.Stop(handle); }
        if (ServerState.Active) ServerState.Status = "Stopping…";
    }

    public override void OnDestroy()
    {
        RequestStop();
        if (detached != null) UnregisterReceiver(detached);
        base.OnDestroy();
    }

    private sealed class DetachReceiver(ServerService owner) : BroadcastReceiver
    {
        public override void OnReceive(Context? context, Intent? intent)
        {
            var manager = (UsbManager)owner.GetSystemService(UsbService)!;
            if (owner.deviceName != null && !manager.DeviceList!.ContainsKey(owner.deviceName))
            {
                ServerState.Log("HackRF unplugged; stopping server");
                owner.RequestStop();
            }
        }
    }
}
