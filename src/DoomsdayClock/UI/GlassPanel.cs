using System.ComponentModel;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace DoomsdayClock.UI;

/// <summary>
/// A rounded "recessed well" panel - the Aero-era grouping container used
/// to visually bucket the settings window into sections ("Deadline",
/// "Wallpaper style", "Preview", "Status"). Painted with a light-to-dark
/// vertical gradient plus a one-pixel highlight/shadow pair on its border
/// to read as gently sunken into the window, rather than a flat modern card.
/// </summary>
public sealed class GlassPanel : Panel
{
    public GlassPanel()
    {
        SetStyle(
            ControlStyles.AllPaintingInWmPaint |
            ControlStyles.UserPaint |
            ControlStyles.OptimizedDoubleBuffer |
            ControlStyles.ResizeRedraw,
            true);

        Padding = new Padding(14);
    }

    [Category("Appearance")]
    [DefaultValue(12)]
    public int CornerRadius { get; set; } = 12;

    [Category("Appearance")]
    public string? Title { get; set; }

    protected override void OnPaint(PaintEventArgs e)
    {
        Graphics g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;

        var bounds = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
        using GraphicsPath path = AeroPalette.RoundedRect(bounds, CornerRadius);

        using (var fill = new LinearGradientBrush(bounds, AeroPalette.WellFillTop, AeroPalette.WellFillBottom, LinearGradientMode.Vertical))
        {
            g.FillPath(fill, path);
        }

        using (var borderPen = new Pen(AeroPalette.WellBorder, 1f))
        {
            g.DrawPath(borderPen, path);
        }

        if (!string.IsNullOrEmpty(Title))
        {
            var titleRect = new RectangleF(bounds.X + 14, bounds.Y + 8, bounds.Width - 28, 20);
            TextRenderer.DrawText(
                g,
                Title,
                AeroPalette.HeadingFont,
                Rectangle.Round(titleRect),
                AeroPalette.TextDark,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);
        }
    }
}
