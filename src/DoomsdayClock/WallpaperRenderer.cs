using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;

namespace DoomsdayClock;

/// <summary>
/// Renders the countdown wallpaper: a black canvas with large, centred,
/// auto-sized text. This is the one piece of rendering logic shared by both
/// the full settings UI (for its live preview) and the silent
/// <c>--update</c> path (for the real wallpaper) - see the "Why one
/// renderer?" note in README.md.
/// </summary>
public static class WallpaperRenderer
{
    private const float MinFontPoints = 12f;
    private const float MaxFontPoints = 400f;

    // Text is never allowed to stretch closer than this to the screen edge,
    // so the countdown reads comfortably even on ultrawide monitors.
    private const float HorizontalMarginFraction = 0.08f;

    /// <summary>
    /// Renders the wallpaper for <paramref name="today"/> at
    /// <paramref name="canvasSize"/>. The caller owns the returned
    /// <see cref="Bitmap"/> and must dispose it.
    /// </summary>
    public static Bitmap Render(AppConfig config, DateTime today, Size canvasSize)
    {
        canvasSize.Width = Math.Max(canvasSize.Width, 1);
        canvasSize.Height = Math.Max(canvasSize.Height, 1);

        var bitmap = new Bitmap(canvasSize.Width, canvasSize.Height, System.Drawing.Imaging.PixelFormat.Format24bppRgb);

        using (Graphics g = Graphics.FromImage(bitmap))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.Clear(Color.Black);

            string text = BuildCountdownText(config.TargetDate, today);
            Color textColor = ColorInterpolator.GetTextColor(config.CreatedDate, config.TargetDate, today, config.ColorMode);

            float maxWidth = canvasSize.Width * (1f - 2 * HorizontalMarginFraction);
            float maxHeight = canvasSize.Height * 0.5f;

            using Font font = FitFont(g, text, config.FontFamilyName, maxWidth, maxHeight);
            SizeF measured = g.MeasureString(text, font);

            using var brush = new SolidBrush(textColor);
            var origin = new PointF(
                (canvasSize.Width - measured.Width) / 2f,
                (canvasSize.Height - measured.Height) / 2f);

            g.DrawString(text, font, brush, origin);
        }

        return bitmap;
    }

    /// <summary>
    /// Builds the on-screen countdown string, e.g. "47 Days left till
    /// 25-12-2026", handling the singular/zero/overdue edge cases.
    /// </summary>
    public static string BuildCountdownText(DateTime targetDate, DateTime today)
    {
        int daysLeft = (int)(targetDate.Date - today.Date).TotalDays;
        string formattedDate = targetDate.ToString("dd-MM-yyyy");

        return daysLeft switch
        {
            > 1 => $"{daysLeft} Days left till {formattedDate}",
            1 => $"1 Day left till {formattedDate}",
            0 => $"Deadline is today - {formattedDate}",
            _ => $"Deadline passed - {formattedDate}",
        };
    }

    /// <summary>
    /// Binary-searches for the largest point size at which <paramref name="text"/>
    /// still fits within <paramref name="maxWidth"/> x <paramref name="maxHeight"/>,
    /// so the countdown always fills the screen dramatically regardless of
    /// resolution, monitor count, or how long today's text happens to be.
    /// The caller owns the returned <see cref="Font"/> and must dispose it.
    /// </summary>
    private static Font FitFont(Graphics g, string text, string preferredFamily, float maxWidth, float maxHeight)
    {
        float low = MinFontPoints;
        float high = MaxFontPoints;
        Font best = FontResolver.Resolve(preferredFamily, low);

        // ~12 iterations narrows a 12-400pt range to sub-point precision,
        // which is more than enough for a value that only has to look right,
        // not be pixel-exact - and keeps this a bounded, cheap loop.
        for (int i = 0; i < 12; i++)
        {
            float mid = (low + high) / 2f;
            using Font candidate = FontResolver.Resolve(preferredFamily, mid);
            SizeF size = g.MeasureString(text, candidate);

            if (size.Width <= maxWidth && size.Height <= maxHeight)
            {
                best.Dispose();
                best = FontResolver.Resolve(preferredFamily, mid);
                low = mid;
            }
            else
            {
                high = mid;
            }
        }

        return best;
    }
}
