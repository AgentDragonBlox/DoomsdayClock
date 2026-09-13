using System.ComponentModel;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace DoomsdayClock.UI;

/// <summary>
/// The accent colour family a <see cref="GlassButton"/> is painted with.
/// </summary>
public enum GlassAccent
{
    Blue,
    Green,
    Red,
    Neutral,
}

/// <summary>
/// A hand-painted "Aero glass bubble" button: a rounded, glossy, gradient-
/// filled control with hover/press states, in the Windows 7 Aero style the
/// whole app is themed around.
///
/// This is a <see cref="Control"/> subclass rather than a themed
/// <see cref="Button"/> because Windows' visual-styles engine (uxtheme.dll)
/// owns and repaints native Button chrome on top of anything you draw once
/// a theme is active - fighting that with a Button subclass means fragile
/// owner-draw workarounds. Painting a plain Control from scratch is both
/// simpler and cheaper: one OnPaint, no theme handle churn.
/// </summary>
public sealed class GlassButton : Control
{
    private bool _isHovered;
    private bool _isPressed;

    public GlassButton()
    {
        SetStyle(
            ControlStyles.AllPaintingInWmPaint |
            ControlStyles.UserPaint |
            ControlStyles.OptimizedDoubleBuffer |
            ControlStyles.ResizeRedraw |
            ControlStyles.SupportsTransparentBackColor,
            true);

        BackColor = Color.Transparent;
        ForeColor = AeroPalette.TextDark;
        Font = new Font("Segoe UI Semibold", 9.5f, FontStyle.Bold);
        Cursor = Cursors.Hand;
        Size = new Size(140, 34);
    }

    [Category("Appearance")]
    [DefaultValue(GlassAccent.Neutral)]
    public GlassAccent Accent { get; set; } = GlassAccent.Neutral;

    /// <summary>Corner radius in pixels. Larger values read as "more bubble-like".</summary>
    [Category("Appearance")]
    [DefaultValue(10)]
    public int CornerRadius { get; set; } = 10;

    protected override void OnMouseEnter(EventArgs e)
    {
        base.OnMouseEnter(e);
        _isHovered = true;
        Invalidate();
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        base.OnMouseLeave(e);
        _isHovered = false;
        _isPressed = false;
        Invalidate();
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        base.OnMouseDown(e);
        _isPressed = true;
        Invalidate();
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        base.OnMouseUp(e);
        _isPressed = false;
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        Graphics g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;

        var bounds = new RectangleF(0.5f, 0.5f, Width - 1f, Height - 1f);
        using GraphicsPath path = AeroPalette.RoundedRect(bounds, CornerRadius);

        (Color top, Color bottom) = GetAccentColors();

        // Pressed = darken slightly and drop the gloss to read as "pushed
        // in"; hovered = brighten slightly. Both are cheap linear nudges,
        // not new gradients, so painting stays fast during rapid mouse-move.
        if (_isPressed)
        {
            top = Darken(top, 0.12f);
            bottom = Darken(bottom, 0.12f);
        }
        else if (_isHovered)
        {
            top = Lighten(top, 0.08f);
            bottom = Lighten(bottom, 0.08f);
        }

        using (var fill = new LinearGradientBrush(bounds, top, bottom, LinearGradientMode.Vertical))
        {
            g.FillPath(fill, path);
        }

        if (!_isPressed)
        {
            g.SetClip(path);
            AeroPalette.PaintGlossHighlight(g, bounds);
            g.ResetClip();
        }

        using (var pen = new Pen(Darken(bottom, 0.25f), 1f))
        {
            g.DrawPath(pen, path);
        }

        TextRenderer.DrawText(
            g,
            Text,
            Font,
            Rectangle.Round(bounds),
            Accent == GlassAccent.Neutral ? AeroPalette.TextDark : AeroPalette.TextLight,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis | TextFormatFlags.NoPrefix);
    }

    private (Color top, Color bottom) GetAccentColors() => Accent switch
    {
        GlassAccent.Blue => (AeroPalette.AccentBlueTop, AeroPalette.AccentBlueBottom),
        GlassAccent.Green => (AeroPalette.AccentGreenTop, AeroPalette.AccentGreenBottom),
        GlassAccent.Red => (AeroPalette.AccentRedTop, AeroPalette.AccentRedBottom),
        _ => (AeroPalette.AccentNeutralTop, AeroPalette.AccentNeutralBottom),
    };

    private static Color Lighten(Color c, float amount) => Blend(c, Color.White, amount);
    private static Color Darken(Color c, float amount) => Blend(c, Color.Black, amount);

    private static Color Blend(Color c, Color toward, float amount)
    {
        int r = (int)(c.R + (toward.R - c.R) * amount);
        int g = (int)(c.G + (toward.G - c.G) * amount);
        int b = (int)(c.B + (toward.B - c.B) * amount);
        return Color.FromArgb(c.A, Math.Clamp(r, 0, 255), Math.Clamp(g, 0, 255), Math.Clamp(b, 0, 255));
    }
}
