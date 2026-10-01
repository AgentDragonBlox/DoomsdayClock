using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;
using DoomsdayClock.Native;

namespace DoomsdayClock.UI;

/// <summary>
/// The app's only window: a borderless, hand-themed "Aero glass" settings
/// panel where the user picks a target date and a colour mode, previews the
/// result, and activates/deactivates the daily background update.
///
/// Layout is done with fixed, manually-computed <see cref="Control.Bounds"/>
/// rather than WinForms' anchor/dock/TableLayoutPanel machinery. For a
/// small, fixed-size, non-resizable utility window with a handful of
/// controls, hand-placed bounds are easier to read top-to-bottom, cheaper to
/// lay out (no layout-engine passes on every paint), and avoid the
/// auto-layout engine fighting with the custom-painted controls above.
/// </summary>
public sealed class SettingsForm : Form
{
    private const int CornerRadius = 16;
    private const int TitleBarHeight = 42;

    private readonly AppConfig _originalConfig;
    private readonly DateTimePicker _dtpTargetDate;
    private readonly GlassButton _btnWhite;
    private readonly GlassButton _btnGradient;
    private readonly PictureBox _preview;
    private readonly Label _statusLabel;
    private readonly GlassButton _btnSave;
    private readonly GlassButton _btnDisable;

    private ColorMode _selectedMode;

    public SettingsForm()
    {
        _originalConfig = AppConfig.Load() ?? new AppConfig();
        _selectedMode = _originalConfig.ColorMode;

        // -- Form chrome ---------------------------------------------------
        FormBorderStyle = FormBorderStyle.None;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(460, 640);
        MaximizeBox = false;
        MinimizeBox = true;
        Text = "Doomsday Clock";
        BackColor = AeroPalette.WindowBackgroundTop;
        DoubleBuffered = true;
        Font = AeroPalette.BodyFont;

        var titleBar = BuildTitleBar();
        Controls.Add(titleBar);

        int y = TitleBarHeight + 16;
        const int margin = 16;
        int contentWidth = ClientSize.Width - margin * 2;

        var deadlinePanel = new GlassPanel { Title = "Deadline", Bounds = new Rectangle(margin, y, contentWidth, 96) };
        var deadlineLabel = new Label
        {
            Text = "Count down to:",
            Font = AeroPalette.BodyFont,
            ForeColor = AeroPalette.TextDark,
            AutoSize = true,
            Location = new Point(14, 36),
        };
        _dtpTargetDate = new DateTimePicker
        {
            Format = DateTimePickerFormat.Custom,
            CustomFormat = "dd MMMM yyyy",
            Font = new Font("Segoe UI", 12f, FontStyle.Bold),
            Location = new Point(14, 58),
            Width = contentWidth - 28,
            Value = _originalConfig.TargetDate,
        };
        _dtpTargetDate.ValueChanged += (_, _) => RefreshPreview();
        deadlinePanel.Controls.Add(deadlineLabel);
        deadlinePanel.Controls.Add(_dtpTargetDate);
        Controls.Add(deadlinePanel);
        y += deadlinePanel.Height + 14;

        var stylePanel = new GlassPanel { Title = "Wallpaper text colour", Bounds = new Rectangle(margin, y, contentWidth, 96) };
        _btnWhite = new GlassButton
        {
            Text = "Keep it white",
            Bounds = new Rectangle(14, 40, (contentWidth - 28 - 10) / 2, 40),
        };
        _btnGradient = new GlassButton
        {
            Text = "Fade to red near deadline",
            Bounds = new Rectangle(_btnWhite.Right + 10, 40, (contentWidth - 28 - 10) / 2, 40),
        };
        _btnWhite.Click += (_, _) => SetColorMode(ColorMode.StaticWhite);
        _btnGradient.Click += (_, _) => SetColorMode(ColorMode.GradientToRed);
        stylePanel.Controls.Add(_btnWhite);
        stylePanel.Controls.Add(_btnGradient);
        Controls.Add(stylePanel);
        y += stylePanel.Height + 14;

        int previewHeight = 220;
        var previewPanel = new GlassPanel { Title = "Live preview", Bounds = new Rectangle(margin, y, contentWidth, previewHeight) };
        _preview = new PictureBox
        {
            Bounds = new Rectangle(14, 34, contentWidth - 28, previewHeight - 48),
            BackColor = Color.Black,
            SizeMode = PictureBoxSizeMode.Zoom,
        };
        previewPanel.Controls.Add(_preview);
        Controls.Add(previewPanel);
        y += previewPanel.Height + 14;

        var statusPanel = new GlassPanel { Title = "Daily update status", Bounds = new Rectangle(margin, y, contentWidth, 64) };
        _statusLabel = new Label
        {
            Font = AeroPalette.BodyFont,
            ForeColor = AeroPalette.TextDark,
            AutoSize = false,
            Bounds = new Rectangle(14, 32, contentWidth - 28, 24),
        };
        statusPanel.Controls.Add(_statusLabel);
        Controls.Add(statusPanel);
        y += statusPanel.Height + 18;

        int buttonWidth = (contentWidth - 20) / 3;
        _btnSave = new GlassButton { Text = "Save & Activate", Accent = GlassAccent.Green, Bounds = new Rectangle(margin, y, buttonWidth, 38) };
        _btnDisable = new GlassButton { Text = "Disable Updates", Accent = GlassAccent.Red, Bounds = new Rectangle(_btnSave.Right + 10, y, buttonWidth, 38) };
        var btnClose = new GlassButton { Text = "Close", Accent = GlassAccent.Neutral, Bounds = new Rectangle(_btnDisable.Right + 10, y, buttonWidth, 38) };

        _btnSave.Click += (_, _) => SaveAndActivate();
        _btnDisable.Click += (_, _) => DisableUpdates();
        btnClose.Click += (_, _) => Close();

        Controls.Add(_btnSave);
        Controls.Add(_btnDisable);
        Controls.Add(btnClose);

        ApplyToggleVisuals();
        RefreshPreview();
        RefreshStatusLabel();
    }

    protected override CreateParams CreateParams
    {
        get
        {
            CreateParams cp = base.CreateParams;
            // CS_DROPSHADOW: a free, OS-rendered drop shadow around a
            // borderless window - no custom layered-window shadow bitmap or
            // extra paint work required.
            cp.ClassStyle |= 0x00020000;
            return cp;
        }
    }

    protected override void OnLoad(EventArgs e)
    {
        base.OnLoad(e);
        ApplyRoundedRegion();
    }

    protected override void OnPaintBackground(PaintEventArgs e)
    {
        var bounds = new Rectangle(0, TitleBarHeight, ClientSize.Width, ClientSize.Height - TitleBarHeight);
        using var brush = new LinearGradientBrush(
            bounds,
            AeroPalette.WindowBackgroundTop,
            AeroPalette.WindowBackgroundBottom,
            LinearGradientMode.Vertical);
        e.Graphics.FillRectangle(brush, bounds);
    }

    private void ApplyRoundedRegion()
    {
        using GraphicsPath path = AeroPalette.RoundedRect(new RectangleF(0, 0, ClientSize.Width, ClientSize.Height), CornerRadius);
        Region = new Region(path);
    }

    private Panel BuildTitleBar()
    {
        var bar = new TitleBarPanel { Bounds = new Rectangle(0, 0, ClientSize.Width, TitleBarHeight) };

        var titleLabel = new Label
        {
            Text = "⏳  Doomsday Clock",
            Font = AeroPalette.TitleFont,
            ForeColor = AeroPalette.TextDark,
            AutoSize = false,
            Bounds = new Rectangle(14, 0, 260, TitleBarHeight),
            TextAlign = ContentAlignment.MiddleLeft,
            BackColor = Color.Transparent,
        };

        var btnMinimize = new GlassButton
        {
            Text = "-",
            Accent = GlassAccent.Neutral,
            CornerRadius = 13,
            Size = new Size(26, 26),
            Location = new Point(ClientSize.Width - 68, 8),
        };
        btnMinimize.Click += (_, _) => WindowState = FormWindowState.Minimized;

        var btnClose = new GlassButton
        {
            Text = "x",
            Accent = GlassAccent.Red,
            CornerRadius = 13,
            Size = new Size(26, 26),
            Location = new Point(ClientSize.Width - 34, 8),
        };
        btnClose.Click += (_, _) => Close();

        bar.Controls.Add(titleLabel);
        bar.Controls.Add(btnMinimize);
        bar.Controls.Add(btnClose);

        void StartDrag(object? sender, MouseEventArgs e)
        {
            if (e.Button != MouseButtons.Left)
            {
                return;
            }

            NativeMethods.ReleaseCapture();
            NativeMethods.SendMessage(Handle, NativeMethods.WM_NCLBUTTONDOWN, (IntPtr)NativeMethods.HTCAPTION, IntPtr.Zero);
        }

        bar.MouseDown += StartDrag;
        titleLabel.MouseDown += StartDrag;

        return bar;
    }

    private void SetColorMode(ColorMode mode)
    {
        _selectedMode = mode;
        ApplyToggleVisuals();
        RefreshPreview();
    }

    private void ApplyToggleVisuals()
    {
        _btnWhite.Accent = _selectedMode == ColorMode.StaticWhite ? GlassAccent.Blue : GlassAccent.Neutral;
        _btnGradient.Accent = _selectedMode == ColorMode.GradientToRed ? GlassAccent.Blue : GlassAccent.Neutral;
        _btnWhite.Invalidate();
        _btnGradient.Invalidate();
    }

    private AppConfig BuildConfigFromControls()
    {
        var config = new AppConfig
        {
            TargetDate = _dtpTargetDate.Value.Date,
            ColorMode = _selectedMode,
            FontFamilyName = _originalConfig.FontFamilyName,

            // The gradient's "day zero" only resets when the target date
            // actually changes; editing colour mode alone (or re-saving the
            // same date) preserves progress already made toward red.
            CreatedDate = _dtpTargetDate.Value.Date == _originalConfig.TargetDate.Date
                ? _originalConfig.CreatedDate
                : DateTime.Today,
        };

        return config;
    }

    private void RefreshPreview()
    {
        AppConfig previewConfig = BuildConfigFromControls();
        Bitmap? old = _preview.Image as Bitmap;
        _preview.Image = WallpaperRenderer.Render(previewConfig, DateTime.Today, new Size(1280, 720));
        old?.Dispose();
    }

    private void SaveAndActivate()
    {
        try
        {
            AppConfig config = BuildConfigFromControls();
            config.Save();

            WallpaperSetter.ApplyForToday(config, DateTime.Today);

            string exePath = Environment.ProcessPath ?? Application.ExecutablePath;
            bool scheduled = TaskSchedulerService.CreateOrUpdateTask(exePath);

            if (!scheduled)
            {
                MessageBox.Show(
                    this,
                    "The wallpaper was updated for today, but Windows would not let me install the daily " +
                    "Scheduled Task, so it won't refresh automatically tomorrow. You can re-open this window " +
                    "any time and click \"Save & Activate\" again to retry.",
                    "Doomsday Clock",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Warning);
            }

            RefreshStatusLabel();
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
            MessageBox.Show(
                this,
                $"Couldn't save your settings or update the wallpaper:\n\n{ex.Message}",
                "Doomsday Clock",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
        }
    }

    private void DisableUpdates()
    {
        TaskSchedulerService.RemoveTask();
        RefreshStatusLabel();
    }

    private void RefreshStatusLabel()
    {
        bool installed = TaskSchedulerService.IsTaskInstalled();
        _statusLabel.Text = installed
            ? "Enabled - the wallpaper refreshes automatically every day."
            : "Disabled - click \"Save & Activate\" to turn on daily updates.";
        _statusLabel.ForeColor = installed ? Color.FromArgb(255, 30, 120, 40) : AeroPalette.TextDark;
    }

    /// <summary>
    /// The title bar strip. A tiny subclass (rather than a plain Panel) so
    /// its own OnPaint can draw the 3-stop Aero gradient without a lambda
    /// hijacking the base Panel's Paint event, keeping this file's control
    /// tree declarations above free of inline painting code.
    /// </summary>
    private sealed class TitleBarPanel : Panel
    {
        public TitleBarPanel()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var bounds = new Rectangle(0, 0, Width, Height);
            using var blend = new LinearGradientBrush(bounds, AeroPalette.TitleBarTop, AeroPalette.TitleBarBottom, LinearGradientMode.Vertical)
            {
                Blend = new Blend
                {
                    Positions = [0f, 0.45f, 1f],
                    Factors = [0f, 0.55f, 1f],
                },
            };
            e.Graphics.FillRectangle(blend, bounds);

            using var bottomLine = new Pen(AeroPalette.TitleBarBottom, 1f);
            e.Graphics.DrawLine(bottomLine, 0, Height - 1, Width, Height - 1);
        }
    }
}
