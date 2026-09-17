using Android;
using Android.App;
using Android.Content;
using Android.Content.PM;
using Android.Hardware.Usb;
using Android.OS;
using Android.Text;
using Android.Views;
using Android.Widget;
using System.Net.NetworkInformation;
using System.Net.Sockets;

namespace MayhemTcp;

[Activity(Label = "mayhem_tcp", MainLauncher = true, Exported = true)]
public sealed class MainActivity : Activity
{
    private const string UsbPermission = "com.wixely.mayhemtcp.USB_PERMISSION";
    private EditText port = null!;
    private EditText queueBlocks = null!;
    private CheckBox lan = null!, analog = null!, digital = null!;
    private TextView status = null!, addresses = null!, logs = null!;
    private ScrollView logScroll = null!;
    private Button start = null!, advanced = null!;
    private PermissionReceiver? permissionReceiver;
    private UsbDevice? pendingDevice;
    private bool pending;
    private ushort pendingPort;
    private int pendingQueueBlocks;
    private bool pendingLan, pendingAnalog, pendingDigital;
    private System.Threading.Timer? timer;

    protected override void OnCreate(Bundle? savedInstanceState)
    {
        base.OnCreate(savedInstanceState);
        Window?.SetSoftInputMode(SoftInput.AdjustResize);
        var root = new LinearLayout(this) { Orientation = Orientation.Vertical };
        var density = Resources!.DisplayMetrics!.Density;
        root.SetPadding((int)(20 * density), (int)(36 * density), (int)(20 * density), (int)(24 * density));
        if (OperatingSystem.IsAndroidVersionAtLeast(30))
            root.SetOnApplyWindowInsetsListener(new ContentInsets(density));
        var heading = new TextView(this) { Text = "mayhem_tcp", TextSize = 28 };
        root.AddView(heading);
        root.AddView(new TextView(this) { Text = "HackRF → Android → rtl_tcp\nUSB receive server · Android test build", TextSize = 15 });
        var connectionFields = new LinearLayout(this) { Orientation = Orientation.Horizontal };
        var portField = new LinearLayout(this) { Orientation = Orientation.Vertical };
        var queueField = new LinearLayout(this) { Orientation = Orientation.Vertical };
        portField.AddView(new TextView(this) { Text = "TCP port", TextSize = 16 });
        port = new EditText(this) { Id = Resource.Id.port, Text = "12346", InputType = InputTypes.ClassNumber, ContentDescription = "TCP port" };
        portField.AddView(port);
        queueField.AddView(new TextView(this) { Text = "Buffer blocks (1–1024)", TextSize = 16 });
        queueBlocks = new EditText(this) { Id = Resource.Id.queue_blocks, Text = "32", InputType = InputTypes.ClassNumber, ContentDescription = "Output buffer blocks" };
        queueField.AddView(queueBlocks);
        connectionFields.AddView(portField, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        connectionFields.AddView(queueField, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        root.AddView(connectionFields);
        lan = new CheckBox(this) { Id = Resource.Id.listen_lan, Text = "Listen on all interfaces (LAN / WireGuard)", Checked = true };
        analog = new CheckBox(this) { Id = Resource.Id.analog_agc, Text = "Start with analog AGC", Checked = true };
        digital = new CheckBox(this) { Id = Resource.Id.digital_agc, Text = "Start with digital AGC", Checked = true };
        var preferences = GetSharedPreferences("server", FileCreationMode.Private)!;
        port.Text = preferences.GetString("port", "12346");
        queueBlocks.Text = preferences.GetInt("queueBlocks", 32).ToString();
        lan.Checked = preferences.GetBoolean("lan", true);
        analog.Checked = preferences.GetBoolean("analog", true);
        digital.Checked = preferences.GetBoolean("digital", true);
        root.AddView(lan); root.AddView(analog); root.AddView(digital);
        var buttons = new LinearLayout(this) { Orientation = Orientation.Horizontal };
        start = new Button(this) { Text = "Start server" };
        var stop = new Button(this) { Text = "Stop" };
        start.Click += (_, _) => RequestStart();
        stop.Click += (_, _) =>
        {
            pending = false;
            pendingDevice = null;
            if (ServerState.Active) StartService(new Intent(this, typeof(ServerService)).SetAction(ServerService.StopAction));
            else ServerState.Status = "Stopped";
        };
        buttons.AddView(start, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        buttons.AddView(stop, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        root.AddView(buttons);
        var helpButtons = new LinearLayout(this) { Orientation = Orientation.Horizontal };
        var battery = new Button(this) { Text = "Battery" };
        var about = new Button(this) { Text = "About" };
        advanced = new Button(this) { Text = "Radio" };
        advanced.Click += (_, _) => StartupOptions.Show(this);
        battery.Click += (_, _) => new AlertDialog.Builder(this)
            .SetTitle("Background streaming")!
            .SetMessage("In Android's battery optimization settings, select All apps if needed, find mayhem_tcp, and choose Don't optimize or Unrestricted. Labels vary by phone. This may help background streaming and increases battery use; it does not guarantee uninterrupted operation.")!
            .SetPositiveButton("Open settings", (_, _) => OpenBatterySettings())!
            .SetNegativeButton("Cancel", (_, _) => { })!.Show();
        about.Click += (_, _) => new AlertDialog.Builder(this)
            .SetTitle("About mayhem_tcp")!
            .SetMessage("A receive-only HackRF USB server for existing rtl_tcp clients.\n\nExperimental Android build.\n\nSource, documentation and issue reports:\nhttps://github.com/Wixely/mayhem_tcp")!
            .SetPositiveButton("Open GitHub", (_, _) => OpenExternal(new Intent(Intent.ActionView,
                global::Android.Net.Uri.Parse("https://github.com/Wixely/mayhem_tcp"))))!
            .SetNegativeButton("Close", (_, _) => { })!.Show();
        helpButtons.AddView(battery, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        helpButtons.AddView(about, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        helpButtons.AddView(advanced, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WrapContent, 1));
        root.AddView(helpButtons);
        status = new TextView(this) { TextSize = 17 };
        addresses = new TextView(this) { TextSize = 13 };
        root.AddView(status); root.AddView(addresses);
        root.AddView(new TextView(this) { Text = "Connect using RTL-TCP and the phone's reachable IP. Over WireGuard use its tunnel IP. HackRF must be in HackRF mode. Client commands can override AGC.", TextSize = 13 });
        logScroll = new ScrollView(this);
        logs = new TextView(this) { TextSize = 12 };
        logs.SetTextIsSelectable(true);
        logScroll.AddView(logs);
        root.AddView(logScroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MatchParent, 0, 1));
        SetContentView(root);
        root.RequestApplyInsets();
        permissionReceiver = new PermissionReceiver(this);
        if (OperatingSystem.IsAndroidVersionAtLeast(33)) RegisterReceiver(permissionReceiver, new IntentFilter(UsbPermission), ReceiverFlags.NotExported);
        else RegisterReceiver(permissionReceiver, new IntentFilter(UsbPermission));
        if (OperatingSystem.IsAndroidVersionAtLeast(33) && CheckSelfPermission(Manifest.Permission.PostNotifications) != Permission.Granted)
            RequestPermissions([Manifest.Permission.PostNotifications], 2);
    }

    private void OpenBatterySettings()
    {
        try { StartActivity(new Intent(global::Android.Provider.Settings.ActionIgnoreBatteryOptimizationSettings)); }
        catch (ActivityNotFoundException)
        {
            OpenExternal(new Intent(global::Android.Provider.Settings.ActionApplicationDetailsSettings,
                global::Android.Net.Uri.Parse($"package:{PackageName}")));
        }
        catch (Exception error) { ServerState.Log($"Could not open battery settings: {error.Message}"); }
    }

    private void OpenExternal(Intent intent)
    {
        try { StartActivity(intent); }
        catch (Exception error)
        {
            new AlertDialog.Builder(this).SetTitle("Could not open")!
                .SetMessage("Open Android Settings or visit https://github.com/Wixely/mayhem_tcp manually.")!
                .SetPositiveButton("OK", (_, _) => { })!.Show();
            ServerState.Log($"Could not open external activity: {error.Message}");
        }
    }

    private void RequestStart()
    {
        if (ServerState.Active || pending) return;
        if (!ushort.TryParse(port.Text, out pendingPort) || pendingPort == 0)
        { ServerState.Log("Choose a TCP port from 1 to 65535"); return; }
        if (!int.TryParse(queueBlocks.Text, out pendingQueueBlocks) || pendingQueueBlocks < 1 || pendingQueueBlocks > 1024)
        { ServerState.Log("Choose an output buffer count from 1 to 1024 (default 32)"); return; }
        var manager = (UsbManager)GetSystemService(UsbService)!;
        var devices = manager.DeviceList!.Values.Where(d => d.VendorId == 0x1d50 && d.ProductId == 0x6089).ToArray();
        if (devices.Length != 1)
        {
            ServerState.Status = "HackRF not ready";
            ServerState.Log($"Expected one HackRF in HackRF mode; found {devices.Length}. Connect USB/OTG and select HackRF mode on the PortaPack.");
            return;
        }
        pendingDevice = devices[0];
        pendingLan = lan.Checked; pendingAnalog = analog.Checked; pendingDigital = digital.Checked;
        GetSharedPreferences("server", FileCreationMode.Private)!.Edit()!
            .PutInt("queueBlocks", pendingQueueBlocks)!
            .PutString("port", port.Text)!.PutBoolean("lan", pendingLan)!
            .PutBoolean("analog", pendingAnalog)!.PutBoolean("digital", pendingDigital)!.Apply();
        pending = true;
        if (manager.HasPermission(pendingDevice)) BeginService();
        else
        {
            ServerState.Status = "Waiting for USB permission…";
            var intent = new Intent(UsbPermission).SetPackage(PackageName);
            var permission = PendingIntent.GetBroadcast(this, 0, intent, PendingIntentFlags.Immutable | PendingIntentFlags.UpdateCurrent);
            manager.RequestPermission(pendingDevice, permission);
        }
    }

    private void BeginService()
    {
        if (!pending || pendingDevice == null) return;
        pending = false;
        var intent = new Intent(this, typeof(ServerService))
            .PutExtra("device", pendingDevice.DeviceName).PutExtra("port", (int)pendingPort)
            .PutExtra("queueBlocks", pendingQueueBlocks)
            .PutExtra("lan", pendingLan).PutExtra("analog", pendingAnalog).PutExtra("digital", pendingDigital);
        try { StartForegroundService(intent); }
        catch (Exception error) { ServerState.Log(error.Message); ServerState.Status = "Start failed"; }
        pendingDevice = null;
    }

    protected override void OnResume()
    {
        base.OnResume();
        timer = new System.Threading.Timer(_ => RunOnUiThread(Refresh), null, 0, 750);
    }
    protected override void OnPause() { timer?.Dispose(); timer = null; base.OnPause(); }
    protected override void OnDestroy()
    {
        timer?.Dispose();
        if (permissionReceiver != null) UnregisterReceiver(permissionReceiver);
        base.OnDestroy();
    }

    private void Refresh()
    {
        if (IsDestroyed || IsFinishing) return;
        try { Native.DrainLogs(); }
        catch (Exception error) { ServerState.Log($"Native library error: {error.Message}"); }
        SetText(status, pending ? "Waiting for USB permission…" : ServerState.Status);
        start.Enabled = !ServerState.Active && !pending;
        advanced.Enabled = !ServerState.Active && !pending;
        port.Enabled = queueBlocks.Enabled = lan.Enabled = analog.Enabled = digital.Enabled = !ServerState.Active && !pending;
        var logText = ServerState.LogText();
        if (logs.Text != logText)
        {
            // Check the old content before layout grows; leave readers who scrolled up alone.
            var follow = !logScroll.CanScrollVertically(1);
            logs.Text = logText;
            if (follow)
                logScroll.Post(() =>
                {
                    if (!IsDestroyed && !IsFinishing)
                        logScroll.ScrollTo(0, Math.Max(0, logs.Bottom - logScroll.Height));
                });
        }
        try
        {
            var entries = NetworkInterface.GetAllNetworkInterfaces()
                .SelectMany(n => n.GetIPProperties().UnicastAddresses
                    .Where(a => a.Address.AddressFamily == AddressFamily.InterNetwork && !System.Net.IPAddress.IsLoopback(a.Address))
                    .Select(a => $"{n.Name}: {a.Address}"));
            SetText(addresses, "IPv4 addresses: " + string.Join(" · ", entries));
        }
        catch { SetText(addresses, "Use your WireGuard IP from the WireGuard app."); }
    }

    private static void SetText(TextView view, string text)
    {
        if (view.Text != text) view.Text = text;
    }

    private sealed class ContentInsets(float density) : Java.Lang.Object, View.IOnApplyWindowInsetsListener
    {
        public WindowInsets OnApplyWindowInsets(View? view, WindowInsets? insets)
        {
            if (view != null && insets != null && OperatingSystem.IsAndroidVersionAtLeast(30))
            {
                var safe = insets.GetInsets(WindowInsets.Type.SystemBars() | WindowInsets.Type.DisplayCutout() | WindowInsets.Type.Ime());
                // Always calculate from fixed spacing, so repeated dispatches cannot accumulate padding.
                view.SetPadding((int)(20 * density) + safe.Left,
                    Math.Max((int)(36 * density), safe.Top + (int)(12 * density)),
                    (int)(20 * density) + safe.Right, (int)(24 * density) + safe.Bottom);
            }
            return insets!;
        }
    }

    private sealed class PermissionReceiver(MainActivity owner) : BroadcastReceiver
    {
        public override void OnReceive(Context? context, Intent? intent)
        {
            if (!owner.pending || owner.pendingDevice == null) return;
            var manager = (UsbManager)owner.GetSystemService(UsbService)!;
            // Recheck actual permission; never trust broadcast extras.
            if (manager.HasPermission(owner.pendingDevice)) owner.BeginService();
            else
            {
                owner.pending = false; owner.pendingDevice = null;
                ServerState.Status = "USB permission denied";
                ServerState.Log("USB access was not granted. Tap Start to retry.");
            }
        }
    }
}
