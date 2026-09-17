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
    private CheckBox lan = null!, analog = null!, digital = null!;
    private TextView status = null!, addresses = null!, logs = null!;
    private Button start = null!;
    private PermissionReceiver? permissionReceiver;
    private UsbDevice? pendingDevice;
    private bool pending;
    private ushort pendingPort;
    private bool pendingLan, pendingAnalog, pendingDigital;
    private System.Threading.Timer? timer;

    protected override void OnCreate(Bundle? savedInstanceState)
    {
        base.OnCreate(savedInstanceState);
        Window?.SetSoftInputMode(SoftInput.AdjustResize);
        var root = new LinearLayout(this) { Orientation = Orientation.Vertical };
        var density = Resources!.DisplayMetrics!.Density;
        root.SetPadding((int)(20 * density), (int)(36 * density), (int)(20 * density), (int)(24 * density));
        var heading = new TextView(this) { Text = "mayhem_tcp", TextSize = 28 };
        root.AddView(heading);
        root.AddView(new TextView(this) { Text = "HackRF → Android → rtl_tcp\nUSB receive server · Android test build", TextSize = 15 });
        root.AddView(new TextView(this) { Text = "TCP port", TextSize = 16 });
        port = new EditText(this) { Text = "12346", InputType = InputTypes.ClassNumber, ContentDescription = "TCP port" };
        root.AddView(port);
        lan = new CheckBox(this) { Text = "Listen on all interfaces (LAN / WireGuard)", Checked = true };
        analog = new CheckBox(this) { Text = "Start with analog AGC", Checked = true };
        digital = new CheckBox(this) { Text = "Start with digital AGC", Checked = true };
        var preferences = GetSharedPreferences("server", FileCreationMode.Private)!;
        port.Text = preferences.GetString("port", "12346");
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
        status = new TextView(this) { TextSize = 17 };
        addresses = new TextView(this) { TextSize = 13 };
        root.AddView(status); root.AddView(addresses);
        root.AddView(new TextView(this) { Text = "Connect using RTL-TCP and the phone's reachable IP. Over WireGuard use its tunnel IP. HackRF must be in HackRF mode. Client commands can override AGC.", TextSize = 13 });
        var scroll = new ScrollView(this);
        logs = new TextView(this) { TextSize = 12 };
        logs.SetTextIsSelectable(true);
        scroll.AddView(logs);
        root.AddView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MatchParent, 0, 1));
        SetContentView(root);
        permissionReceiver = new PermissionReceiver(this);
        if (OperatingSystem.IsAndroidVersionAtLeast(33)) RegisterReceiver(permissionReceiver, new IntentFilter(UsbPermission), ReceiverFlags.NotExported);
        else RegisterReceiver(permissionReceiver, new IntentFilter(UsbPermission));
        if (OperatingSystem.IsAndroidVersionAtLeast(33) && CheckSelfPermission(Manifest.Permission.PostNotifications) != Permission.Granted)
            RequestPermissions([Manifest.Permission.PostNotifications], 2);
    }

    private void RequestStart()
    {
        if (ServerState.Active || pending) return;
        if (!ushort.TryParse(port.Text, out pendingPort) || pendingPort == 0)
        { ServerState.Log("Choose a TCP port from 1 to 65535"); return; }
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
        port.Enabled = lan.Enabled = analog.Enabled = digital.Enabled = !ServerState.Active && !pending;
        SetText(logs, ServerState.LogText());
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
