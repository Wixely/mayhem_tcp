using Android.App;
using Android.Content;
using Android.Text;
using Android.Widget;
using System.Globalization;
using System.Runtime.InteropServices;

namespace MayhemTcp;

[StructLayout(LayoutKind.Sequential)]
internal struct StartupOptions
{
    internal uint Frequency, Rate;
    internal int GainTenths, Ppm;
    internal uint UsbBuffers, Flags;

    internal static StartupOptions Load(Context context)
    {
        var p = context.GetSharedPreferences("radio", FileCreationMode.Private)!;
        return new StartupOptions {
            Frequency = checked((uint)p.GetLong("frequency", 100_000_000)),
            Rate = checked((uint)p.GetLong("rate", 2_048_000)),
            GainTenths = p.GetInt("gain", 320), Ppm = p.GetInt("ppm", 0),
            UsbBuffers = checked((uint)p.GetInt("usb", 16)),
            Flags = checked((uint)p.GetInt("flags", 0))
        };
    }

    internal static void Show(Activity activity)
    {
        var options = Load(activity);
        var body = new LinearLayout(activity) { Orientation = Orientation.Vertical };
        var padding = (int)(16 * activity.Resources!.DisplayMetrics!.Density);
        body.SetPadding(padding, padding, padding, padding);
        body.AddView(new TextView(activity) { Text = "Applied on the next server start. Client commands can override these settings." });
        EditText Field(string label, string value, InputTypes type = InputTypes.ClassNumber)
        {
            body.AddView(new TextView(activity) { Text = label });
            var field = new EditText(activity) { Text = value, InputType = type, ContentDescription = label };
            body.AddView(field);
            return field;
        }
        CheckBox Check(string label, uint flag)
        {
            var check = new CheckBox(activity) { Text = label, Checked = (options.Flags & flag) != 0 };
            body.AddView(check);
            return check;
        }
        var frequency = Field("Frequency (Hz)", options.Frequency.ToString());
        var rate = Field("Output sample rate (S/s)", options.Rate.ToString());
        var gain = Field("Manual gain (dB)", (options.GainTenths / 10m).ToString(CultureInfo.InvariantCulture), InputTypes.ClassNumber | InputTypes.NumberFlagDecimal);
        var ppm = Field("Clock correction (PPM)", options.Ppm.ToString(), InputTypes.ClassNumber | InputTypes.NumberFlagSigned);
        var usb = Field("USB buffers (1–64)", options.UsbBuffers.ToString());
        var offset = Check("Offset tuning (move the centre spike)", 1);
        var test = Check("Test pattern instead of radio IQ", 2);
        var allowBias = Check("Allow client antenna power (bias tee)", 4);
        var bias = Check("Start with antenna power on", 8);
        body.AddView(new TextView(activity) { Text = "Enable antenna power only for equipment that accepts DC power on the RF connector." });
        var error = new TextView(activity);
        body.AddView(error);
        var scroll = new ScrollView(activity);
        scroll.AddView(body);
        var dialog = new AlertDialog.Builder(activity).SetTitle("Radio settings")!
            .SetView(scroll)!.SetPositiveButton("Save", (_, _) => { })!
            .SetNegativeButton("Cancel", (_, _) => { })!.Create()!;
        dialog.Show();
        dialog.GetButton((int)DialogButtonType.Positive)!.Click += (_, _) =>
        {
            if (!uint.TryParse(frequency.Text, out var f) || f < 1_000_000 ||
                !uint.TryParse(rate.Text, out var r) || r < 225_001 || r > 3_200_000 ||
                !decimal.TryParse(gain.Text, NumberStyles.Number, CultureInfo.InvariantCulture, out var g) || g < 0 || g > 102 ||
                !int.TryParse(ppm.Text, out var p) || p < -1000 || p > 1000 ||
                !int.TryParse(usb.Text, out var b) || b < 1 || b > 64)
            { error.Text = "Use frequency 1 MHz–4.294967295 GHz, rate 225001–3200000, gain 0–102 dB, PPM -1000–1000, and 1–64 USB buffers."; return; }
            if (bias.Checked && !allowBias.Checked)
            { error.Text = "Allow antenna power before enabling it at startup."; return; }
            var flags = (offset.Checked ? 1 : 0) | (test.Checked ? 2 : 0) | (allowBias.Checked ? 4 : 0) | (bias.Checked ? 8 : 0);
            activity.GetSharedPreferences("radio", FileCreationMode.Private)!.Edit()!
                .PutLong("frequency", f)!.PutLong("rate", r)!
                .PutInt("gain", (int)decimal.Round(g * 10, 0, MidpointRounding.AwayFromZero))!
                .PutInt("ppm", p)!.PutInt("usb", b)!.PutInt("flags", flags)!.Apply();
            ServerState.Log("Radio settings saved for the next server start");
            dialog.Dismiss();
        };
    }
}
