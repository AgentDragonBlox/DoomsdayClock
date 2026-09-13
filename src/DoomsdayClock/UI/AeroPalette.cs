using System.Drawing;
using System.Drawing.Drawing2D;

namespace DoomsdayClock.UI;

/// <summary>
/// Shared colours and small drawing helpers used across every custom
/// control in the settings window, so the "Windows 7 Aero" look stays
/// consistent instead of being redefined slightly differently in each
/// control's OnPaint. See README.md "UI design philosophy" for why this
/// project hand-paints the look rather than depending on OS glass/DWM
/// effects that were deprecated after Windows 7.
/// </summary>
internal static class AeroPalette
{
    internal static readonly Color TitleBarTop = Color.FromArgb(255, 226, 239, 252);
    internal static readonly Color TitleBarMid = Color.FromArgb(255, 121, 174, 229);
    internal static readonly Color TitleBarBottom = Color.FromArgb(255, 43, 100, 158);

    internal static readonly Color WindowBackgroundTop = Color.FromArgb(255, 238, 241, 245);
    internal static readonly Color WindowBackgroundBottom = Color.FromArgb(255, 202, 210, 220);

    internal static readonly Color WellFillTop = Color.FromArgb(255, 250, 251, 252);
    internal static readonly Color WellFillBottom = Color.FromArgb(255, 223, 228, 234);
    internal static readonly Color WellBorder = Color.FromArgb(255, 150, 160, 172);
    internal static readonly Color WellHighlight = Color.FromArgb(120, 255, 255, 255);
    internal static readonly Color WellShadow = Color.FromArgb(90, 90, 100, 112);

    internal static readonly Color AccentBlueTop = Color.FromArgb(255, 150, 205, 250);
    internal static readonly Color AccentBlueBottom = Color.FromArgb(255, 47, 121, 199);

    internal static readonly Color AccentRedTop = Color.FromArgb(255, 250, 150, 150);
    internal static readonly Color AccentRedBottom = Color.FromArgb(255, 199, 47, 47);

    internal static readonly Color AccentGreenTop = Color.FromArgb(255, 165, 235, 165);
    internal static readonly Color AccentGreenBottom = Color.FromArgb(255, 60, 150, 60);

    internal static readonly Color AccentNeutralTop = Color.FromArgb(255, 246, 247, 249);
    internal static readonly Color AccentNeutralBottom = Color.FromArgb(255, 197, 203, 211);

    internal static readonly Color TextDark = Color.FromArgb(255, 35, 42, 50);
    internal static readonly Color TextLight = Color.White;

    internal static readonly Font HeadingFont = new("Segoe UI Semibold", 11f, FontStyle.Bold);
    internal static readonly Font BodyFont = new("Segoe UI", 9.5f);
    internal static readonly Font TitleFont = new("Segoe UI Semibold", 10.5f, FontStyle.Regular);

    /// <summary>Builds a rounded-rectangle path - the basis of every "glass bubble" shape in the UI.</summary>
    internal static GraphicsPath RoundedRect(RectangleF bounds, float radius)
    {
        var path = new GraphicsPath();
        if (radius <= 0f)
        {
            path.AddRectangle(bounds);
            return path;
        }

        float d = radius * 2f;
        path.AddArc(bounds.X, bounds.Y, d, d, 180, 90);
        path.AddArc(bounds.Right - d, bounds.Y, d, d, 270, 90);
        path.AddArc(bounds.Right - d, bounds.Bottom - d, d, d, 0, 90);
        path.AddArc(bounds.X, bounds.Bottom - d, d, d, 90, 90);
        path.CloseFigure();
        return path;
    }

    /// <summary>
    /// The classic Aero "glossy bubble" highlight: a soft white ellipse
    /// covering roughly the top half of a control, giving the impression of
    /// light reflecting off a convex glass surface.
    /// </summary>
    internal static void PaintGlossHighlight(Graphics g, RectangleF bounds)
    {
        var glossRect = new RectangleF(
            bounds.X + bounds.Width * 0.04f,
            bounds.Y + bounds.Height * 0.06f,
            bounds.Width * 0.92f,
            bounds.Height * 0.55f);

        using var path = new GraphicsPath();
        path.AddEllipse(glossRect);

        using var brush = new PathGradientBrush(path)
        {
            CenterColor = Color.FromArgb(150, 255, 255, 255),
            SurroundColors = [Color.FromArgb(0, 255, 255, 255)],
        };

        g.FillPath(brush, path);
    }
}
